use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::error;

use crate::config::Config;
use crate::data::models::{
    ClusterSnapshot, ConnectionIssue, ConnectionIssueKind, ResourceProblem, MAX_EVENT_CACHE_ENTRIES,
};
use crate::data::store::{ClusterStore, ResourceSet};
use crate::data::watch::{self, WatchUpdate};
use crate::data::{checks, collector};
use crate::events::{AppEvent, DataEvent, FetchCommand};

/// Quiet period after a watch update before the snapshot is rebuilt, so a
/// burst of changes (a rollout, a node drain) redraws once.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(500);
/// Upper bound on the debounce, so constant churn (events on a busy
/// cluster never go quiet) still redraws, but at most this often.
const WATCH_MAX_DELAY: Duration = Duration::from_secs(2);
const WATCH_CHANNEL_CAPACITY: usize = 1024;
/// Cluster-scoped checks list every APIService, PV and Namespace; their
/// failures are slow-moving, so they run at most this often.
const CLUSTER_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

pub struct Fetcher {
    config: Config,
    tx: mpsc::Sender<AppEvent>,
}

/// Results of the last cluster-scoped checks, reused between runs.
#[derive(Default)]
struct ClusterChecks {
    problems: Vec<ResourceProblem>,
    checked_at: Option<Instant>,
    /// Context the results came from; they are cluster-specific and must
    /// not outlive a `kubectl config use-context` made elsewhere.
    context: Option<String>,
}

impl ClusterChecks {
    /// Records the context of results produced by this poll, or drops older
    /// results from another context and forces a rerun on the next poll.
    /// kubectl resolves the context per call, so results are only trusted
    /// when they match the context the poll itself observed.
    fn reconcile_context(&mut self, context: Option<&str>, ran_this_poll: bool) {
        if ran_this_poll {
            self.context = context.map(str::to_string);
        } else if self.context.as_deref() != context {
            *self = Self::default();
        }
    }

    fn is_due(&self) -> bool {
        match self.checked_at {
            Some(at) => at.elapsed() >= CLUSTER_CHECK_INTERVAL,
            None => true,
        }
    }
}

/// Everything a poll reads and updates, carried across loop iterations.
#[derive(Default)]
struct PollState {
    store: Option<ClusterStore>,
    event_cache: HashMap<String, ResourceSet>,
    watches: Watches,
    cluster_checks: ClusterChecks,
}

/// The running `kubectl --watch` streams for one context and namespace.
#[derive(Default)]
struct Watches {
    /// Dropping the set aborts the tasks, which kills their kubectl processes.
    tasks: JoinSet<()>,
    /// Bumped on every start and stop; updates tagged with an older
    /// generation are still draining from stopped watches and are dropped.
    generation: u64,
    target: Option<(Option<String>, String)>,
}

impl Watches {
    /// The store is reused across a context switch (only a namespace change
    /// makes it stale), so the generation bump in `stop()` is what keeps the
    /// old cluster's queued updates, and their meaningless resource versions,
    /// out of it.
    fn ensure(&mut self, context: Option<&str>, namespace: &str, tx: &mpsc::Sender<WatchUpdate>) {
        let target = (context.map(str::to_string), namespace.to_string());
        if self.target.as_ref() == Some(&target) {
            return;
        }
        self.stop();
        watch::spawn_watches(&mut self.tasks, context, namespace, self.generation, tx);
        self.target = Some(target);
    }

    fn stop(&mut self) {
        self.generation += 1;
        self.tasks = JoinSet::new();
        self.target = None;
    }
}

/// When to publish after a watch update, given when the pending batch began.
fn next_flush(now: Instant, batch_started: Instant) -> Instant {
    (now + WATCH_DEBOUNCE).min(batch_started + WATCH_MAX_DELAY)
}

impl Fetcher {
    pub fn new(config: Config, tx: mpsc::Sender<AppEvent>) -> Self {
        Self { config, tx }
    }

    pub async fn run(self, mut cmd_rx: mpsc::Receiver<FetchCommand>) {
        let mut interval_secs = self.config.refresh_interval_secs;
        let mut interval = aligned_interval(interval_secs);
        let mut current_namespace = self.config.namespace.clone();
        let mut state = PollState::default();

        let (watch_tx, mut watch_rx) = mpsc::channel::<WatchUpdate>(WATCH_CHANNEL_CAPACITY);
        let mut flush_at: Option<Instant> = None;
        let mut batch_started: Option<Instant> = None;

        let mut log_cancel: Option<oneshot::Sender<()>> = None;
        let mut log_task: Option<tokio::task::JoinHandle<()>> = None;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if current_namespace.trim().is_empty() {
                        continue;
                    }
                    flush_at = None;
                    batch_started = None;
                    self.poll(&current_namespace, &mut state, &watch_tx).await;
                }
                Some(update) = watch_rx.recv() => {
                    if update.generation != state.watches.generation {
                        continue;
                    }
                    if let Some(store) = state.store.as_mut() {
                        store.apply(update);
                        let now = Instant::now();
                        let started = *batch_started.get_or_insert(now);
                        flush_at = Some(next_flush(now, started));
                    }
                }
                _ = tokio::time::sleep_until(flush_at.unwrap_or_else(Instant::now)), if flush_at.is_some() => {
                    flush_at = None;
                    batch_started = None;
                    if let Some(store) = &state.store {
                        let snapshot = store.snapshot(self.config.node_pool_filter.as_deref(), false);
                        let _ = self.tx.send(AppEvent::Data(DataEvent::Refreshed(snapshot))).await;
                    }
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        FetchCommand::RefreshAll { namespace } => {
                            match self.resolve_namespace(namespace).await {
                                Ok(resolved) => current_namespace = resolved,
                                Err(issue) => {
                                    let _ = self.tx.send(AppEvent::Data(DataEvent::ConnectionState(Some(issue)))).await;
                                    continue;
                                }
                            }
                            interval.reset();
                            flush_at = None;
                    batch_started = None;
                            self.poll(&current_namespace, &mut state, &watch_tx).await;
                        }
                        FetchCommand::UpdateRefreshInterval { namespace, interval_secs: new_interval_secs } => {
                            match self.resolve_namespace(namespace).await {
                                Ok(resolved) => current_namespace = resolved,
                                Err(issue) => {
                                    let _ = self.tx.send(AppEvent::Data(DataEvent::ConnectionState(Some(issue)))).await;
                                    continue;
                                }
                            }
                            interval_secs = new_interval_secs.max(1);
                            interval = aligned_interval(interval_secs);
                            flush_at = None;
                    batch_started = None;
                            self.poll(&current_namespace, &mut state, &watch_tx).await;
                        }
                        FetchCommand::StartLogStream {
                            stream_id,
                            pod,
                            namespace,
                            container,
                            previous,
                        } => {
                            stop_log_stream(&mut log_cancel, &mut log_task).await;
                            let (tx, rx) = oneshot::channel::<()>();
                            log_cancel = Some(tx);
                            let event_tx = self.tx.clone();
                            log_task = Some(tokio::spawn(collector::inherit_context(stream_logs(
                                stream_id,
                                pod,
                                namespace,
                                container,
                                previous,
                                event_tx,
                                rx,
                            ))));
                        }
                        FetchCommand::StopLogStream => {
                            stop_log_stream(&mut log_cancel, &mut log_task).await;
                        }
                        FetchCommand::FetchNamespaces => {
                            self.fetch_namespaces().await;
                        }
                        FetchCommand::ExportPods { cluster_name, namespace, path } => {
                            self.export_pods(cluster_name, namespace, path).await;
                        }
                        FetchCommand::ExportLogs { path, lines } => {
                            let message = match write_log_file(&lines, &path) {
                                Ok(count) => format!("Exported {count} log lines to {path}"),
                                Err(e) => format!("Log export failed: {e}"),
                            };
                            let _ = self.tx.send(AppEvent::Data(DataEvent::ExportResult { message })).await;
                        }
                    }
                }
            }
        }
    }

    /// Lists everything, replaces the store with the result and publishes a
    /// snapshot, then makes sure watches are streaming for `namespace`.
    async fn poll(
        &self,
        namespace: &str,
        state: &mut PollState,
        watch_tx: &mpsc::Sender<WatchUpdate>,
    ) {
        let PollState {
            store,
            event_cache,
            watches,
            cluster_checks,
        } = state;
        let run_cluster_checks = cluster_checks.is_due();
        let (
            nodes_result,
            node_top,
            workloads,
            pods_result,
            pod_top,
            events_result,
            context_result,
            check_results,
            cluster_problems,
        ) = tokio::join!(
            collector::list_nodes(),
            collector::top_nodes(),
            collector::fetch_workload_summaries(namespace),
            collector::list_pods(namespace),
            collector::top_pods(namespace),
            collector::list_events(namespace),
            collector::fetch_current_context(),
            checks::run_custom_resource_checks(namespace, &self.config.crd_checks),
            async {
                if run_cluster_checks {
                    Some(checks::run_cluster_checks().await)
                } else {
                    None
                }
            },
        );
        let checks_ran = cluster_problems.is_some();
        if let Some(problems) = cluster_problems {
            cluster_checks.problems = problems;
            cluster_checks.checked_at = Some(Instant::now());
        }

        let stale = match store {
            Some(store) => store.namespace != namespace,
            None => true,
        };
        if stale {
            // Watches for the previous namespace must not patch the new store,
            // even if this poll fails before new watches start.
            watches.stop();
            *store = Some(ClusterStore::new(namespace));
        }
        let Some(store) = store.as_mut() else {
            return;
        };

        let mut errors = Vec::new();
        let mut connection_issue: Option<ConnectionIssue> = None;
        let (context_name, context_issue) = match context_result {
            Ok(context) => (Some(context), None),
            Err(e) => {
                errors.push(format!("Context: {e}"));
                (None, collector::classify_kubectl_error(&e))
            }
        };
        let cache_key = event_cache_key(context_name.as_deref(), namespace);

        store.nodes_visible = nodes_result.is_ok();
        match nodes_result {
            Ok(list) => store.nodes.replace(list),
            Err(e) => {
                errors.push(format!("Nodes: {e}"));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                store.nodes.clear();
            }
        }

        match pods_result {
            Ok(list) => store.pods.replace(list),
            Err(e) => {
                error!("Failed to fetch pods: {}", e);
                errors.push(format!("Pods: {e}"));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                store.pods.clear();
            }
        }

        match events_result {
            Ok(list) => {
                store.events.replace(list);
                let newest = collector::newest_events(store.events.values());
                event_cache.insert(
                    cache_key.clone(),
                    ResourceSet::unlisted(newest.into_iter().cloned()),
                );
            }
            Err(e) => {
                error!("Failed to fetch events: {}", e);
                errors.push(format!("Events: {e}"));
                connection_issue = prioritize_connection_issue(
                    connection_issue,
                    collector::classify_kubectl_error(&e),
                );
                store.events = event_cache.get(&cache_key).cloned().unwrap_or_default();
            }
        }
        if event_cache.len() > MAX_EVENT_CACHE_ENTRIES {
            event_cache.retain(|k, _| k == &cache_key);
        }

        errors.extend(check_results.warnings);
        store.node_top = node_top;
        store.pod_top = pod_top;
        store.workloads = workloads;
        cluster_checks.reconcile_context(context_name.as_deref(), checks_ran);
        store.problems = cluster_checks.problems.clone();
        store.problems.extend(check_results.problems);
        store.errors = errors;
        store.context_name = context_name;
        store.polled_at = Some(Instant::now().into_std());

        connection_issue = prioritize_connection_issue(connection_issue, context_issue);
        if connection_issue.is_some() && !store.has_data() {
            watches.stop();
            let _ = self
                .tx
                .send(AppEvent::Data(DataEvent::ConnectionState(connection_issue)))
                .await;
            return;
        }

        let _ = self
            .tx
            .send(AppEvent::Data(DataEvent::ConnectionState(connection_issue)))
            .await;
        let snapshot = store.snapshot(self.config.node_pool_filter.as_deref(), true);
        let _ = self
            .tx
            .send(AppEvent::Data(DataEvent::Refreshed(snapshot)))
            .await;

        if self.config.watch {
            watches.ensure(store.context_name.as_deref(), namespace, watch_tx);
        }
    }

    async fn fetch_namespaces(&self) {
        match collector::fetch_namespaces().await {
            Ok(namespaces) => {
                let _ = self
                    .tx
                    .send(AppEvent::Data(DataEvent::Namespaces(namespaces)))
                    .await;
            }
            Err(e) => {
                error!("Failed to fetch namespaces: {}", e);
                let issue = collector::classify_kubectl_error(&e);
                let _ = self
                    .tx
                    .send(AppEvent::Data(DataEvent::ConnectionState(issue)))
                    .await;
                let _ = self
                    .tx
                    .send(AppEvent::Data(DataEvent::Error(format!("Namespaces: {e}"))))
                    .await;
            }
        }
    }

    async fn resolve_namespace(&self, namespace: String) -> Result<String, ConnectionIssue> {
        if !namespace.trim().is_empty() {
            return Ok(namespace);
        }

        match collector::fetch_current_namespace().await {
            Ok(resolved) if !resolved.trim().is_empty() => Ok(resolved),
            // No namespace set in context (common for AKS, EKS, GKE, minikube, docker-desktop,
            // colima, etc.) — fall back to "default" so the app works out of the box.
            Ok(_) => Ok("default".to_string()),
            Err(err) => {
                // Only block on hard failures (kubectl missing, no context configured).
                // For any other error, fall back to "default" so local/cloud setups work.
                match collector::classify_kubectl_error(&err) {
                    Some(issue)
                        if matches!(
                            issue.kind,
                            ConnectionIssueKind::KubectlMissing | ConnectionIssueKind::NoContext
                        ) =>
                    {
                        Err(issue)
                    }
                    _ => Ok("default".to_string()),
                }
            }
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
                    coverage: Default::default(),
                    resource_problems: vec![],
                    metrics_sampled: true,
                };
                match write_pods_csv(&snapshot, &path) {
                    Ok(count) => {
                        format!("Exported {count} pods to {path} (cluster: {cluster_label})")
                    }
                    Err(e) => format!("Export failed: {e}"),
                }
            }
            Err(e) => format!("Export failed: {e}"),
        };
        let _ = self
            .tx
            .send(AppEvent::Data(DataEvent::ExportResult { message }))
            .await;
    }
}

async fn stop_log_stream(
    log_cancel: &mut Option<oneshot::Sender<()>>,
    log_task: &mut Option<tokio::task::JoinHandle<()>>,
) {
    if let Some(tx) = log_cancel.take() {
        let _ = tx.send(());
    }
    if let Some(task) = log_task.take() {
        // A task may be blocked waiting to send a log line through the bounded
        // event channel. Abort guarantees it has stopped before a replacement
        // stream begins, eliminating concurrent writers during rapid switching.
        task.abort();
        let _ = task.await;
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

fn export_path(path: &str) -> Result<&std::path::Path, String> {
    use std::path::{Component, Path};

    let candidate = Path::new(path);
    let mut components = candidate.components();
    let is_plain_filename =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if !is_plain_filename || path.trim().is_empty() {
        return Err(format!(
            "Invalid export path `{path}`: use a plain filename (no directories, `..`, or absolute paths)"
        ));
    }

    Ok(candidate)
}

fn write_pods_csv(snapshot: &ClusterSnapshot, path: &str) -> Result<usize, String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let candidate = export_path(path)?;
    // `create_new` refuses every existing entry, including a symlink. This
    // prevents a filename inside the working directory from being redirected
    // to an arbitrary target and avoids overwriting an existing export.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(candidate)
        .map_err(|e| format!("Failed to create export `{path}`: {e}"))?;

    fn format_cpu(millicores: u64) -> String {
        if millicores == 0 {
            "-".to_string()
        } else if millicores >= 1000 {
            let value = millicores as f64 / 1000.0;
            if (value.fract() - 0.0).abs() < f64::EPSILON {
                format!("{}c", value as u64)
            } else {
                format!("{value:.1}c")
            }
        } else {
            format!("{millicores}m")
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
                format!("{value:.1}Gi")
            }
        } else {
            format!("{mb}Mi")
        }
    }

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

fn write_log_file(lines: &[String], path: &str) -> Result<usize, String> {
    use std::fs::OpenOptions;
    use std::io::Write;

    let candidate = export_path(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(candidate)
        .map_err(|e| format!("Failed to create export `{path}`: {e}"))?;

    for line in lines {
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }

    Ok(lines.len())
}

fn log_args(pod: &str, namespace: &str, container: Option<&str>, previous: bool) -> Vec<String> {
    let mut args = vec![
        "logs".to_string(),
        "-n".to_string(),
        namespace.to_string(),
        pod.to_string(),
    ];
    if let Some(container) = container {
        args.push("-c".to_string());
        args.push(container.to_string());
    }
    args.extend(["--tail=100".to_string(), "--timestamps=true".to_string()]);
    if previous {
        args.push("--previous".to_string());
    } else {
        args.push("-f".to_string());
    }
    args
}

async fn stream_logs(
    stream_id: u64,
    pod: String,
    namespace: String,
    container: Option<String>,
    previous: bool,
    tx: mpsc::Sender<AppEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
) {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    let log_args = log_args(&pod, &namespace, container.as_deref(), previous);
    let log_arg_refs: Vec<&str> = log_args.iter().map(String::as_str).collect();
    if let Err(e) = collector::ensure_readonly_kubectl_args("kubectl", &log_arg_refs) {
        let _ = tx
            .send(AppEvent::Data(DataEvent::LogStreamError {
                stream_id,
                message: format!("Rejected log stream command: {e}"),
            }))
            .await;
        return;
    }

    let mut command = tokio::process::Command::new("kubectl");
    if let Some(context) = collector::context_override() {
        command.args(["--context", &context]);
    }
    command
        .args(&log_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // `stop_log_stream` may abort this task while it is blocked sending an
        // event. Ensure dropping the child cannot leave `kubectl logs -f`
        // running in the background.
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = tx
                .send(AppEvent::Data(DataEvent::LogStreamError {
                    stream_id,
                    message: format!("Failed to stream logs: {e}"),
                }))
                .await;
            return;
        }
    };

    let stderr_task = child.stderr.take().map(|mut stderr| {
        tokio::spawn(async move {
            let mut output = String::new();
            let _ = stderr.read_to_string(&mut output).await;
            output
        })
    });
    let mut cancelled = false;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    cancelled = true;
                    break;
                },
                result = lines.next_line() => {
                    match result {
                        Ok(Some(line)) => {
                            let _ = tx
                                .send(AppEvent::Data(DataEvent::LogLine { stream_id, line }))
                                .await;
                        }
                        _ => break,
                    }
                }
            }
        }
    }

    let status = if cancelled {
        let _ = child.kill().await;
        None
    } else {
        child.wait().await.ok()
    };
    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if status.is_some_and(|status| !status.success()) {
        let detail = stderr.trim();
        let message = if detail.is_empty() {
            "kubectl logs exited with an error".to_string()
        } else {
            format!("kubectl logs: {detail}")
        };
        let _ = tx
            .send(AppEvent::Data(DataEvent::LogStreamError {
                stream_id,
                message,
            }))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::{event_cache_key, export_path, log_args, write_log_file, write_pods_csv};

    #[test]
    fn cluster_check_results_do_not_survive_a_context_switch() {
        use super::ClusterChecks;
        use crate::data::models::{IncidentSeverity, ResourceProblem};

        let mut checks = ClusterChecks {
            problems: vec![ResourceProblem {
                kind: "APIService".to_string(),
                name: "v1beta1.metrics.k8s.io".to_string(),
                namespace: None,
                reason: "APIServiceUnavailable".to_string(),
                severity: IncidentSeverity::Critical,
                message: String::new(),
            }],
            checked_at: Some(tokio::time::Instant::now()),
            context: None,
        };
        checks.reconcile_context(Some("staging"), true);
        checks.reconcile_context(Some("staging"), false);
        assert_eq!(checks.problems.len(), 1);
        assert!(!checks.is_due());

        checks.reconcile_context(Some("prod"), false);
        assert!(checks.problems.is_empty());
        assert!(checks.is_due());
    }
    use crate::data::models::{ClusterSnapshot, HealthScore};

    fn empty_snapshot() -> ClusterSnapshot {
        ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![],
            events: vec![],
            incident_buckets: vec![],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: std::time::Instant::now(),
            error: None,
            context_name: None,
            coverage: Default::default(),
            resource_problems: vec![],
            metrics_sampled: true,
        }
    }

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

    #[test]
    fn write_pods_csv_rejects_path_traversal() {
        let snapshot = empty_snapshot();
        assert!(write_pods_csv(&snapshot, "../../etc/passwd").is_err());
        assert!(write_pods_csv(&snapshot, "/etc/passwd").is_err());
        assert!(write_pods_csv(&snapshot, "sub/dir/out.csv").is_err());
        assert!(write_pods_csv(&snapshot, "  ").is_err());
    }

    #[test]
    fn export_path_only_accepts_a_single_filename() {
        assert!(export_path("pods.csv").is_ok());
        assert!(export_path("./pods.csv").is_err());
        assert!(export_path("exports/pods.csv").is_err());
        assert!(export_path("../pods.csv").is_err());
    }

    #[test]
    fn current_log_args_select_container_and_follow_with_timestamps() {
        assert_eq!(
            log_args("api-0", "payments", Some("sidecar"), false),
            vec![
                "logs",
                "-n",
                "payments",
                "api-0",
                "-c",
                "sidecar",
                "--tail=100",
                "--timestamps=true",
                "-f",
            ]
        );
    }

    #[test]
    fn previous_log_args_request_snapshot_without_following() {
        assert_eq!(
            log_args("api-0", "payments", None, true),
            vec![
                "logs",
                "-n",
                "payments",
                "api-0",
                "--tail=100",
                "--timestamps=true",
                "--previous",
            ]
        );
    }

    #[test]
    fn write_log_file_rejects_path_traversal() {
        assert!(write_log_file(&[], "../logs.txt").is_err());
    }
}
