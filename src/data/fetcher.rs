use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing::error;

use crate::config::Config;
use crate::data::models::{ClusterEvent, ClusterSnapshot, ConnectionIssue, ConnectionIssueKind};
use crate::data::{collector, health, incidents};
use crate::events::{AppEvent, DataEvent, FetchCommand};

pub struct Fetcher {
    config: Config,
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl Fetcher {
    pub fn new(config: Config, tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        Self { config, tx }
    }

    pub async fn run(self, mut cmd_rx: mpsc::UnboundedReceiver<FetchCommand>) {
        let mut interval_secs = self.config.refresh_interval_secs;
        let mut interval = aligned_interval(interval_secs);
        let mut current_namespace = self.config.namespace.clone();
        let mut event_cache: HashMap<String, Vec<ClusterEvent>> = HashMap::new();

        let mut log_cancel: Option<oneshot::Sender<()>> = None;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if current_namespace.trim().is_empty() {
                        continue;
                    }
                    self.fetch_all(&current_namespace, &mut event_cache).await;
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        FetchCommand::RefreshAll { namespace } => {
                            match self.resolve_namespace(namespace).await {
                                Ok(resolved) => current_namespace = resolved,
                                Err(issue) => {
                                    let _ = self.tx.send(AppEvent::Data(DataEvent::ConnectionState(Some(issue))));
                                    continue;
                                }
                            }
                            interval.reset();
                            self.fetch_all(&current_namespace, &mut event_cache).await;
                        }
                        FetchCommand::UpdateRefreshInterval { namespace, interval_secs: new_interval_secs } => {
                            match self.resolve_namespace(namespace).await {
                                Ok(resolved) => current_namespace = resolved,
                                Err(issue) => {
                                    let _ = self.tx.send(AppEvent::Data(DataEvent::ConnectionState(Some(issue))));
                                    continue;
                                }
                            }
                            interval_secs = new_interval_secs.max(1);
                            interval = aligned_interval(interval_secs);
                            self.fetch_all(&current_namespace, &mut event_cache).await;
                        }
                        FetchCommand::StartLogStream { pod, namespace } => {
                            if let Some(tx) = log_cancel.take() {
                                let _ = tx.send(());
                            }
                            let (tx, rx) = oneshot::channel::<()>();
                            log_cancel = Some(tx);
                            let event_tx = self.tx.clone();
                            tokio::spawn(stream_logs(pod, namespace, event_tx, rx));
                        }
                        FetchCommand::StopLogStream => {
                            if let Some(tx) = log_cancel.take() {
                                let _ = tx.send(());
                            }
                        }
                        FetchCommand::FetchNamespaces => {
                            self.fetch_namespaces().await;
                        }
                        FetchCommand::ExportPods { cluster_name, namespace, path } => {
                            self.export_pods(cluster_name, namespace, path).await;
                        }
                    }
                }
            }
        }
    }

    async fn fetch_all(
        &self,
        namespace: &str,
        event_cache: &mut HashMap<String, Vec<ClusterEvent>>,
    ) {
        let (nodes_result, workloads_result, pods_result, events_result, context_result) = tokio::join!(
            collector::fetch_node_metrics(),
            collector::fetch_workload_summaries(namespace),
            collector::fetch_pod_info(namespace),
            collector::fetch_events(namespace),
            collector::fetch_current_context(),
        );
        let mut errors = Vec::new();
        let mut connection_issue: Option<ConnectionIssue> = None;
        let (context_name, context_issue) = match context_result {
            Ok(context) => (Some(context), None),
            Err(e) => {
                errors.push(format!("Context: {}", e));
                (None, collector::classify_kubectl_error(&e))
            }
        };
        let cache_key = event_cache_key(context_name.as_deref(), namespace);

        let nodes = match nodes_result {
            Ok(n) => n,
            Err(e) => {
                errors.push(format!("Nodes: {}", e));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                vec![]
            }
        };

        let mut workloads = match workloads_result {
            Ok(workloads) => workloads,
            Err(e) => {
                error!("Failed to fetch workloads: {}", e);
                errors.push(format!("Workloads: {}", e));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                vec![]
            }
        };

        let pods = match pods_result {
            Ok(pods) => pods,
            Err(e) => {
                error!("Failed to fetch pods: {}", e);
                errors.push(format!("Pods: {}", e));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                vec![]
            }
        };

        let events = match events_result {
            Ok(events) => events,
            Err(e) => {
                error!("Failed to fetch events: {}", e);
                errors.push(format!("Events: {}", e));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                event_cache.get(&cache_key).cloned().unwrap_or_default()
            }
        };

        connection_issue = prioritize_connection_issue(connection_issue, context_issue);

        let has_usable_data =
            !nodes.is_empty() || !workloads.is_empty() || !pods.is_empty() || !events.is_empty();
        if connection_issue.is_some() && !has_usable_data {
            let _ = self
                .tx
                .send(AppEvent::Data(DataEvent::ConnectionState(connection_issue)));
            return;
        }

        collector::attach_workload_events(&mut workloads, &events);

        let health_score = health::calculate_health(&nodes, &pods, &events);
        let incident_buckets = incidents::build_incident_buckets(&nodes, &pods, &events);
        let error_msg = if errors.is_empty() {
            None
        } else {
            Some(errors.join(" | "))
        };

        let _ = self
            .tx
            .send(AppEvent::Data(DataEvent::ConnectionState(connection_issue)));
        let snapshot = ClusterSnapshot {
            nodes,
            workloads,
            pods,
            events: events.clone(),
            incident_buckets,
            health: health_score,
            fetched_at: std::time::Instant::now(),
            error: error_msg,
            context_name: context_name.clone(),
        };

        let _ = self.tx.send(AppEvent::Data(DataEvent::Refreshed(snapshot)));
        event_cache.insert(cache_key, events);
    }

    async fn fetch_namespaces(&self) {
        match collector::fetch_namespaces().await {
            Ok(namespaces) => {
                let _ = self
                    .tx
                    .send(AppEvent::Data(DataEvent::Namespaces(namespaces)));
            }
            Err(e) => {
                error!("Failed to fetch namespaces: {}", e);
                let issue = collector::classify_kubectl_error(&e);
                let _ = self
                    .tx
                    .send(AppEvent::Data(DataEvent::ConnectionState(issue)));
                let _ = self.tx.send(AppEvent::Data(DataEvent::Error(format!(
                    "Namespaces: {}",
                    e
                ))));
            }
        }
    }

    async fn resolve_namespace(&self, namespace: String) -> Result<String, ConnectionIssue> {
        if !namespace.trim().is_empty() {
            return Ok(namespace);
        }

        match collector::fetch_current_namespace().await {
            Ok(resolved) if !resolved.trim().is_empty() => Ok(resolved),
            Ok(_) => Err(ConnectionIssue {
                kind: ConnectionIssueKind::NamespaceUnavailable,
                namespace: None,
                detail: "No namespace is configured in the current kubectl context.".to_string(),
            }),
            Err(err) => Err(
                collector::classify_kubectl_error(&err).unwrap_or(ConnectionIssue {
                    kind: ConnectionIssueKind::NamespaceUnavailable,
                    namespace: None,
                    detail: "No namespace is configured in the current kubectl context."
                        .to_string(),
                }),
            ),
        }
    }

    async fn export_pods(&self, cluster_name: Option<String>, namespace: String, path: String) {
        let cluster_label = match cluster_name {
            Some(cluster_name) => cluster_name,
            None => collector::fetch_current_context()
                .await
                .unwrap_or_else(|_| "unknown".to_string()),
        };
        let message = match collector::fetch_pod_info(&namespace).await {
            Ok(pods) => {
                let snapshot = ClusterSnapshot {
                    nodes: vec![],
                    workloads: vec![],
                    pods,
                    events: vec![],
                    incident_buckets: vec![],
                    health: crate::data::models::HealthScore {
                        score: 100,
                        grade: 'A',
                        critical_nodes: 0,
                        critical_pods: 0,
                        total_restarts: 0,
                    },
                    fetched_at: std::time::Instant::now(),
                    error: None,
                    context_name: Some(cluster_label.clone()),
                };
                match write_pods_csv(&snapshot, &path) {
                    Ok(count) => format!(
                        "Exported {} pods to {} (cluster: {})",
                        count, path, cluster_label
                    ),
                    Err(e) => format!("Export failed: {}", e),
                }
            }
            Err(e) => format!("Export failed: {}", e),
        };
        let _ = self
            .tx
            .send(AppEvent::Data(DataEvent::ExportResult { message }));
    }
}

fn prioritize_connection_issue(
    current: Option<ConnectionIssue>,
    next: Option<ConnectionIssue>,
) -> Option<ConnectionIssue> {
    match (current, next) {
        (Some(current), Some(next)) => {
            if connection_issue_priority(&next) >= connection_issue_priority(&current) {
                Some(next)
            } else {
                Some(current)
            }
        }
        (None, Some(next)) => Some(next),
        (Some(current), None) => Some(current),
        (None, None) => None,
    }
}

fn connection_issue_priority(issue: &ConnectionIssue) -> u8 {
    match issue.kind {
        ConnectionIssueKind::KubectlMissing => 4,
        ConnectionIssueKind::NoContext => 3,
        ConnectionIssueKind::NamespaceUnavailable => 2,
        ConnectionIssueKind::Generic => 1,
    }
}

fn event_cache_key(context_name: Option<&str>, namespace: &str) -> String {
    format!("{}\0{}", context_name.unwrap_or("<unknown>"), namespace)
}

fn aligned_interval(interval_secs: u64) -> tokio::time::Interval {
    let first_tick =
        tokio::time::Instant::now() + Duration::from_secs(secs_until_next_boundary(interval_secs));
    tokio::time::interval_at(first_tick, Duration::from_secs(interval_secs))
}

fn secs_until_next_boundary(interval_secs: u64) -> u64 {
    let now = chrono::Local::now();
    let epoch_secs = now.timestamp() as u64;
    let elapsed_in_window = epoch_secs % interval_secs;
    if elapsed_in_window == 0 {
        interval_secs
    } else {
        interval_secs - elapsed_in_window
    }
}

fn write_pods_csv(snapshot: &ClusterSnapshot, path: &str) -> Result<usize, String> {
    use std::fs::File;
    use std::io::Write;

    fn format_cpu(millicores: u64) -> String {
        if millicores == 0 {
            "-".to_string()
        } else if millicores >= 1000 {
            let value = millicores as f64 / 1000.0;
            if (value.fract() - 0.0).abs() < f64::EPSILON {
                format!("{}c", value as u64)
            } else {
                format!("{:.1}c", value)
            }
        } else {
            format!("{}m", millicores)
        }
    }

    fn format_memory(mb: u64) -> String {
        if mb == 0 {
            "-".to_string()
        } else if mb >= 1024 {
            let value = mb as f64 / 1024.0;
            if (value.fract() - 0.0).abs() < f64::EPSILON {
                format!("{}Gi", value as u64)
            } else {
                format!("{:.1}Gi", value)
            }
        } else {
            format!("{}Mi", mb)
        }
    }

    let mut file = File::create(path).map_err(|e| e.to_string())?;

    writeln!(
        file,
        "status,pod,cpu_pct,cpu_use,cpu_req,cpu_lim,mem_pct,mem_use,mem_req,mem_lim,restarts,age"
    )
    .map_err(|e| e.to_string())?;

    for pod in &snapshot.pods {
        let status_str = match pod.status {
            crate::data::models::HealthStatus::Critical => "Critical",
            crate::data::models::HealthStatus::Warning => "Warning",
            crate::data::models::HealthStatus::Elevated => "Elevated",
            crate::data::models::HealthStatus::Healthy => "Healthy",
        };
        let phase_status = format!("{} {}", status_str, pod.phase);

        writeln!(
            file,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            phase_status,
            pod.name,
            pod.cpu_pct,
            format_cpu(pod.cpu_millicores),
            format_cpu(pod.cpu_request_millicores),
            format_cpu(pod.cpu_limit_millicores),
            pod.memory_pct,
            format_memory(pod.memory_mb),
            format_memory(pod.memory_request_mb),
            format_memory(pod.memory_limit_mb),
            pod.restarts,
            pod.age,
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(snapshot.pods.len())
}

async fn stream_logs(
    pod: String,
    namespace: String,
    tx: mpsc::UnboundedSender<AppEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let log_args = ["logs", "-n", &namespace, &pod, "--tail=100", "-f"];
    if let Err(e) = collector::ensure_readonly_kubectl_args("kubectl", &log_args) {
        let _ = tx.send(AppEvent::Data(DataEvent::Error(format!(
            "Rejected log stream command: {}",
            e
        ))));
        return;
    }

    let mut child = match tokio::process::Command::new("kubectl")
        .args(log_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(AppEvent::Data(DataEvent::Error(format!(
                "Failed to stream logs: {}",
                e
            ))));
            return;
        }
    };

    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    let _ = child.kill().await;
                    break;
                }
                result = lines.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            let _ = tx.send(AppEvent::Data(DataEvent::LogLine(line)));
                        }
                        _ => break,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::event_cache_key;

    #[test]
    fn event_cache_key_includes_context_and_namespace() {
        assert_ne!(
            event_cache_key(Some("cluster-a"), "default"),
            event_cache_key(Some("cluster-b"), "default")
        );
        assert_ne!(
            event_cache_key(Some("cluster-a"), "default"),
            event_cache_key(Some("cluster-a"), "payments")
        );
    }
}
