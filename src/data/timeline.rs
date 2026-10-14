use chrono::{DateTime, FixedOffset};

use crate::data::models::{
    ClusterEvent, ClusterSnapshot, ConditionStatus, EventType, IncidentSeverity, NodeMetric,
};

/// A single entry in the incident timeline, representing a significant cluster event.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineEntry {
    pub timestamp: String,
    pub kind: TimelineEntryKind,
    pub severity: IncidentSeverity,
    pub namespace: Option<String>,
    pub resource_name: String,
    pub message: String,
    pub details: Vec<(String, String)>,
    pub is_precursor: bool,
}

/// The kind of timeline entry, used for filtering and categorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineEntryKind {
    Event,
    ContainerRestart,
    OOMKilled,
    CrashLoop,
    NodeConditionChange,
    SchedulingFailure,
    ImagePullFailure,
    Rollout,
}

impl TimelineEntryKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Event => "Event",
            Self::ContainerRestart => "Restart",
            Self::OOMKilled => "OOMKilled",
            Self::CrashLoop => "CrashLoop",
            Self::NodeConditionChange => "NodeCondition",
            Self::SchedulingFailure => "SchedulingFail",
            Self::ImagePullFailure => "ImagePullFail",
            Self::Rollout => "Rollout",
        }
    }
}

/// A group of related timeline entries, clustered by workload/namespace and time window.
/// Used for correlation analysis; will be exposed to future GPUI frontend.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TimelineGroup {
    pub workload_key: String,
    pub namespace: Option<String>,
    pub entries: Vec<TimelineEntry>,
    pub first_timestamp: String,
    pub last_timestamp: String,
    pub is_precursor: bool,
}

/// Type of rollout detected in an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RolloutType {
    NewRevision,
    ImageChange,
    ScaledUp,
    ScaledDown,
    Failed,
}

/// Detect deployment/replicaset rollout events from cluster events.
fn detect_rollout_event(event: &ClusterEvent) -> Option<(RolloutType, Vec<(String, String)>)> {
    let reason = event.reason.as_str();
    let message = event.message.to_ascii_lowercase();
    let kind = event.kind.as_str();

    // Only process Deployment, ReplicaSet, StatefulSet, DaemonSet events
    if !matches!(kind, "Deployment" | "ReplicaSet" | "StatefulSet" | "DaemonSet") {
        return None;
    }

    // Detect new revision / rollout
    if reason == "ScalingReplicaSet"
        || message.contains("scaled up")
        || message.contains("scaled down")
    {
        let rollout_type = if message.contains("scaled up") {
            RolloutType::ScaledUp
        } else {
            RolloutType::ScaledDown
        };

        return Some((
            rollout_type,
            vec![
                ("Reason".to_string(), event.reason.clone()),
                ("Message".to_string(), event.message.clone()),
            ],
        ));
    }

    // Detect image changes (common in deployment updates)
    if message.contains("image") && (message.contains("updated") || message.contains("changed")) {
        return Some((
            RolloutType::ImageChange,
            vec![
                ("Reason".to_string(), event.reason.clone()),
                ("Message".to_string(), event.message.clone()),
            ],
        ));
    }

    // Detect new revision creation
    if message.contains("revision") || reason == "NewRevision" || reason == "RollingUpdate" {
        return Some((
            RolloutType::NewRevision,
            vec![
                ("Reason".to_string(), event.reason.clone()),
                ("Message".to_string(), event.message.clone()),
            ],
        ));
    }

    // Detect failed rollouts
    if reason == "ProgressDeadlineExceeded"
        || reason == "FailedCreate"
        || message.contains("exceeded its progress deadline")
    {
        return Some((
            RolloutType::Failed,
            vec![
                ("Reason".to_string(), event.reason.clone()),
                ("Message".to_string(), event.message.clone()),
            ],
        ));
    }

    None
}

/// Build a timeline from the current cluster snapshot.
pub fn build_timeline(snapshot: &ClusterSnapshot) -> Vec<TimelineEntry> {
    let mut entries = Vec::new();

    // Add cluster events (warnings prioritized)
    for event in &snapshot.events {
        let severity = match event.event_type {
            EventType::Warning => classify_event_severity(event),
            EventType::Normal => IncidentSeverity::Elevated,
        };

        let kind = classify_event_kind(event);

        entries.push(TimelineEntry {
            timestamp: event.timestamp.clone(),
            kind,
            severity,
            namespace: Some(extract_namespace_from_event(event)),
            resource_name: format!("{}/{}", event.kind, event.name),
            message: event.message.clone(),
            details: vec![
                ("Reason".to_string(), event.reason.clone()),
                ("Count".to_string(), event.count.to_string()),
            ],
            is_precursor: false,
        });
    }

    // Add pod/container incidents (restarts, OOM, crash loops)
    for pod in &snapshot.pods {
        for container in pod.running_containers() {
            if container.restart_count > 0 {
                // Estimate timestamp for restarts (we don't have exact timestamps from kubectl)
                // Use the snapshot timestamp as a best-effort approach
                let estimated_timestamp = chrono::Utc::now().to_rfc3339();

                entries.push(TimelineEntry {
                    timestamp: estimated_timestamp,
                    kind: TimelineEntryKind::ContainerRestart,
                    severity: if container.restart_count > 5 {
                        IncidentSeverity::Critical
                    } else if container.restart_count > 2 {
                        IncidentSeverity::Warning
                    } else {
                        IncidentSeverity::Elevated
                    },
                    namespace: Some(pod.namespace.clone()),
                    resource_name: format!("Pod/{}/{}", pod.name, container.name),
                    message: format!("Container restarted {} times", container.restart_count),
                    details: vec![
                        ("Pod".to_string(), pod.name.clone()),
                        ("Container".to_string(), container.name.clone()),
                        ("Restarts".to_string(), container.restart_count.to_string()),
                    ],
                    is_precursor: false,
                });
            }

            if let Some(reason) = &container.last_termination_reason {
                if reason == "OOMKilled" {
                    let estimated_timestamp = chrono::Utc::now().to_rfc3339();

                    entries.push(TimelineEntry {
                        timestamp: estimated_timestamp,
                        kind: TimelineEntryKind::OOMKilled,
                        severity: IncidentSeverity::Critical,
                        namespace: Some(pod.namespace.clone()),
                        resource_name: format!("Pod/{}/{}", pod.name, container.name),
                        message: "Container terminated by OOM killer".to_string(),
                        details: vec![
                            ("Pod".to_string(), pod.name.clone()),
                            ("Container".to_string(), container.name.clone()),
                            ("Reason".to_string(), reason.clone()),
                        ],
                        is_precursor: false,
                    });
                }
            }
        }

        if pod.crash_looping {
            let estimated_timestamp = chrono::Utc::now().to_rfc3339();

            entries.push(TimelineEntry {
                timestamp: estimated_timestamp,
                kind: TimelineEntryKind::CrashLoop,
                severity: IncidentSeverity::Critical,
                namespace: Some(pod.namespace.clone()),
                resource_name: format!("Pod/{}", pod.name),
                message: "Pod is in CrashLoopBackOff".to_string(),
                details: vec![
                    ("Pod".to_string(), pod.name.clone()),
                    ("Restarts".to_string(), pod.restarts.to_string()),
                ],
                is_precursor: false,
            });
        }
    }

    // Add node condition changes
    for node in &snapshot.nodes {
        add_node_condition_entries(&mut entries, node);
    }

    // Add deployment/replicaset rollout events
    for event in &snapshot.events {
        if let Some((rollout_type, details)) = detect_rollout_event(event) {
            entries.push(TimelineEntry {
                timestamp: event.timestamp.clone(),
                kind: TimelineEntryKind::Rollout,
                severity: match rollout_type {
                    RolloutType::NewRevision | RolloutType::ImageChange => {
                        IncidentSeverity::Elevated
                    }
                    RolloutType::ScaledUp | RolloutType::ScaledDown => IncidentSeverity::Elevated,
                    RolloutType::Failed => IncidentSeverity::Warning,
                },
                namespace: Some(extract_namespace_from_event(event)),
                resource_name: format!("{}/{}", event.kind, event.name),
                message: event.message.clone(),
                details,
                is_precursor: false,
            });
        }
    }

    // Sort by timestamp (newest first)
    entries.sort_by(|a, b| compare_timestamps(&b.timestamp, &a.timestamp));

    entries
}

/// Classify the severity of a cluster event based on its reason and message.
fn classify_event_severity(event: &ClusterEvent) -> IncidentSeverity {
    let reason = event.reason.as_str();
    let message = event.message.to_ascii_lowercase();

    if reason == "OOMKilled"
        || reason == "CrashLoopBackOff"
        || reason == "NodeNotReady"
        || message.contains("oomkilled")
    {
        IncidentSeverity::Critical
    } else if reason == "FailedScheduling"
        || reason == "Evicted"
        || reason == "FailedMount"
        || reason == "FailedAttachVolume"
        || reason == "Unhealthy"
        || reason == "BackOff"
    {
        IncidentSeverity::Warning
    } else {
        IncidentSeverity::Elevated
    }
}

/// Classify the kind of a cluster event for filtering and display.
fn classify_event_kind(event: &ClusterEvent) -> TimelineEntryKind {
    let reason = event.reason.as_str();
    let message = event.message.to_ascii_lowercase();

    if reason == "FailedScheduling" {
        TimelineEntryKind::SchedulingFailure
    } else if reason == "OOMKilled" || message.contains("oomkilled") {
        TimelineEntryKind::OOMKilled
    } else if reason == "CrashLoopBackOff" || reason == "BackOff" {
        TimelineEntryKind::CrashLoop
    } else if message.contains("imagepullbackoff")
        || message.contains("errimagepull")
        || reason == "Failed"
            && (message.contains("pull image") || message.contains("pulling image"))
    {
        TimelineEntryKind::ImagePullFailure
    } else if event.kind == "Node" {
        TimelineEntryKind::NodeConditionChange
    } else {
        TimelineEntryKind::Event
    }
}

/// Extract namespace from an event (defaults to "default" if not present).
fn extract_namespace_from_event(_event: &ClusterEvent) -> String {
    // For now, we don't have namespace info in ClusterEvent directly
    // This would need to be added to the data collection layer
    "default".to_string()
}

/// Add timeline entries for unhealthy node conditions.
fn add_node_condition_entries(entries: &mut Vec<TimelineEntry>, node: &NodeMetric) {
    let timestamp = chrono::Utc::now().to_rfc3339();

    if !node.ready {
        entries.push(TimelineEntry {
            timestamp: timestamp.clone(),
            kind: TimelineEntryKind::NodeConditionChange,
            severity: IncidentSeverity::Critical,
            namespace: None,
            resource_name: format!("Node/{}", node.name),
            message: "Node is NotReady".to_string(),
            details: vec![
                ("Node".to_string(), node.name.clone()),
                ("Ready".to_string(), node.conditions.ready.as_str().to_string()),
            ],
            is_precursor: false,
        });
    }

    if node.conditions.memory_pressure == ConditionStatus::True {
        entries.push(TimelineEntry {
            timestamp: timestamp.clone(),
            kind: TimelineEntryKind::NodeConditionChange,
            severity: IncidentSeverity::Warning,
            namespace: None,
            resource_name: format!("Node/{}", node.name),
            message: "Node is under memory pressure".to_string(),
            details: vec![
                ("Node".to_string(), node.name.clone()),
                (
                    "MemoryPressure".to_string(),
                    node.conditions.memory_pressure.as_str().to_string(),
                ),
            ],
            is_precursor: false,
        });
    }

    if node.conditions.disk_pressure == ConditionStatus::True {
        entries.push(TimelineEntry {
            timestamp: timestamp.clone(),
            kind: TimelineEntryKind::NodeConditionChange,
            severity: IncidentSeverity::Warning,
            namespace: None,
            resource_name: format!("Node/{}", node.name),
            message: "Node is under disk pressure".to_string(),
            details: vec![
                ("Node".to_string(), node.name.clone()),
                ("DiskPressure".to_string(), node.conditions.disk_pressure.as_str().to_string()),
            ],
            is_precursor: false,
        });
    }

    if node.conditions.pid_pressure == ConditionStatus::True {
        entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::NodeConditionChange,
            severity: IncidentSeverity::Warning,
            namespace: None,
            resource_name: format!("Node/{}", node.name),
            message: "Node is under PID pressure".to_string(),
            details: vec![
                ("Node".to_string(), node.name.clone()),
                ("PIDPressure".to_string(), node.conditions.pid_pressure.as_str().to_string()),
            ],
            is_precursor: false,
        });
    }
}

/// Compare two RFC3339 timestamps, with fallback to string comparison.
fn compare_timestamps(a: &str, b: &str) -> std::cmp::Ordering {
    match (
        DateTime::<FixedOffset>::parse_from_rfc3339(a),
        DateTime::<FixedOffset>::parse_from_rfc3339(b),
    ) {
        (Ok(a_dt), Ok(b_dt)) => a_dt.cmp(&b_dt),
        _ => a.cmp(b),
    }
}

/// Group timeline entries by workload and time window for better correlation.
/// Entries within `time_window_secs` and sharing a workload prefix are grouped.
/// Exposed for future GPUI frontend use.
#[allow(dead_code)]
pub fn group_timeline_entries(
    entries: &[TimelineEntry],
    time_window_secs: i64,
) -> Vec<TimelineGroup> {
    let mut groups: Vec<TimelineGroup> = Vec::new();

    for entry in entries {
        let workload_key = extract_workload_key(&entry.resource_name);

        // Try to find an existing group that matches
        let matching_group = groups.iter_mut().find(|group| {
            group.workload_key == workload_key
                && group.namespace == entry.namespace
                && timestamp_within_window(
                    &entry.timestamp,
                    &group.last_timestamp,
                    time_window_secs,
                )
        });

        if let Some(group) = matching_group {
            // Add to existing group
            group.entries.push(entry.clone());
            if compare_timestamps(&entry.timestamp, &group.first_timestamp)
                == std::cmp::Ordering::Less
            {
                group.first_timestamp = entry.timestamp.clone();
            }
            if compare_timestamps(&entry.timestamp, &group.last_timestamp)
                == std::cmp::Ordering::Greater
            {
                group.last_timestamp = entry.timestamp.clone();
            }
        } else {
            // Create new group
            groups.push(TimelineGroup {
                workload_key: workload_key.to_string(),
                namespace: entry.namespace.clone(),
                entries: vec![entry.clone()],
                first_timestamp: entry.timestamp.clone(),
                last_timestamp: entry.timestamp.clone(),
                is_precursor: false,
            });
        }
    }

    groups
}

/// Extract a workload key (e.g., "api" from "Pod/api-6d4f9-x2k") for grouping.
#[allow(dead_code)]
fn extract_workload_key(resource_name: &str) -> &str {
    // Remove kind prefix (e.g., "Pod/", "Node/")
    let without_kind = resource_name.split_once('/').map(|(_, name)| name).unwrap_or(resource_name);

    // For pod names like "api-6d4f9-x2k", extract "api"
    // For node names, return as-is
    without_kind.split('-').next().unwrap_or(without_kind)
}

/// Check if a timestamp is within the time window of another timestamp.
#[allow(dead_code)]
fn timestamp_within_window(ts1: &str, ts2: &str, window_secs: i64) -> bool {
    match (
        DateTime::<FixedOffset>::parse_from_rfc3339(ts1),
        DateTime::<FixedOffset>::parse_from_rfc3339(ts2),
    ) {
        (Ok(dt1), Ok(dt2)) => {
            let diff = (dt1.timestamp() - dt2.timestamp()).abs();
            diff <= window_secs
        }
        _ => false,
    }
}

/// Mark entries as precursors to incidents based on timing and relevance.
/// Precursors are events (especially rollouts) that occurred shortly before incidents.
pub fn mark_precursors(entries: &mut [TimelineEntry], precursor_window_secs: i64) {
    // Collect incident metadata first (without borrowing the entries slice)
    let incidents: Vec<(String, String)> = entries
        .iter()
        .filter_map(|entry| {
            if matches!(
                entry.kind,
                TimelineEntryKind::CrashLoop
                    | TimelineEntryKind::OOMKilled
                    | TimelineEntryKind::ImagePullFailure
                    | TimelineEntryKind::SchedulingFailure
            ) && entry.severity == IncidentSeverity::Critical
            {
                Some((
                    entry.timestamp.clone(),
                    extract_workload_key(&entry.resource_name).to_string(),
                ))
            } else {
                None
            }
        })
        .collect();

    // Now mark precursors using the collected incident metadata
    for (incident_timestamp, incident_workload) in incidents {
        for entry in entries.iter_mut() {
            // Skip if already marked
            if entry.is_precursor {
                continue;
            }

            // Check if this entry is a likely precursor
            if is_likely_precursor(
                entry,
                &incident_timestamp,
                &incident_workload,
                precursor_window_secs,
            ) {
                entry.is_precursor = true;
            }
        }
    }
}

/// Determine if an entry is likely a precursor to an incident.
fn is_likely_precursor(
    entry: &TimelineEntry,
    incident_timestamp: &str,
    incident_workload: &str,
    window_secs: i64,
) -> bool {
    // Rollouts and node condition changes are the most likely precursors
    let is_precursor_kind =
        matches!(entry.kind, TimelineEntryKind::Rollout | TimelineEntryKind::NodeConditionChange);

    if !is_precursor_kind {
        return false;
    }

    // Check if the entry happened before the incident and within the time window
    let entry_ts = match DateTime::<FixedOffset>::parse_from_rfc3339(&entry.timestamp) {
        Ok(dt) => dt,
        Err(_) => return false,
    };

    let incident_ts = match DateTime::<FixedOffset>::parse_from_rfc3339(incident_timestamp) {
        Ok(dt) => dt,
        Err(_) => return false,
    };

    // Entry must be before the incident
    if entry_ts >= incident_ts {
        return false;
    }

    // Entry must be within the precursor window
    let time_diff = (incident_ts.timestamp() - entry_ts.timestamp()).abs();
    if time_diff > window_secs {
        return false;
    }

    // For rollouts, check if it's related to the same workload
    if entry.kind == TimelineEntryKind::Rollout {
        let entry_workload = extract_workload_key(&entry.resource_name);
        if entry_workload != incident_workload {
            return false;
        }
    }

    true
}

/// Filter timeline entries by kind, namespace, and/or severity.
pub fn filter_timeline_entries(
    entries: &[TimelineEntry],
    kind_filter: Option<TimelineEntryKind>,
    namespace_filter: Option<&str>,
    min_severity: Option<IncidentSeverity>,
) -> Vec<TimelineEntry> {
    entries
        .iter()
        .filter(|entry| {
            let kind_match = kind_filter.map(|k| k == entry.kind).unwrap_or(true);

            let ns_match = namespace_filter
                .map(|ns| entry.namespace.as_ref().map(|entry_ns| entry_ns == ns).unwrap_or(false))
                .unwrap_or(true);

            let severity_match =
                min_severity.map(|min| entry.severity.rank() >= min.rank()).unwrap_or(true);

            kind_match && ns_match && severity_match
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::{
        ClusterSnapshot, ConditionStatus, ContainerInfo, HealthScore, HealthStatus, NodeConditions,
        PodInfo,
    };
    use std::time::Instant;

    #[test]
    fn build_timeline_includes_warning_events() {
        let snapshot = ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![],
            events: vec![ClusterEvent {
                kind: "Pod".to_string(),
                name: "api".to_string(),
                reason: "FailedScheduling".to_string(),
                message: "0/3 nodes are available".to_string(),
                event_type: EventType::Warning,
                count: 2,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            }],
            incident_buckets: vec![],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: Some("test".to_string()),
        };

        let timeline = build_timeline(&snapshot);

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].kind, TimelineEntryKind::SchedulingFailure);
        assert_eq!(timeline[0].severity, IncidentSeverity::Warning);
        assert!(timeline[0].message.contains("0/3 nodes"));
    }

    #[test]
    fn build_timeline_includes_crash_looping_pods() {
        let mut pod = create_test_pod("api", "default", false);
        pod.crash_looping = true;
        pod.restarts = 10;

        let snapshot = ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![pod],
            events: vec![],
            incident_buckets: vec![],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: Some("test".to_string()),
        };

        let timeline = build_timeline(&snapshot);

        let crash_loop_entry = timeline.iter().find(|e| e.kind == TimelineEntryKind::CrashLoop);
        assert!(crash_loop_entry.is_some());
        assert_eq!(crash_loop_entry.unwrap().severity, IncidentSeverity::Critical);
    }

    #[test]
    fn build_timeline_includes_oom_killed_containers() {
        let mut pod = create_test_pod("api", "default", false);
        pod.containers[0].last_termination_reason = Some("OOMKilled".to_string());

        let snapshot = ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![pod],
            events: vec![],
            incident_buckets: vec![],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: Some("test".to_string()),
        };

        let timeline = build_timeline(&snapshot);

        let oom_entry = timeline.iter().find(|e| e.kind == TimelineEntryKind::OOMKilled);
        assert!(oom_entry.is_some());
        assert_eq!(oom_entry.unwrap().severity, IncidentSeverity::Critical);
    }

    #[test]
    fn build_timeline_includes_node_conditions() {
        let node = NodeMetric {
            name: "node-a".to_string(),
            cpu_millicores: 0,
            memory_mb: 0,
            memory_total_mb: 0,
            memory_pct: 90,
            cpu_pct: 0,
            status: HealthStatus::Critical,
            cpu_capacity: 0,
            memory_capacity_mb: 0,
            unhealthy_conditions: 2,
            ready: false,
            conditions: NodeConditions {
                ready: ConditionStatus::False,
                memory_pressure: ConditionStatus::True,
                disk_pressure: ConditionStatus::False,
                pid_pressure: ConditionStatus::False,
                network_unavailable: ConditionStatus::False,
            },
            cordoned: false,
            draining: false,
            node_info: None,
        };

        let snapshot = ClusterSnapshot {
            nodes: vec![node],
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
            fetched_at: Instant::now(),
            error: None,
            context_name: Some("test".to_string()),
        };

        let timeline = build_timeline(&snapshot);

        let node_entries: Vec<_> =
            timeline.iter().filter(|e| e.kind == TimelineEntryKind::NodeConditionChange).collect();
        assert_eq!(node_entries.len(), 2); // NotReady + MemoryPressure
    }

    #[test]
    fn filter_timeline_entries_by_kind() {
        let entries = vec![
            create_test_entry(TimelineEntryKind::Event, IncidentSeverity::Elevated, None),
            create_test_entry(
                TimelineEntryKind::CrashLoop,
                IncidentSeverity::Critical,
                Some("default"),
            ),
            create_test_entry(
                TimelineEntryKind::SchedulingFailure,
                IncidentSeverity::Warning,
                Some("default"),
            ),
        ];

        let filtered =
            filter_timeline_entries(&entries, Some(TimelineEntryKind::CrashLoop), None, None);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].kind, TimelineEntryKind::CrashLoop);
    }

    #[test]
    fn filter_timeline_entries_by_namespace() {
        let entries = vec![
            create_test_entry(
                TimelineEntryKind::Event,
                IncidentSeverity::Elevated,
                Some("default"),
            ),
            create_test_entry(
                TimelineEntryKind::CrashLoop,
                IncidentSeverity::Critical,
                Some("kube-system"),
            ),
        ];

        let filtered = filter_timeline_entries(&entries, None, Some("default"), None);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].namespace, Some("default".to_string()));
    }

    #[test]
    fn filter_timeline_entries_by_severity() {
        let entries = vec![
            create_test_entry(TimelineEntryKind::Event, IncidentSeverity::Elevated, None),
            create_test_entry(TimelineEntryKind::CrashLoop, IncidentSeverity::Critical, None),
            create_test_entry(
                TimelineEntryKind::SchedulingFailure,
                IncidentSeverity::Warning,
                None,
            ),
        ];

        let filtered =
            filter_timeline_entries(&entries, None, None, Some(IncidentSeverity::Warning));

        assert_eq!(filtered.len(), 2); // Critical and Warning, not Elevated
    }

    #[test]
    fn group_timeline_entries_by_workload_and_time() {
        let entries = vec![
            TimelineEntry {
                timestamp: "2026-03-09T10:00:00Z".to_string(),
                kind: TimelineEntryKind::Event,
                severity: IncidentSeverity::Warning,
                namespace: Some("default".to_string()),
                resource_name: "Pod/api-6d4f9-x2k".to_string(),
                message: "Failed to schedule".to_string(),
                details: vec![],
                is_precursor: false,
            },
            TimelineEntry {
                timestamp: "2026-03-09T10:00:30Z".to_string(),
                kind: TimelineEntryKind::CrashLoop,
                severity: IncidentSeverity::Critical,
                namespace: Some("default".to_string()),
                resource_name: "Pod/api-6d4f9-x2k".to_string(),
                message: "CrashLoopBackOff".to_string(),
                details: vec![],
                is_precursor: false,
            },
            TimelineEntry {
                timestamp: "2026-03-09T11:00:00Z".to_string(),
                kind: TimelineEntryKind::Event,
                severity: IncidentSeverity::Warning,
                namespace: Some("default".to_string()),
                resource_name: "Pod/worker-abc".to_string(),
                message: "Different workload".to_string(),
                details: vec![],
                is_precursor: false,
            },
        ];

        let groups = group_timeline_entries(&entries, 300); // 5 minute window

        // Should have 2 groups: api (2 entries within 30s) and worker (1 entry)
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].workload_key, "api");
        assert_eq!(groups[0].entries.len(), 2);
        assert_eq!(groups[1].workload_key, "worker");
        assert_eq!(groups[1].entries.len(), 1);
    }

    #[test]
    fn extract_workload_key_handles_pod_names() {
        assert_eq!(extract_workload_key("Pod/api-6d4f9-x2k"), "api");
        assert_eq!(extract_workload_key("Pod/worker-0"), "worker");
        assert_eq!(extract_workload_key("Node/node-a"), "node");
    }

    fn create_test_pod(name: &str, namespace: &str, crash_looping: bool) -> PodInfo {
        PodInfo {
            uid: format!("{namespace}-{name}-uid"),
            name: name.to_string(),
            namespace: namespace.to_string(),
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
            oom_killed: false,
            init_containers: vec![],
            sidecar_containers: vec![],
            owner: None,
            metrics_available: true,
            node_name: Some("node-a".to_string()),
            containers: vec![ContainerInfo {
                name: "app".to_string(),
                ready: true,
                restart_count: 0,
                state: "Running".to_string(),
                last_termination_reason: None,
                last_exit_code: None,
                memory_limit_mb: 0,
                started: true,
            }],
        }
    }

    fn create_test_entry(
        kind: TimelineEntryKind,
        severity: IncidentSeverity,
        namespace: Option<&str>,
    ) -> TimelineEntry {
        TimelineEntry {
            timestamp: "2026-03-09T10:00:00Z".to_string(),
            kind,
            severity,
            namespace: namespace.map(String::from),
            resource_name: "Pod/test".to_string(),
            message: "Test message".to_string(),
            details: vec![],
            is_precursor: false,
        }
    }
}
