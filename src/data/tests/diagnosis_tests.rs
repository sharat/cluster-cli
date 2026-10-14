use crate::data::diagnosis::diagnose_pod;
use crate::data::models::{
    ClusterEvent, ClusterSnapshot, ConditionStatus, ContainerInfo, EventType, HealthScore,
    HealthStatus, IncidentSeverity, NodeConditions, NodeMetric, PodInfo,
};
use std::time::Instant;

#[test]
fn healthy_pod_has_no_findings() {
    let pod = create_test_pod();
    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));
    assert!(findings.is_empty());
}

#[test]
fn crash_loop_explains_exit_code_and_suggests_previous_logs() {
    let mut pod = create_test_pod();
    pod.crash_looping = true;
    pod.containers[0].state = "Waiting (CrashLoopBackOff)".to_string();
    pod.containers[0].restart_count = 7;
    pod.containers[0].last_termination_reason = Some("Error".to_string());
    pod.containers[0].last_exit_code = Some(127);

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, IncidentSeverity::Critical);
    assert!(findings[0].issue.contains("crash-looping (7 restarts)"));
    assert!(findings[0].likely_cause.contains("exit 127"));
    assert_eq!(findings[0].next_command, "kubectl logs api-7d9f -n prod --previous");
}

#[test]
fn oom_killed_reports_memory_limit_and_skips_pressure_warning() {
    let mut pod = create_test_pod();
    pod.oom_killed = true;
    pod.memory_limit_mb = 256;
    pod.containers[0].memory_limit_mb = 256;
    pod.memory_pct = 99;
    pod.containers[0].restart_count = 2;
    pod.containers[0].last_termination_reason = Some("OOMKilled".to_string());
    pod.containers[0].last_exit_code = Some(137);

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert!(findings[0].issue.contains("OOMKilled"));
    assert!(findings[0].likely_cause.contains("its 256 MB limit"));
}

#[test]
fn image_pull_failure_uses_event_message() {
    let mut pod = create_test_pod();
    pod.is_ready = false;
    pod.phase = "Pending".to_string();
    pod.containers[0].ready = false;
    pod.containers[0].state = "Waiting (ImagePullBackOff)".to_string();
    let event = pod_event("Failed", "Failed to pull image \"nginx:typo\": not found");

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![event]));

    assert_eq!(findings.len(), 1, "pending phase should not add a second finding: {findings:?}");
    assert!(findings[0].issue.contains("cannot pull its image"));
    assert!(findings[0].likely_cause.contains("nginx:typo"));
    assert_eq!(findings[0].next_command, "kubectl describe pod api-7d9f -n prod");
}

#[test]
fn image_pull_cause_skips_the_generic_error_event_and_summarizes_the_real_one() {
    let mut pod = create_test_pod();
    pod.is_ready = false;
    pod.phase = "Pending".to_string();
    pod.containers[0].state = "Waiting (ImagePullBackOff)".to_string();
    // Newest first, as kubectl reports from a kind cluster.
    let events = vec![
        pod_event("Failed", "Error: ImagePullBackOff"),
        pod_event(
            "Failed",
            "Failed to pull image \"nonexistent/image:v1\": failed to pull and unpack image \
             \"docker.io/nonexistent/image:v1\": failed to resolve reference: pull access denied, \
             repository does not exist or may require authorization",
        ),
    ];

    let findings = diagnose_pod(&pod, &snapshot(vec![], events));

    assert_eq!(
        findings[0].likely_cause,
        "Image nonexistent/image:v1 does not exist, or the registry needs credentials \
         (imagePullSecrets)"
    );
}

#[test]
fn unschedulable_pod_explains_insufficient_memory() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.is_ready = false;
    pod.node_name = None;
    pod.memory_request_mb = 64000;
    pod.containers.clear();
    let event = pod_event("FailedScheduling", "0/3 nodes are available: 3 Insufficient memory.");

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![event]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].issue, "Pod cannot be scheduled");
    assert!(findings[0].likely_cause.contains("64000 MB"));
    assert!(findings[0].next_command.contains("--field-selector involvedObject.name=api-7d9f"));
}

#[test]
fn probe_failures_and_node_pressure_are_reported_most_severe_first() {
    let mut pod = create_test_pod();
    pod.is_ready = false;
    pod.cpu_limit_millicores = 500;
    pod.cpu_pct = 95;
    let events = vec![
        pod_event("Unhealthy", "Readiness probe failed: HTTP probe failed with statuscode: 503"),
        ClusterEvent { count: 4, ..pod_event("Unhealthy", "Liveness probe failed: timeout") },
    ];
    let node = create_test_node(false);

    let findings = diagnose_pod(&pod, &snapshot(vec![node], events));

    let issues: Vec<&str> = findings.iter().map(|f| f.issue.as_str()).collect();
    assert_eq!(
        issues,
        vec![
            "Node node-a is NotReady",
            "Liveness probe failed 4 time(s)",
            "Readiness probe failed 1 time(s)",
            "CPU at 95% of its limit",
        ]
    );
    assert_eq!(findings[0].next_command, "kubectl describe node node-a");
}

#[test]
fn multi_container_pods_name_the_container_in_log_commands() {
    let mut pod = create_test_pod();
    let mut sidecar = pod.containers[0].clone();
    sidecar.name = "proxy".to_string();
    sidecar.restart_count = 3;
    sidecar.last_termination_reason = Some("Error".to_string());
    sidecar.last_exit_code = Some(1);
    pod.containers.push(sidecar);

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, IncidentSeverity::Elevated);
    assert_eq!(findings[0].next_command, "kubectl logs api-7d9f -n prod -c proxy --previous");
}

#[test]
fn running_but_not_ready_without_other_signals_gets_a_fallback_finding() {
    let mut pod = create_test_pod();
    pod.is_ready = false;

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].issue, "Pod is running but not ready");
}

#[test]
fn events_for_other_pods_are_ignored() {
    let pod = create_test_pod();
    let mut event = pod_event("Unhealthy", "Liveness probe failed: timeout");
    event.name = "other-pod".to_string();

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![event]));

    assert!(findings.is_empty());
}

#[test]
fn oom_cause_uses_the_killed_containers_own_limit() {
    let mut pod = create_test_pod();
    pod.memory_limit_mb = 1024; // pod-wide sum, not the sidecar's limit
    let mut sidecar = pod.containers[0].clone();
    sidecar.name = "proxy".to_string();
    sidecar.memory_limit_mb = 0;
    sidecar.restart_count = 1;
    sidecar.last_termination_reason = Some("OOMKilled".to_string());
    pod.containers[0].memory_limit_mb = 1024;
    pod.containers.push(sidecar);

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].issue, "Container proxy was OOMKilled");
    assert!(findings[0].likely_cause.contains("No memory limit is set"));
}

#[test]
fn quotes_the_latest_matching_event() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.node_name = None;
    pod.containers.clear();
    // fetch_events stores events newest first.
    let events = vec![
        pod_event("FailedScheduling", "0/3 nodes are available: 3 node(s) had untolerated taint"),
        pod_event("FailedScheduling", "0/3 nodes are available: 3 Insufficient memory."),
    ];

    let findings = diagnose_pod(&pod, &snapshot(vec![], events));

    assert_eq!(findings[0].likely_cause, "Nodes have taints the pod does not tolerate");
}

#[test]
fn failing_init_container_is_reported_instead_of_still_starting() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.is_ready = false;
    pod.containers[0].ready = false;
    pod.containers[0].state = "Waiting (PodInitializing)".to_string();
    pod.init_containers = vec![ContainerInfo {
        name: "migrate".to_string(),
        ready: false,
        restart_count: 5,
        state: "Waiting (CrashLoopBackOff)".to_string(),
        last_termination_reason: Some("Error".to_string()),
        last_exit_code: Some(2),
        memory_limit_mb: 0,
    }];

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1, "pending phase should not add a second finding: {findings:?}");
    assert_eq!(findings[0].issue, "Init container migrate is crash-looping (5 restarts)");
    assert_eq!(findings[0].next_command, "kubectl logs api-7d9f -n prod -c migrate --previous");
}

#[test]
fn running_init_container_explains_a_pending_pod() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.is_ready = false;
    pod.containers[0].state = "Waiting (PodInitializing)".to_string();
    let mut init = pod.containers[0].clone();
    init.name = "wait-for-db".to_string();
    init.state = "Running".to_string();
    init.ready = false;
    pod.init_containers = vec![init];

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].likely_cause, "Init container wait-for-db is still running");
}

#[test]
fn init_container_that_completed_after_an_earlier_oom_is_not_reported() {
    let pod = PodInfo {
        init_containers: vec![ContainerInfo {
            name: "migrate".to_string(),
            ready: true,
            restart_count: 1,
            state: "Terminated (Completed)".to_string(),
            // From lastState: the earlier, failed attempt.
            last_termination_reason: Some("OOMKilled".to_string()),
            last_exit_code: Some(137),
            memory_limit_mb: 64,
        }],
        ..create_test_pod()
    };

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn started_native_sidecar_is_not_blamed_for_a_pending_pod() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.is_ready = false;
    pod.containers[0].ready = false;
    pod.containers[0].state = "Waiting (ContainerCreating)".to_string();
    let mut sidecar = pod.containers[0].clone();
    sidecar.name = "istio-proxy".to_string();
    sidecar.state = "Running".to_string();
    sidecar.ready = true;
    pod.sidecar_containers = vec![sidecar];

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert!(findings[0].likely_cause.starts_with("Scheduled; containers are still being created"));
}

#[test]
fn unready_sidecar_explains_a_pending_pod() {
    let mut pod = create_test_pod();
    pod.phase = "Pending".to_string();
    pod.is_ready = false;
    pod.containers[0].ready = false;
    pod.containers[0].state = "Waiting (PodInitializing)".to_string();
    let mut sidecar = pod.containers[0].clone();
    sidecar.name = "istio-proxy".to_string();
    sidecar.state = "Running".to_string();
    sidecar.ready = false;
    pod.sidecar_containers = vec![sidecar];

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].likely_cause,
        "Sidecar container istio-proxy is running but not ready yet"
    );
}

#[test]
fn crash_looping_sidecar_is_reported_with_its_own_logs_command() {
    let mut pod = create_test_pod();
    let mut sidecar = pod.containers[0].clone();
    sidecar.name = "istio-proxy".to_string();
    sidecar.ready = false;
    sidecar.restart_count = 6;
    sidecar.state = "Waiting (CrashLoopBackOff)".to_string();
    sidecar.last_termination_reason = Some("Error".to_string());
    sidecar.last_exit_code = Some(1);
    pod.sidecar_containers = vec![sidecar];

    let findings = diagnose_pod(&pod, &snapshot(vec![], vec![]));

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].issue, "Sidecar container istio-proxy is crash-looping (6 restarts)");
    assert_eq!(findings[0].next_command, "kubectl logs api-7d9f -n prod -c istio-proxy --previous");
}

fn create_test_pod() -> PodInfo {
    PodInfo {
        uid: "uid-1".to_string(),
        name: "api-7d9f".to_string(),
        namespace: "prod".to_string(),
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
        node_name: Some("node-a".to_string()),
        containers: vec![ContainerInfo {
            name: "app".to_string(),
            ready: true,
            restart_count: 0,
            state: "Running".to_string(),
            last_termination_reason: None,
            last_exit_code: None,
            memory_limit_mb: 0,
        }],
    }
}

fn create_test_node(ready: bool) -> NodeMetric {
    let status = if ready { ConditionStatus::True } else { ConditionStatus::False };
    NodeMetric {
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
        ready,
        conditions: NodeConditions {
            ready: status,
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

fn pod_event(reason: &str, message: &str) -> ClusterEvent {
    ClusterEvent {
        kind: "Pod".to_string(),
        name: "api-7d9f".to_string(),
        reason: reason.to_string(),
        message: message.to_string(),
        event_type: EventType::Warning,
        count: 1,
        timestamp: "2026-10-14T10:00:00Z".to_string(),
    }
}

fn snapshot(nodes: Vec<NodeMetric>, events: Vec<ClusterEvent>) -> ClusterSnapshot {
    ClusterSnapshot {
        nodes,
        workloads: vec![],
        pods: vec![],
        events,
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
