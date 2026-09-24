use crate::data::models::{
    ClusterEvent, EventType, HealthScore, NodeMetric, PodInfo, GRADE_A_THRESHOLD,
    GRADE_B_THRESHOLD, GRADE_C_THRESHOLD, GRADE_D_THRESHOLD, RESOURCE_PRESSURE_PCT,
};
const ROLLOUT_FAILURE_REASONS: &[&str] = &[
    "ProgressDeadlineExceeded",
    "ReplicaFailure",
    "FailedCreate",
    "FailedDaemonPod",
    "RolloutAborted",
];

/// A share-based penalty: `cap` points once `ratio` reaches `saturation`,
/// scaling linearly below that, and never less than `floor` while anything
/// is affected. Ratios keep a 3,000-pod cluster with three bad pods from
/// grading like a 30-pod cluster with three bad pods; the floor keeps a
/// single failure visible in a very large cluster.
fn share_penalty(affected: usize, total: usize, cap: f32, saturation: f32, floor: f32) -> f32 {
    if affected == 0 || total == 0 {
        return 0.0;
    }
    let ratio = affected as f32 / total as f32;
    (cap * (ratio / saturation).min(1.0)).max(floor.min(cap))
}

/// Scores a snapshot 0–100 from the *share* of the cluster in trouble.
///
/// | Category | Cap | Full penalty at |
/// |---|---|---|
/// | Failing pods (crash loop, OOM, Failed/Unknown) | 30 | 10% of pods |
/// | Unready or Pending pods | 15 | 20% of pods |
/// | Pods at ≥85% of their memory limit | 5 | 20% of pods |
/// | Restarts | 10 | 1 restart per pod on average |
/// | Nodes not ready or with bad conditions | 30 | 25% of nodes |
/// | Nodes at ≥85% memory | 15 | 25% of nodes |
/// | Warning events (incl. scheduling and rollout failures) | 20 | fixed counts |
///
/// Completed Job/CronJob pods and evicted pods are not counted: both are
/// normal leftovers, not current problems.
pub fn calculate_health(
    nodes: &[NodeMetric],
    pods: &[PodInfo],
    events: &[ClusterEvent],
) -> HealthScore {
    let active: Vec<&PodInfo> = pods
        .iter()
        .filter(|pod| !pod.is_completed() && !pod.is_evicted())
        .collect();

    let mut failing_pods = 0usize;
    let mut unready_pods = 0usize;
    let mut memory_pressure_pods = 0usize;
    let mut critical_pods = 0u32;
    let mut total_restarts = 0u32;
    for pod in &active {
        total_restarts = total_restarts.saturating_add(pod.restarts);
        let failing = pod.crash_looping
            || pod.oom_killed
            || matches!(pod.phase.as_str(), "Failed" | "Unknown");
        let unready = !failing && (!pod.is_ready || pod.phase == "Pending");
        let memory_pressure = pod.memory_pct >= RESOURCE_PRESSURE_PCT;
        failing_pods += usize::from(failing);
        unready_pods += usize::from(unready);
        memory_pressure_pods += usize::from(memory_pressure);
        if failing || unready || memory_pressure {
            critical_pods += 1;
        }
    }

    let unhealthy_nodes = nodes
        .iter()
        .filter(|node| !node.ready || node.unhealthy_conditions > 0)
        .count();
    let pressured_nodes = nodes
        .iter()
        .filter(|node| node.memory_pct >= RESOURCE_PRESSURE_PCT)
        .count();
    let critical_nodes = nodes
        .iter()
        .filter(|node| {
            !node.ready || node.unhealthy_conditions > 0 || node.memory_pct >= RESOURCE_PRESSURE_PCT
        })
        .count() as u32;

    let mut warning_events = 0u32;
    let mut failed_scheduling_events = 0u32;
    let mut rollout_failures = 0u32;
    for event in events.iter().filter(|e| e.event_type == EventType::Warning) {
        warning_events += event.count;
        if event.reason == "FailedScheduling" {
            failed_scheduling_events += event.count;
        }
        if is_rollout_failure(event) {
            rollout_failures += event.count;
        }
    }

    let pod_count = active.len();
    let restart_penalty = if pod_count == 0 {
        0.0
    } else {
        10.0 * (total_restarts as f32 / pod_count as f32).min(1.0)
    };
    let event_penalty = (warning_events.min(20) as f32 * 0.5)
        + (failed_scheduling_events.min(10) as f32 * 0.5)
        + (rollout_failures.min(5) as f32 * 2.0);

    let penalty = share_penalty(failing_pods, pod_count, 30.0, 0.10, 5.0)
        + share_penalty(unready_pods, pod_count, 15.0, 0.20, 2.0)
        + share_penalty(memory_pressure_pods, pod_count, 5.0, 0.20, 1.0)
        + restart_penalty
        + share_penalty(unhealthy_nodes, nodes.len(), 30.0, 0.25, 5.0)
        + share_penalty(pressured_nodes, nodes.len(), 15.0, 0.25, 2.0)
        + event_penalty.min(20.0);

    let score = (100.0 - penalty).round().clamp(0.0, 100.0) as u8;

    let grade = if score >= GRADE_A_THRESHOLD {
        'A'
    } else if score >= GRADE_B_THRESHOLD {
        'B'
    } else if score >= GRADE_C_THRESHOLD {
        'C'
    } else if score >= GRADE_D_THRESHOLD {
        'D'
    } else {
        'F'
    };

    HealthScore {
        score,
        grade,
        critical_nodes,
        critical_pods,
        total_restarts,
    }
}

fn is_rollout_failure(event: &ClusterEvent) -> bool {
    if ROLLOUT_FAILURE_REASONS.contains(&event.reason.as_str()) {
        return true;
    }

    let message = event.message.to_ascii_lowercase();
    message.contains("progress deadline exceeded")
        || message.contains("replica failure")
        || message.contains("rollout") && message.contains("failed")
}

#[cfg(test)]
mod tests {
    use super::calculate_health;
    use crate::data::models::{
        ClusterEvent, ConditionStatus, EventType, HealthStatus, NodeConditions, NodeMetric, PodInfo,
    };

    #[test]
    fn penalizes_runtime_and_rollout_failures() {
        let nodes = vec![NodeMetric {
            name: "node-1".to_string(),
            cpu_millicores: 0,
            memory_mb: 0,
            memory_total_mb: 0,
            memory_pct: 40,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
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
        }];
        let pods = vec![PodInfo {
            uid: "api-uid".to_string(),
            name: "api".to_string(),
            namespace: "default".to_string(),
            phase: "Running".to_string(),
            restarts: 3,
            age: "1h".to_string(),
            cpu_millicores: 0,
            cpu_request_millicores: 0,
            cpu_limit_millicores: 0,
            memory_mb: 0,
            memory_request_mb: 0,
            memory_limit_mb: 0,
            memory_request_pct: 0,
            memory_pct: 90,
            cpu_request_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Critical,
            ready_containers: 0,
            total_containers: 1,
            is_ready: false,
            crash_looping: true,
            oom_killed: true,
            node_name: Some("node-1".to_string()),
            containers: vec![],
            status_reason: None,
        }];
        let events = vec![
            ClusterEvent {
                kind: "Pod".to_string(),
                name: "api".to_string(),
                reason: "FailedScheduling".to_string(),
                message: "0/3 nodes are available".to_string(),
                event_type: EventType::Warning,
                count: 2,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            },
            ClusterEvent {
                kind: "Deployment".to_string(),
                name: "api".to_string(),
                reason: "ProgressDeadlineExceeded".to_string(),
                message: "ReplicaSet exceeded progress deadline".to_string(),
                event_type: EventType::Warning,
                count: 1,
                timestamp: "2026-03-09T10:01:00Z".to_string(),
            },
        ];

        let health = calculate_health(&nodes, &pods, &events);

        assert!(health.score < 30, "score was {}", health.score);
        assert_eq!(health.grade, 'F');
        assert_eq!(health.total_restarts, 3);
        assert!(health.critical_nodes >= 1);
        assert!(health.critical_pods >= 1);
    }

    #[test]
    fn healthy_cluster_stays_high() {
        let nodes = vec![NodeMetric {
            name: "node-1".to_string(),
            cpu_millicores: 0,
            memory_mb: 0,
            memory_total_mb: 0,
            memory_pct: 40,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            cpu_capacity: 0,
            memory_capacity_mb: 0,
            unhealthy_conditions: 0,
            ready: true,
            conditions: NodeConditions {
                ready: ConditionStatus::True,
                memory_pressure: ConditionStatus::False,
                disk_pressure: ConditionStatus::False,
                pid_pressure: ConditionStatus::False,
                network_unavailable: ConditionStatus::False,
            },
            cordoned: false,
            draining: false,
            node_info: None,
        }];
        let pods = vec![PodInfo {
            uid: "api-uid".to_string(),
            name: "api".to_string(),
            namespace: "default".to_string(),
            phase: "Running".to_string(),
            restarts: 0,
            age: "1h".to_string(),
            cpu_millicores: 0,
            cpu_request_millicores: 0,
            cpu_limit_millicores: 0,
            memory_mb: 0,
            memory_request_mb: 0,
            memory_limit_mb: 0,
            memory_request_pct: 0,
            memory_pct: 45,
            cpu_request_pct: 0,
            cpu_pct: 0,
            status: HealthStatus::Healthy,
            ready_containers: 1,
            total_containers: 1,
            is_ready: true,
            crash_looping: false,
            oom_killed: false,
            node_name: Some("node-1".to_string()),
            containers: vec![],
            status_reason: None,
        }];

        let health = calculate_health(&nodes, &pods, &[]);

        assert!(health.score >= 95, "score was {}", health.score);
        assert_eq!(health.grade, 'A');
    }
}
