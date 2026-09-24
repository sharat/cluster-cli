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

pub fn calculate_health(
    nodes: &[NodeMetric],
    pods: &[PodInfo],
    events: &[ClusterEvent],
) -> HealthScore {
    let mut score: i32 = 100;
    let mut critical_nodes = 0u32;
    let mut critical_pods = 0u32;
    let mut total_restarts = 0u32;
    let mut unhealthy_nodes = 0u32;
    let mut failed_scheduling_events = 0u32;
    let mut warning_events = 0u32;
    let mut rollout_failures = 0u32;

    for node in nodes {
        if node.memory_pct >= RESOURCE_PRESSURE_PCT {
            score = score.saturating_sub(15);
            critical_nodes += 1;
        }
        if !node.ready || node.unhealthy_conditions > 0 {
            unhealthy_nodes += 1;
            score = score.saturating_sub(10);
            score = score.saturating_sub((node.unhealthy_conditions as i32) * 4);
        }
    }

    for pod in pods {
        total_restarts += pod.restarts;

        // Track unique critical pods (avoid double-counting)
        let mut is_critical = false;

        if pod.memory_pct >= RESOURCE_PRESSURE_PCT {
            score = score.saturating_sub(10);
            is_critical = true;
        }

        if pod.phase == "Failed" {
            score = score.saturating_sub(18);
            is_critical = true;
        } else if pod.phase == "Pending" {
            score = score.saturating_sub(6);
        } else if pod.phase == "Unknown" {
            score = score.saturating_sub(10);
            is_critical = true;
        }

        if !pod.is_ready {
            score = score.saturating_sub(8);
            is_critical = true;
        }

        if pod.crash_looping {
            score = score.saturating_sub(20);
            is_critical = true;
        }

        if pod.oom_killed {
            score = score.saturating_sub(15);
            is_critical = true;
        }

        if is_critical {
            critical_pods += 1;
        }
    }

    let restart_penalty = (total_restarts * 2).min(100) as i32;
    score = score.saturating_sub(restart_penalty);

    for event in events {
        if event.event_type != EventType::Warning {
            continue;
        }

        warning_events += event.count;
        if event.reason == "FailedScheduling" {
            failed_scheduling_events += event.count;
        }
        if is_rollout_failure(event) {
            rollout_failures += event.count;
        }
    }

    score = score.saturating_sub((warning_events.min(20) as i32) * 2);
    score = score.saturating_sub((failed_scheduling_events.min(10) as i32) * 3);
    score = score.saturating_sub((rollout_failures.min(10) as i32) * 4);

    let score = score.max(0) as u8;

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
        critical_nodes: critical_nodes + unhealthy_nodes,
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

        assert!(health.score < 20, "score was {}", health.score);
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
        }];

        let health = calculate_health(&nodes, &pods, &[]);

        assert!(health.score >= 95, "score was {}", health.score);
        assert_eq!(health.grade, 'A');
    }
}
