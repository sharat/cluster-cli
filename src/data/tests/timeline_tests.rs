use crate::data::models::{
    ClusterEvent, ClusterSnapshot, ConditionStatus, ContainerInfo, EventType, HealthScore,
    HealthStatus, IncidentSeverity, NodeConditions, NodeMetric, PodInfo,
};
use crate::data::timeline::{
    TimelineEntryKind, build_timeline, filter_timeline_entries, group_timeline_entries,
};
use std::time::Instant;

#[test]
fn build_timeline_from_empty_snapshot() {
    let snapshot = create_empty_snapshot();
    let timeline = build_timeline(&snapshot);
    assert_eq!(timeline.len(), 0);
}

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
    let mut pod = create_test_pod("api", "default");
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
    let mut pod = create_test_pod("api", "default");
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
fn build_timeline_includes_container_restarts() {
    let mut pod = create_test_pod("api", "default");
    pod.containers[0].restart_count = 3;

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

    let restart_entry = timeline.iter().find(|e| e.kind == TimelineEntryKind::ContainerRestart);
    assert!(restart_entry.is_some());
    assert!(restart_entry.unwrap().message.contains("restarted 3 times"));
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
    let snapshot = create_snapshot_with_mixed_events();
    let timeline = build_timeline(&snapshot);

    let filtered =
        filter_timeline_entries(&timeline, Some(TimelineEntryKind::CrashLoop), None, None);

    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|e| e.kind == TimelineEntryKind::CrashLoop));
}

#[test]
fn filter_timeline_entries_by_namespace() {
    let snapshot = create_snapshot_with_mixed_namespaces();
    let timeline = build_timeline(&snapshot);

    // Filter by default namespace (pods have namespace info)
    let filtered = filter_timeline_entries(&timeline, None, Some("default"), None);

    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|e| e.namespace.as_deref() == Some("default")));
}

#[test]
fn filter_timeline_entries_by_severity() {
    let snapshot = create_snapshot_with_mixed_events();
    let timeline = build_timeline(&snapshot);

    let filtered = filter_timeline_entries(&timeline, None, None, Some(IncidentSeverity::Critical));

    assert!(!filtered.is_empty());
    assert!(filtered.iter().all(|e| e.severity == IncidentSeverity::Critical));
}

#[test]
fn group_timeline_entries_by_workload_and_time() {
    use crate::data::timeline::TimelineEntry;

    let entries = vec![
        TimelineEntry {
            timestamp: "2026-03-09T10:00:00Z".to_string(),
            kind: TimelineEntryKind::SchedulingFailure,
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
            kind: TimelineEntryKind::SchedulingFailure,
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
fn group_timeline_entries_respects_time_window() {
    use crate::data::timeline::TimelineEntry;

    let entries = vec![
        TimelineEntry {
            timestamp: "2026-03-09T10:00:00Z".to_string(),
            kind: TimelineEntryKind::CrashLoop,
            severity: IncidentSeverity::Critical,
            namespace: Some("default".to_string()),
            resource_name: "Pod/api-1".to_string(),
            message: "First crash".to_string(),
            details: vec![],
            is_precursor: false,
        },
        TimelineEntry {
            timestamp: "2026-03-09T10:10:00Z".to_string(),
            kind: TimelineEntryKind::CrashLoop,
            severity: IncidentSeverity::Critical,
            namespace: Some("default".to_string()),
            resource_name: "Pod/api-2".to_string(),
            message: "Second crash - beyond window".to_string(),
            details: vec![],
            is_precursor: false,
        },
    ];

    // 5 minute window - should create separate groups
    let groups = group_timeline_entries(&entries, 300);
    assert_eq!(groups.len(), 2);
}

// Helper functions

fn create_empty_snapshot() -> ClusterSnapshot {
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
        fetched_at: Instant::now(),
        error: None,
        context_name: Some("test".to_string()),
    }
}

fn create_test_pod(name: &str, namespace: &str) -> PodInfo {
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
        crash_looping: false,
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

fn create_snapshot_with_mixed_events() -> ClusterSnapshot {
    let mut pod1 = create_test_pod("api", "default");
    pod1.crash_looping = true;

    let mut pod2 = create_test_pod("worker", "default");
    pod2.containers[0].restart_count = 3;

    ClusterSnapshot {
        nodes: vec![],
        workloads: vec![],
        pods: vec![pod1, pod2],
        events: vec![ClusterEvent {
            kind: "Pod".to_string(),
            name: "db".to_string(),
            reason: "FailedScheduling".to_string(),
            message: "0/3 nodes".to_string(),
            event_type: EventType::Warning,
            count: 1,
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
    }
}

#[test]
fn build_timeline_includes_deployment_rollouts() {
    let snapshot = ClusterSnapshot {
        nodes: vec![],
        workloads: vec![],
        pods: vec![],
        events: vec![
            ClusterEvent {
                kind: "Deployment".to_string(),
                name: "api".to_string(),
                reason: "ScalingReplicaSet".to_string(),
                message: "Scaled up replica set api-xyz to 3".to_string(),
                event_type: EventType::Normal,
                count: 1,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            },
            ClusterEvent {
                kind: "Deployment".to_string(),
                name: "api".to_string(),
                reason: "NewRevision".to_string(),
                message: "Created new revision api-v2".to_string(),
                event_type: EventType::Normal,
                count: 1,
                timestamp: "2026-03-09T10:01:00Z".to_string(),
            },
        ],
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

    let rollout_entries: Vec<_> =
        timeline.iter().filter(|e| e.kind == TimelineEntryKind::Rollout).collect();

    assert_eq!(rollout_entries.len(), 2);
    assert!(rollout_entries.iter().any(|e| e.message.contains("Scaled up")));
    assert!(rollout_entries.iter().any(|e| e.message.contains("new revision")));
}

#[test]
fn mark_precursors_identifies_rollout_before_crash() {
    use crate::data::timeline::{TimelineEntry, TimelineEntryKind, mark_precursors};

    let mut entries = vec![
        TimelineEntry {
            timestamp: "2026-03-09T10:00:00Z".to_string(),
            kind: TimelineEntryKind::Rollout,
            severity: IncidentSeverity::Elevated,
            namespace: Some("default".to_string()),
            resource_name: "Deployment/api".to_string(),
            message: "Scaled up replica set".to_string(),
            details: vec![],
            is_precursor: false,
        },
        TimelineEntry {
            timestamp: "2026-03-09T10:05:00Z".to_string(),
            kind: TimelineEntryKind::CrashLoop,
            severity: IncidentSeverity::Critical,
            namespace: Some("default".to_string()),
            resource_name: "Pod/api-6d4f9".to_string(),
            message: "CrashLoopBackOff".to_string(),
            details: vec![],
            is_precursor: false,
        },
    ];

    // Mark precursors with 10 minute window
    mark_precursors(&mut entries, 600);

    // The rollout should be marked as a precursor
    assert!(entries[0].is_precursor);
    assert!(!entries[1].is_precursor); // The incident itself is not a precursor
}

#[test]
fn mark_precursors_ignores_unrelated_workloads() {
    use crate::data::timeline::{TimelineEntry, TimelineEntryKind, mark_precursors};

    let mut entries = vec![
        TimelineEntry {
            timestamp: "2026-03-09T10:00:00Z".to_string(),
            kind: TimelineEntryKind::Rollout,
            severity: IncidentSeverity::Elevated,
            namespace: Some("default".to_string()),
            resource_name: "Deployment/worker".to_string(),
            message: "Scaled up replica set".to_string(),
            details: vec![],
            is_precursor: false,
        },
        TimelineEntry {
            timestamp: "2026-03-09T10:05:00Z".to_string(),
            kind: TimelineEntryKind::CrashLoop,
            severity: IncidentSeverity::Critical,
            namespace: Some("default".to_string()),
            resource_name: "Pod/api-6d4f9".to_string(),
            message: "CrashLoopBackOff".to_string(),
            details: vec![],
            is_precursor: false,
        },
    ];

    mark_precursors(&mut entries, 600);

    // The rollout for worker should NOT be marked as precursor for api incident
    assert!(!entries[0].is_precursor);
}

#[test]
fn mark_precursors_respects_time_window() {
    use crate::data::timeline::{TimelineEntry, TimelineEntryKind, mark_precursors};

    let mut entries = vec![
        TimelineEntry {
            timestamp: "2026-03-09T09:00:00Z".to_string(),
            kind: TimelineEntryKind::Rollout,
            severity: IncidentSeverity::Elevated,
            namespace: Some("default".to_string()),
            resource_name: "Deployment/api".to_string(),
            message: "Scaled up replica set".to_string(),
            details: vec![],
            is_precursor: false,
        },
        TimelineEntry {
            timestamp: "2026-03-09T10:05:00Z".to_string(),
            kind: TimelineEntryKind::CrashLoop,
            severity: IncidentSeverity::Critical,
            namespace: Some("default".to_string()),
            resource_name: "Pod/api-6d4f9".to_string(),
            message: "CrashLoopBackOff".to_string(),
            details: vec![],
            is_precursor: false,
        },
    ];

    // Mark precursors with 10 minute window (600 seconds)
    mark_precursors(&mut entries, 600);

    // The rollout is >60 minutes before the crash, so should NOT be marked
    assert!(!entries[0].is_precursor);
}

#[test]
fn detect_image_change_in_rollout() {
    let snapshot = ClusterSnapshot {
        nodes: vec![],
        workloads: vec![],
        pods: vec![],
        events: vec![ClusterEvent {
            kind: "Deployment".to_string(),
            name: "api".to_string(),
            reason: "ImageUpdated".to_string(),
            message: "Image updated from nginx:1.20 to nginx:1.21".to_string(),
            event_type: EventType::Normal,
            count: 1,
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

    let rollout_entries: Vec<_> =
        timeline.iter().filter(|e| e.kind == TimelineEntryKind::Rollout).collect();

    assert_eq!(rollout_entries.len(), 1);
    assert!(rollout_entries[0].message.contains("Image updated"));
}

fn create_snapshot_with_mixed_namespaces() -> ClusterSnapshot {
    let mut pod1 = create_test_pod("api", "default");
    pod1.crash_looping = true; // Make it generate a timeline entry

    let mut pod2 = create_test_pod("coredns", "kube-system");
    pod2.crash_looping = true; // Make it generate a timeline entry

    ClusterSnapshot {
        nodes: vec![],
        workloads: vec![],
        pods: vec![pod1, pod2],
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
    }
}
