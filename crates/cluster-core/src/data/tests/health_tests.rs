#[cfg(test)]
use crate::data::health::calculate_health;
use crate::data::models::*;

fn create_test_node(memory_pct: u8, ready: bool, unhealthy_conditions: u8) -> NodeMetric {
    NodeMetric {
        name: "test-node".to_string(),
        cpu_millicores: 0,
        memory_mb: 0,
        memory_total_mb: 0,
        memory_pct,
        cpu_pct: 0,
        status: HealthStatus::from_pct(memory_pct),
        cpu_capacity: 0,
        memory_capacity_mb: 0,
        unhealthy_conditions,
        ready,
        conditions: NodeConditions {
            ready: if ready {
                ConditionStatus::True
            } else {
                ConditionStatus::False
            },
            memory_pressure: ConditionStatus::False,
            disk_pressure: ConditionStatus::False,
            pid_pressure: ConditionStatus::False,
            network_unavailable: ConditionStatus::False,
        },
        cordoned: false,
        draining: false,
        node_info: None,
    }
}

fn create_test_pod(
    memory_pct: u8,
    phase: &str,
    is_ready: bool,
    restarts: u32,
    crash_looping: bool,
    oom_killed: bool,
) -> PodInfo {
    PodInfo {
        uid: "test-pod-uid".to_string(),
        name: "test-pod".to_string(),
        namespace: "default".to_string(),
        phase: phase.to_string(),
        restarts,
        age: "1h".to_string(),
        cpu_millicores: 0,
        cpu_request_millicores: 0,
        cpu_limit_millicores: 0,
        memory_mb: 0,
        memory_request_mb: 0,
        memory_limit_mb: 0,
        memory_request_pct: 0,
        memory_pct,
        cpu_request_pct: 0,
        cpu_pct: 0,
        status: HealthStatus::from_pct(memory_pct),
        ready_containers: if is_ready { 1 } else { 0 },
        total_containers: 1,
        is_ready,
        crash_looping,
        oom_killed,
        node_name: Some("test-node".to_string()),
        containers: vec![],
    }
}

fn create_test_event(reason: &str, event_type: EventType, count: u32) -> ClusterEvent {
    ClusterEvent {
        kind: "Pod".to_string(),
        name: "test-pod".to_string(),
        reason: reason.to_string(),
        message: "test message".to_string(),
        event_type,
        count,
        timestamp: "2026-03-09T10:00:00Z".to_string(),
    }
}

#[test]
fn test_critical_node_penalty() {
    let nodes = vec![create_test_node(90, true, 0)];
    let health = calculate_health(&nodes, &[], &[]);

    assert!(health.score < 100, "Critical node should reduce score");
    assert_eq!(health.critical_nodes, 1);
}

#[test]
fn test_unhealthy_node_penalty() {
    let nodes = vec![create_test_node(40, false, 2)];
    let health = calculate_health(&nodes, &[], &[]);

    assert!(health.score < 100, "Unhealthy node should reduce score");
    assert_eq!(health.critical_nodes, 1);
}

#[test]
fn test_critical_pod_penalty() {
    let pods = vec![create_test_pod(90, "Running", true, 0, false, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "Critical pod should reduce score");
    assert_eq!(health.critical_pods, 1);
}

#[test]
fn test_failed_pod_penalty() {
    let pods = vec![create_test_pod(40, "Failed", false, 0, false, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "Failed pod should reduce score");
    assert_eq!(health.critical_pods, 1); // Failed pod counted once (not double-counted)
}

#[test]
fn test_pending_pod_penalty() {
    let pods = vec![create_test_pod(40, "Pending", false, 0, false, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "Pending pod should reduce score");
}

#[test]
fn test_unknown_phase_pod_penalty() {
    let pods = vec![create_test_pod(40, "Unknown", false, 0, false, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "Unknown phase pod should reduce score");
}

#[test]
fn test_crash_looping_penalty() {
    let pods = vec![create_test_pod(40, "Running", false, 5, true, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "Crash looping pod should reduce score");
    assert_eq!(health.total_restarts, 5);
}

#[test]
fn test_oom_killed_penalty() {
    let pods = vec![create_test_pod(40, "Running", false, 0, false, true)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "OOM killed pod should reduce score");
}

#[test]
fn test_restart_penalty() {
    let pods = vec![create_test_pod(40, "Running", true, 10, false, false)];
    let health = calculate_health(&[], &pods, &[]);

    assert!(health.score < 100, "High restarts should reduce score");
    assert_eq!(health.total_restarts, 10);
}

#[test]
fn test_warning_events_penalty() {
    let events = vec![create_test_event("BackOff", EventType::Warning, 5)];
    let health = calculate_health(&[], &[], &events);

    assert!(health.score < 100, "Warning events should reduce score");
}

#[test]
fn test_failed_scheduling_penalty() {
    let events = vec![create_test_event("FailedScheduling", EventType::Warning, 3)];
    let health = calculate_health(&[], &[], &events);

    assert!(
        health.score < 100,
        "Failed scheduling events should reduce score"
    );
}

#[test]
fn test_rollout_failure_penalty() {
    let events = vec![create_test_event(
        "ProgressDeadlineExceeded",
        EventType::Warning,
        2,
    )];
    let health = calculate_health(&[], &[], &events);

    assert!(health.score < 100, "Rollout failures should reduce score");
}

#[test]
fn test_normal_events_no_penalty() {
    let events = vec![create_test_event("Created", EventType::Normal, 10)];
    let health = calculate_health(&[], &[], &events);

    assert_eq!(health.score, 100, "Normal events should not affect score");
}

#[test]
fn test_grade_calculation() {
    // Test grade boundaries
    let nodes = vec![create_test_node(40, true, 0)];
    let pods = vec![create_test_pod(40, "Running", true, 0, false, false)];

    let health = calculate_health(&nodes, &pods, &[]);
    assert_eq!(health.grade, 'A');
}

#[test]
fn test_score_never_negative() {
    // Create a scenario that would push score below 0
    let nodes = vec![
        create_test_node(95, false, 5),
        create_test_node(95, false, 5),
    ];
    let pods = vec![
        create_test_pod(95, "Failed", false, 50, true, true),
        create_test_pod(95, "Failed", false, 50, true, true),
    ];
    let events = vec![
        create_test_event("FailedScheduling", EventType::Warning, 20),
        create_test_event("ProgressDeadlineExceeded", EventType::Warning, 20),
    ];

    let health = calculate_health(&nodes, &pods, &events);
    assert_eq!(health.score, 0, "Score should be clamped to 0");
    assert_eq!(health.grade, 'F');
}

#[test]
fn test_empty_cluster_perfect_score() {
    let health = calculate_health(&[], &[], &[]);

    assert_eq!(health.score, 100);
    assert_eq!(health.grade, 'A');
    assert_eq!(health.critical_nodes, 0);
    assert_eq!(health.critical_pods, 0);
    assert_eq!(health.total_restarts, 0);
}
