use std::collections::{BTreeSet, HashMap};

use crate::data::models::{
    ClusterEvent, IncidentBucket, IncidentSeverity, IncidentTarget, NodeMetric, PodInfo,
};

pub fn build_incident_buckets(
    nodes: &[NodeMetric],
    pods: &[PodInfo],
    events: &[ClusterEvent],
) -> Vec<IncidentBucket> {
    let mut buckets: HashMap<String, IncidentAccumulator> = HashMap::new();

    for node in nodes {
        if !node.ready {
            add_incident(
                &mut buckets,
                "NodeNotReady",
                IncidentSeverity::Critical,
                IncidentTarget::Node {
                    node_name: node.name.clone(),
                },
                1,
                String::new(),
                Some("node is reporting NotReady".to_string()),
            );
        }
    }

    for pod in pods {
        if pod.crash_looping {
            add_incident(
                &mut buckets,
                "CrashLoopBackOff",
                IncidentSeverity::Critical,
                IncidentTarget::Pod {
                    pod_name: pod.name.clone(),
                },
                1,
                String::new(),
                Some("pod is crash looping".to_string()),
            );
        }

        if pod.oom_killed {
            add_incident(
                &mut buckets,
                "OOMKilled",
                IncidentSeverity::Critical,
                IncidentTarget::Pod {
                    pod_name: pod.name.clone(),
                },
                1,
                String::new(),
                Some("container was terminated by the kernel OOM killer".to_string()),
            );
        }

        for container in &pod.containers {
            let state_lower = container.state.to_ascii_lowercase();

            // Check for image pull failures in the state summary string
            // State format is "Waiting (ImagePullBackOff)" or "Waiting (ErrImagePull)"
            if state_lower.contains("imagepullbackoff") || state_lower.contains("errimagepull") {
                add_incident(
                    &mut buckets,
                    "ImagePullBackOff",
                    IncidentSeverity::Critical,
                    IncidentTarget::Container {
                        pod_name: pod.name.clone(),
                        container_name: container.name.clone(),
                    },
                    1,
                    String::new(),
                    Some("container cannot pull its image".to_string()),
                );
            }

            if !pod.crash_looping && state_lower.contains("crashloopbackoff") {
                add_incident(
                    &mut buckets,
                    "CrashLoopBackOff",
                    IncidentSeverity::Critical,
                    IncidentTarget::Container {
                        pod_name: pod.name.clone(),
                        container_name: container.name.clone(),
                    },
                    1,
                    String::new(),
                    Some("container is repeatedly restarting".to_string()),
                );
            }

            if !pod.oom_killed && container.last_termination_reason.as_deref() == Some("OOMKilled") {
                add_incident(
                    &mut buckets,
                    "OOMKilled",
                    IncidentSeverity::Critical,
                    IncidentTarget::Container {
                        pod_name: pod.name.clone(),
                        container_name: container.name.clone(),
                    },
                    1,
                    String::new(),
                    Some("container was OOMKilled".to_string()),
                );
            }
        }
    }

    for event in events {
        if let Some((reason, severity)) = canonical_event_reason(event) {
            add_incident(
                &mut buckets,
                reason,
                severity,
                event_target(event),
                event.count.max(1),
                event.timestamp.clone(),
                if event.message.is_empty() {
                    None
                } else {
                    Some(event.message.clone())
                },
            );
        }
    }

    let mut buckets: Vec<_> = buckets
        .into_values()
        .map(IncidentAccumulator::into_bucket)
        .collect();
    buckets.sort_by(|a, b| {
        b.severity
            .rank()
            .cmp(&a.severity.rank())
            .then_with(|| b.occurrences.cmp(&a.occurrences))
            .then_with(|| b.affected_resources.len().cmp(&a.affected_resources.len()))
            .then_with(|| b.latest_timestamp.cmp(&a.latest_timestamp))
            .then_with(|| a.reason.cmp(&b.reason))
    });
    buckets
}

fn canonical_event_reason(event: &ClusterEvent) -> Option<(&str, IncidentSeverity)> {
    if event.event_type != crate::data::models::EventType::Warning {
        return None;
    }

    let reason = event.reason.as_str();
    let message = event.message.to_ascii_lowercase();

    if reason == "FailedScheduling" {
        return Some(("FailedScheduling", IncidentSeverity::Warning));
    }

    if reason == "Evicted" || message.contains("evicted") {
        return Some(("Evicted", IncidentSeverity::Warning));
    }

    if reason == "NodeNotReady" || message.contains("node is not ready") {
        return Some(("NodeNotReady", IncidentSeverity::Critical));
    }

    if reason == "BackOff" && message.contains("restarting failed container") {
        return Some(("CrashLoopBackOff", IncidentSeverity::Critical));
    }

    if message.contains("imagepullbackoff")
        || message.contains("errimagepull")
        || message.contains("pull image")
        || message.contains("failed to pull image")
    {
        return Some(("ImagePullBackOff", IncidentSeverity::Critical));
    }

    if reason == "OOMKilled" || message.contains("oomkilled") {
        return Some(("OOMKilled", IncidentSeverity::Critical));
    }

    if reason.is_empty() {
        None
    } else {
        Some((reason, severity_for_reason(reason)))
    }
}

fn severity_for_reason(reason: &str) -> IncidentSeverity {
    match reason {
        "CrashLoopBackOff" | "ImagePullBackOff" | "OOMKilled" | "NodeNotReady" => {
            IncidentSeverity::Critical
        }
        "FailedScheduling" | "Evicted" | "FailedMount" | "FailedAttachVolume" | "Unhealthy" => {
            IncidentSeverity::Warning
        }
        _ => IncidentSeverity::Elevated,
    }
}

fn add_incident(
    buckets: &mut HashMap<String, IncidentAccumulator>,
    reason: &str,
    severity: IncidentSeverity,
    target: IncidentTarget,
    occurrences: u32,
    latest_timestamp: String,
    sample_message: Option<String>,
) {
    let entry = buckets
        .entry(reason.to_string())
        .or_insert_with(|| IncidentAccumulator::new(reason.to_string(), severity));
    entry.severity = entry.severity.max(severity);
    entry.occurrences += occurrences;
    entry.resources.insert(target.display_label());
    entry.targets.insert(target);
    // Compare timestamps properly using DateTime parsing
    if is_timestamp_newer(&latest_timestamp, &entry.latest_timestamp) {
        entry.latest_timestamp = latest_timestamp;
    }
    if entry.sample_message.is_none() {
        entry.sample_message = sample_message;
    }
}

fn is_timestamp_newer(new: &str, existing: &str) -> bool {
    if existing.is_empty() {
        return true;
    }
    if new.is_empty() {
        return false;
    }
    // Try to parse both as RFC3339 for proper comparison
    match (chrono::DateTime::parse_from_rfc3339(new), chrono::DateTime::parse_from_rfc3339(existing)) {
        (Ok(new_dt), Ok(existing_dt)) => new_dt > existing_dt,
        // Fall back to string comparison if parsing fails
        _ => new > existing,
    }
}

struct IncidentAccumulator {
    reason: String,
    severity: IncidentSeverity,
    occurrences: u32,
    resources: BTreeSet<String>,
    targets: BTreeSet<IncidentTarget>,
    latest_timestamp: String,
    sample_message: Option<String>,
}

impl IncidentAccumulator {
    fn new(reason: String, severity: IncidentSeverity) -> Self {
        Self {
            reason,
            severity,
            occurrences: 0,
            resources: BTreeSet::new(),
            targets: BTreeSet::new(),
            latest_timestamp: String::new(),
            sample_message: None,
        }
    }

    fn into_bucket(self) -> IncidentBucket {
        IncidentBucket {
            reason: self.reason,
            severity: self.severity,
            occurrences: self.occurrences,
            targets: self.targets.into_iter().collect(),
            affected_resources: self.resources.into_iter().collect(),
            latest_timestamp: self.latest_timestamp,
            sample_message: self.sample_message,
        }
    }
}

fn event_target(event: &ClusterEvent) -> IncidentTarget {
    match event.kind.as_str() {
        "Node" => IncidentTarget::Node {
            node_name: event.name.clone(),
        },
        "Pod" => IncidentTarget::Pod {
            pod_name: event.name.clone(),
        },
        kind => IncidentTarget::Workload {
            kind: kind.to_string(),
            name: event.name.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::build_incident_buckets;
    use crate::data::models::{
        ClusterEvent, ContainerInfo, ConditionStatus, EventType, HealthStatus, IncidentSeverity,
        IncidentTarget, NodeConditions, NodeMetric, PodInfo,
    };

    #[test]
    fn builds_ranked_buckets_from_pods_nodes_and_events() {
        let nodes = vec![NodeMetric {
            name: "node-a".to_string(),
            cpu_millicores: 0,
            memory_mb: 0,
            memory_total_mb: 0,
            memory_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            cpu_capacity: 0,
            memory_capacity_mb: 0,
            unhealthy_conditions: 0,
            ready: false,
            conditions: NodeConditions {
                ready: ConditionStatus::False,
                memory_pressure: ConditionStatus::False,
                disk_pressure: ConditionStatus::False,
                pid_pressure: ConditionStatus::False,
                network_unavailable: ConditionStatus::False,
            },
            cordoned: false,
            draining: false,
            node_info: None,
        }];
        let pods = vec![
            pod("api", true, false, "Running"),
            pod("worker", false, true, "Waiting (ImagePullBackOff)"),
        ];
        let events = vec![
            ClusterEvent {
                kind: "Pod".to_string(),
                name: "api".to_string(),
                reason: "BackOff".to_string(),
                message: "Back-off restarting failed container".to_string(),
                event_type: EventType::Warning,
                count: 4,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            },
            ClusterEvent {
                kind: "Pod".to_string(),
                name: "pending".to_string(),
                reason: "FailedScheduling".to_string(),
                message: "0/3 nodes are available".to_string(),
                event_type: EventType::Warning,
                count: 2,
                timestamp: "2026-03-09T10:01:00Z".to_string(),
            },
        ];

        let buckets = build_incident_buckets(&nodes, &pods, &events);

        assert_eq!(buckets[0].reason, "CrashLoopBackOff");
        assert_eq!(buckets[0].severity, IncidentSeverity::Critical);
        assert_eq!(buckets[0].occurrences, 5);
        assert!(buckets[0]
            .targets
            .iter()
            .any(|target| matches!(target, IncidentTarget::Pod { pod_name } if pod_name == "api")));
        assert!(buckets.iter().any(|b| b.reason == "NodeNotReady"));
        assert!(buckets.iter().any(|b| b.reason == "ImagePullBackOff"));
        assert!(buckets.iter().any(|b| b.reason == "FailedScheduling"));
    }

    #[test]
    fn preserves_structured_targets_for_workloads_and_containers() {
        let nodes = vec![];
        let pods = vec![pod("api", true, false, "Waiting (ImagePullBackOff)")];
        let events = vec![
            ClusterEvent {
                kind: "Deployment".to_string(),
                name: "api".to_string(),
                reason: "ProgressDeadlineExceeded".to_string(),
                message: "deployment exceeded its progress deadline".to_string(),
                event_type: EventType::Warning,
                count: 1,
                timestamp: "2026-03-09T10:02:00Z".to_string(),
            },
            ClusterEvent {
                kind: "ReplicaSet".to_string(),
                name: "api-7f9c".to_string(),
                reason: "BackOff".to_string(),
                message: "Back-off restarting failed container".to_string(),
                event_type: EventType::Warning,
                count: 1,
                timestamp: "2026-03-09T10:03:00Z".to_string(),
            },
        ];

        let buckets = build_incident_buckets(&nodes, &pods, &events);

        let workload_bucket = buckets
            .iter()
            .find(|bucket| bucket.reason == "ProgressDeadlineExceeded")
            .expect("workload incident should be present");
        assert!(workload_bucket
            .targets
            .iter()
            .any(|target| matches!(target, IncidentTarget::Workload { kind, name } if kind == "Deployment" && name == "api")));

        let container_bucket = buckets
            .iter()
            .find(|bucket| bucket.reason == "ImagePullBackOff")
            .expect("container incident should be present");
        assert!(container_bucket
            .targets
            .iter()
            .any(|target| matches!(target, IncidentTarget::Container { pod_name, container_name } if pod_name == "api" && container_name == "app")));
    }

    #[test]
    fn merges_event_targets_for_same_reason_and_keeps_newest_timestamp() {
        let nodes = vec![];
        let pods = vec![];
        let events = vec![
            ClusterEvent {
                kind: "Pod".to_string(),
                name: "api".to_string(),
                reason: "FailedScheduling".to_string(),
                message: "0/3 nodes are available".to_string(),
                event_type: EventType::Warning,
                count: 1,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            },
            ClusterEvent {
                kind: "Node".to_string(),
                name: "node-a".to_string(),
                reason: "FailedScheduling".to_string(),
                message: "0/3 nodes are available".to_string(),
                event_type: EventType::Warning,
                count: 2,
                timestamp: "2026-03-09T10:05:00Z".to_string(),
            },
        ];

        let buckets = build_incident_buckets(&nodes, &pods, &events);

        assert_eq!(buckets.len(), 1);
        let bucket = &buckets[0];
        assert_eq!(bucket.reason, "FailedScheduling");
        assert_eq!(bucket.occurrences, 3);
        assert_eq!(bucket.latest_timestamp, "2026-03-09T10:05:00Z");
        assert_eq!(
            bucket.affected_resources,
            vec!["Node/node-a".to_string(), "Pod/api".to_string()]
        );
    }

    fn pod(name: &str, crash_looping: bool, oom_killed: bool, state: &str) -> PodInfo {
        PodInfo {
            uid: format!("{name}-uid"),
            name: name.to_string(),
            namespace: "default".to_string(),
            phase: "Running".to_string(),
            restarts: 0,
            age: "1m".to_string(),
            cpu_millicores: 0,
            cpu_request_millicores: 0,
            cpu_limit_millicores: 0,
            memory_mb: 0,
            memory_request_mb: 0,
            memory_limit_mb: 0,
            memory_request_pct: 0,
            memory_pct: 0,
            cpu_request_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            ready_containers: 1,
            total_containers: 1,
            is_ready: true,
            crash_looping,
            oom_killed,
            node_name: Some("node-a".to_string()),
            containers: vec![ContainerInfo {
                name: "app".to_string(),
                ready: true,
                restart_count: 0,
                state: state.to_string(),
                last_termination_reason: oom_killed.then(|| "OOMKilled".to_string()),
                last_exit_code: None,
            }],
        }
    }
}
