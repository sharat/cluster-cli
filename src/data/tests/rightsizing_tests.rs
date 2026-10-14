use crate::data::models::{HealthStatus, PodInfo, PodOwner, RunningLimits};
use crate::data::rightsizing::{SizingVerdict, analyze, summarize};

fn deployment(name: &str) -> Option<PodOwner> {
    Some(PodOwner { kind: "Deployment".to_string(), name: name.to_string() })
}

/// A running pod with the given requests (cpu m, memory MB) and usage.
fn pod(name: &str, owner: Option<PodOwner>, request: (u64, u64), usage: (u64, u64)) -> PodInfo {
    PodInfo {
        uid: format!("uid-{name}"),
        name: name.to_string(),
        namespace: "prod".to_string(),
        phase: "Running".to_string(),
        restarts: 0,
        age: "1h".to_string(),
        cpu_millicores: usage.0,
        cpu_request_millicores: request.0,
        cpu_limit_millicores: 0,
        memory_mb: usage.1,
        memory_request_mb: request.1,
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
        owner,
        metrics_available: true,
        running_limits: Default::default(),
        node_name: Some("node-a".to_string()),
        containers: vec![],
    }
}

fn current_usage(pod: &PodInfo) -> Option<(u64, u64)> {
    pod.metrics_available.then_some((pod.cpu_millicores, pod.memory_mb))
}

#[test]
fn groups_replicas_of_a_workload_and_flags_idle_requests() {
    let pods = vec![
        pod("web-a", deployment("web"), (500, 512), (20, 60)),
        pod("web-b", deployment("web"), (500, 512), (30, 70)),
    ];

    let sizing = analyze(&pods, current_usage);

    assert_eq!(sizing.len(), 1);
    let web = &sizing[0];
    assert_eq!(web.owner.name, "web");
    assert_eq!(web.pods, 2);
    assert_eq!(web.cpu.request, 1000);
    assert_eq!(web.cpu.usage, 50);
    assert_eq!(web.verdict, SizingVerdict::OverProvisioned);
    // Per pod: 25m used + 30% = 32.5m, rounded up to 40m; 65MB + 30% -> 96MB.
    assert_eq!(web.suggested_cpu, Some(40));
    assert_eq!(web.suggested_memory, Some(96));
    assert_eq!(web.reclaimable(), (1000 - 80, 1024 - 192));
}

#[test]
fn usage_above_the_request_is_under_provisioned() {
    let pods = vec![pod("api-a", deployment("api"), (100, 128), (180, 100))];

    let sizing = analyze(&pods, current_usage);

    assert_eq!(sizing[0].verdict, SizingVerdict::UnderProvisioned);
    assert_eq!(sizing[0].suggested_cpu, Some(240));
    assert_eq!(sizing[0].suggested_memory, None, "memory is within its request");
    assert_eq!(sizing[0].reclaimable(), (0, 0));
}

#[test]
fn pods_without_requests_or_metrics_are_called_out() {
    let mut quiet = pod("quiet", deployment("quiet"), (100, 128), (0, 0));
    quiet.metrics_available = false;
    let pods = vec![pod("batch", None, (0, 0), (40, 200)), quiet];

    let sizing = analyze(&pods, current_usage);

    let verdict = |name: &str| sizing.iter().find(|w| w.owner.name == name).unwrap().verdict;
    assert_eq!(verdict("batch"), SizingVerdict::NoRequests);
    assert_eq!(verdict("quiet"), SizingVerdict::NoMetrics);
    let batch = sizing.iter().find(|w| w.owner.name == "batch").unwrap();
    assert_eq!(batch.owner.kind, "Pod");
    assert_eq!((batch.suggested_cpu, batch.suggested_memory), (Some(60), Some(272)));
}

#[test]
fn small_requests_and_reasonable_usage_are_left_alone() {
    let pods = vec![
        // Tiny request: idle but not worth flagging.
        pod("tiny", deployment("tiny"), (20, 32), (1, 5)),
        // Using about half of what it asks for.
        pod("steady", deployment("steady"), (200, 256), (100, 140)),
    ];

    let sizing = analyze(&pods, current_usage);

    assert!(sizing.iter().all(|w| w.verdict == SizingVerdict::Ok), "{sizing:?}");
}

#[test]
fn peak_usage_from_the_caller_overrides_a_quiet_sample() {
    let pods = vec![pod("web-a", deployment("web"), (500, 512), (20, 60))];

    let sizing = analyze(&pods, |_| Some((450, 480)));

    assert_eq!(sizing[0].verdict, SizingVerdict::Ok);
}

#[test]
fn only_running_pods_count_and_results_are_ordered_by_urgency() {
    let mut pending = pod("web-c", deployment("web"), (500, 512), (0, 0));
    pending.phase = "Pending".to_string();
    let pods = vec![
        pod("ok", deployment("ok"), (100, 128), (60, 90)),
        pod("web-a", deployment("web"), (500, 512), (20, 60)),
        pending,
        pod("hot", deployment("hot"), (50, 64), (90, 40)),
        pod("big", deployment("big"), (2000, 4096), (100, 200)),
    ];

    let sizing = analyze(&pods, current_usage);

    let names: Vec<&str> = sizing.iter().map(|w| w.owner.name.as_str()).collect();
    assert_eq!(names, vec!["hot", "big", "web", "ok"]);
    assert_eq!(sizing.iter().find(|w| w.owner.name == "web").unwrap().pods, 1);

    let summary = summarize(&sizing);
    assert_eq!(summary.workloads_flagged, 3);
    assert_eq!(summary.cpu.request, 100 + 500 + 50 + 2000);
    assert!(summary.reclaimable_cpu > 0 && summary.reclaimable_memory > 0);
}

#[test]
fn unmeasured_pods_are_left_out_instead_of_counted_as_idle() {
    // Two busy replicas were measured; a third is missing from `kubectl top`.
    let mut unmeasured = pod("web-c", deployment("web"), (500, 512), (0, 0));
    unmeasured.metrics_available = false;
    let pods = vec![
        pod("web-a", deployment("web"), (500, 512), (300, 400)),
        pod("web-b", deployment("web"), (500, 512), (320, 380)),
        unmeasured,
    ];

    let sizing = analyze(&pods, current_usage);

    assert_eq!(sizing[0].pods, 3);
    assert_eq!(sizing[0].measured_pods, 2);
    assert_eq!(sizing[0].cpu.request, 1000, "only measured pods are compared");
    assert_eq!(sizing[0].verdict, SizingVerdict::Ok);
}

#[test]
fn measured_zero_memory_is_usage_not_missing_metrics() {
    // `kubectl top` rounds tiny usage down to 0Mi; it was still measured.
    let pods = vec![pod("tiny", deployment("tiny"), (200, 256), (1, 0))];

    let sizing = analyze(&pods, current_usage);

    assert_eq!(sizing[0].verdict, SizingVerdict::OverProvisioned);
    assert_eq!(sizing[0].suggested_memory, Some(16));
}

#[test]
fn suggestion_is_capped_at_the_limit_when_usage_is_pinned_there() {
    // Throttled at a 200m limit with a 20m request: 200m + 30% would be 260m,
    // which Kubernetes rejects as a request above the limit.
    let mut busy = pod("busy", deployment("busy"), (20, 16), (200, 4));
    busy.running_limits.cpu_millicores = Some(200);
    let pods = vec![busy];

    let sizing = analyze(&pods, current_usage);

    assert_eq!(sizing[0].verdict, SizingVerdict::UnderProvisioned);
    assert_eq!(sizing[0].suggested_cpu, Some(200));
    assert!(sizing[0].cpu_at_limit);
    assert!(!sizing[0].memory_at_limit);
}

#[test]
fn suggestion_is_not_capped_when_a_pod_has_no_limit() {
    let mut limited = pod("api-a", deployment("api"), (100, 128), (180, 100));
    limited.running_limits.cpu_millicores = Some(150);
    // A pod with a limit on only some containers is unbounded overall, so the
    // summed limit it reports is not a ceiling.
    let mut partial = pod("api-b", deployment("api"), (100, 128), (180, 100));
    partial.cpu_limit_millicores = 150;
    partial.running_limits = RunningLimits { cpu_millicores: None, memory_mb: Some(256) };

    let sizing = analyze(&[limited, partial], current_usage);

    assert_eq!(sizing[0].suggested_cpu, Some(240));
    assert!(!sizing[0].cpu_at_limit);
}
