//! Rule-based "explain this pod" diagnosis.
//!
//! Shared by the pod detail view and the MCP `explain_pod` tool so both report
//! the same likely causes and suggested next commands. Everything here works on
//! data already in the snapshot; no extra kubectl calls are made.

use crate::data::models::{
    ClusterEvent, ClusterSnapshot, ConditionStatus, ContainerInfo, IncidentSeverity, PodInfo,
    RESOURCE_PRESSURE_PCT,
};

/// One problem found on a pod, with the most likely cause and a read-only
/// kubectl command to dig further.
#[derive(Debug, Clone, PartialEq)]
pub struct PodFinding {
    pub severity: IncidentSeverity,
    pub issue: String,
    pub likely_cause: String,
    pub next_command: String,
}

/// Diagnose a pod against the snapshot it came from. Returns findings ordered
/// most severe first; an empty list means no problems were detected.
pub fn diagnose_pod(pod: &PodInfo, snapshot: &ClusterSnapshot) -> Vec<PodFinding> {
    let events: Vec<&ClusterEvent> =
        snapshot.events.iter().filter(|e| e.kind == "Pod" && e.name == pod.name).collect();
    let mut findings = Vec::new();

    let all_containers = pod
        .init_containers
        .iter()
        .map(|c| (c, ContainerKind::Init))
        .chain(pod.sidecar_containers.iter().map(|c| (c, ContainerKind::Sidecar)))
        .chain(pod.containers.iter().map(|c| (c, ContainerKind::App)));
    for (container, kind) in all_containers {
        if let Some(finding) = diagnose_container(pod, container, kind, &events) {
            findings.push(finding);
        }
    }

    findings.extend(diagnose_phase(pod, &events));
    findings.extend(diagnose_probes(pod, &events));
    findings.extend(diagnose_node(pod, snapshot));
    findings.extend(diagnose_resources(pod));

    if findings.is_empty() && pod.phase == "Running" && !pod.is_ready {
        findings.push(PodFinding {
            severity: IncidentSeverity::Warning,
            issue: "Pod is running but not ready".to_string(),
            likely_cause: "A container has not passed its readiness probe yet, or is still \
                           starting up"
                .to_string(),
            next_command: describe_pod(pod),
        });
    }

    findings.sort_by_key(|f| f.severity);
    findings
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    App,
    Init,
    Sidecar,
}

fn diagnose_container(
    pod: &PodInfo,
    container: &ContainerInfo,
    kind: ContainerKind,
    events: &[&ClusterEvent],
) -> Option<PodFinding> {
    let name = &container.name;
    let label = match kind {
        ContainerKind::App => "Container",
        ContainerKind::Init => "Init container",
        ContainerKind::Sidecar => "Sidecar container",
    };
    // A completed init container has done its job. Its last_termination_reason
    // comes from lastState, i.e. an earlier failed attempt, so it would
    // otherwise be reported as failing.
    if kind == ContainerKind::Init && container.state == "Terminated (Completed)" {
        return None;
    }
    let waiting = waiting_reason(&container.state);
    let last_reason = container.last_termination_reason.as_deref();

    match waiting {
        Some("ImagePullBackOff" | "ErrImagePull" | "InvalidImageName") => {
            // kubelet also emits a bare "Error: ImagePullBackOff" Failed event;
            // only the "Failed to pull image" one says why.
            let detail = event_message(events, |e| {
                e.reason == "Failed" && e.message.starts_with("Failed to pull image")
            });
            return Some(PodFinding {
                severity: IncidentSeverity::Critical,
                issue: format!("{label} {name} cannot pull its image"),
                likely_cause: detail.as_deref().map(image_pull_cause).unwrap_or_else(|| {
                    "The image name or tag is wrong, or the registry needs credentials \
                     (imagePullSecrets)"
                        .to_string()
                }),
                next_command: describe_pod(pod),
            });
        }
        Some("CreateContainerConfigError") => {
            return Some(PodFinding {
                severity: IncidentSeverity::Critical,
                issue: format!("{label} {name} has a configuration error"),
                likely_cause: "A referenced ConfigMap, Secret, or key does not exist".to_string(),
                next_command: describe_pod(pod),
            });
        }
        Some("CreateContainerError" | "RunContainerError") => {
            return Some(PodFinding {
                severity: IncidentSeverity::Critical,
                issue: format!("{label} {name} could not be started"),
                likely_cause: "The runtime rejected the container; check command, args, and \
                               volume mounts"
                    .to_string(),
                next_command: describe_pod(pod),
            });
        }
        _ => {}
    }

    if last_reason == Some("OOMKilled") || container.state == "Terminated (OOMKilled)" {
        let likely_cause = if container.memory_limit_mb > 0 {
            format!(
                "Memory use exceeded its {} MB limit; raise the limit or reduce usage",
                container.memory_limit_mb
            )
        } else {
            "No memory limit is set, so the node killed it while low on memory".to_string()
        };
        return Some(PodFinding {
            severity: IncidentSeverity::Critical,
            issue: format!("{label} {name} was OOMKilled"),
            likely_cause,
            next_command: previous_logs(pod, container, kind),
        });
    }

    if waiting == Some("CrashLoopBackOff") {
        return Some(PodFinding {
            severity: IncidentSeverity::Critical,
            issue: format!(
                "{label} {name} is crash-looping ({} restarts)",
                container.restart_count
            ),
            likely_cause: exit_code_cause(container.last_exit_code),
            next_command: previous_logs(pod, container, kind),
        });
    }

    if container.restart_count > 0 && last_reason.is_some_and(|r| r != "Completed") {
        let exit = container.last_exit_code.map(|c| format!(", exit {c}")).unwrap_or_default();
        return Some(PodFinding {
            severity: IncidentSeverity::Elevated,
            issue: format!(
                "{label} {name} restarted {} time(s) (last: {}{exit})",
                container.restart_count,
                last_reason.unwrap_or("unknown")
            ),
            likely_cause: exit_code_cause(container.last_exit_code),
            next_command: previous_logs(pod, container, kind),
        });
    }

    None
}

fn diagnose_phase(pod: &PodInfo, events: &[&ClusterEvent]) -> Option<PodFinding> {
    match pod.phase.as_str() {
        "Pending" => {
            if let Some(message) = event_message(events, |e| e.reason == "FailedScheduling") {
                return Some(PodFinding {
                    severity: IncidentSeverity::Critical,
                    issue: "Pod cannot be scheduled".to_string(),
                    likely_cause: scheduling_cause(&message, pod),
                    next_command: pod_events(pod),
                });
            }
            if let Some(message) = event_message(events, |e| {
                matches!(e.reason.as_str(), "FailedMount" | "FailedAttachVolume")
            }) {
                return Some(PodFinding {
                    severity: IncidentSeverity::Critical,
                    issue: "A volume cannot be mounted".to_string(),
                    likely_cause: message,
                    next_command: describe_pod(pod),
                });
            }
            // Container-level waiting reasons (image pulls etc.) already explain
            // a scheduled pod that is still pending.
            let all = pod.init_containers.iter().chain(&pod.sidecar_containers);
            if all.chain(&pod.containers).any(|c| {
                waiting_reason(&c.state)
                    .is_some_and(|r| r != "ContainerCreating" && r != "PodInitializing")
            }) {
                return None;
            }
            // A regular init container that is Running has not finished yet. A
            // sidecar holds back later containers only until it has *started*
            // (startup probe passed); readiness does not gate them.
            let running_init =
                pod.init_containers.iter().find(|c| c.state == "Running" && !c.ready);
            let unstarted_sidecar =
                pod.sidecar_containers.iter().find(|c| c.state == "Running" && !c.started);
            let likely_cause = if pod.node_name.is_none() {
                "Waiting for the scheduler to place it on a node".to_string()
            } else if let Some(init) = running_init {
                format!("Init container {} is still running", init.name)
            } else if let Some(sidecar) = unstarted_sidecar {
                format!(
                    "Sidecar container {} has not started yet (startup probe still running)",
                    sidecar.name
                )
            } else {
                "Scheduled; containers are still being created (image pull or volume setup)"
                    .to_string()
            };
            Some(PodFinding {
                severity: IncidentSeverity::Warning,
                issue: "Pod is pending".to_string(),
                likely_cause,
                next_command: pod_events(pod),
            })
        }
        "Failed" => {
            let evicted = event_message(events, |e| e.reason == "Evicted");
            Some(PodFinding {
                severity: IncidentSeverity::Critical,
                issue: if evicted.is_some() {
                    "Pod was evicted".to_string()
                } else {
                    "Pod has failed".to_string()
                },
                likely_cause: evicted.unwrap_or_else(|| {
                    "All containers terminated and at least one exited with an error".to_string()
                }),
                next_command: describe_pod(pod),
            })
        }
        "Unknown" => Some(PodFinding {
            severity: IncidentSeverity::Critical,
            issue: "Pod state is unknown".to_string(),
            likely_cause: "The kubelet on its node stopped reporting; the node may be down"
                .to_string(),
            next_command: describe_pod(pod),
        }),
        _ => None,
    }
}

fn diagnose_probes(pod: &PodInfo, events: &[&ClusterEvent]) -> Vec<PodFinding> {
    let mut liveness = 0;
    let mut readiness = 0;
    for event in events.iter().filter(|e| e.reason == "Unhealthy") {
        let message = event.message.to_lowercase();
        let count = event.count.max(1);
        if message.contains("liveness probe failed") {
            liveness += count;
        }
        if message.contains("readiness probe failed") {
            readiness += count;
        }
    }

    let mut findings = Vec::new();
    if liveness > 0 {
        findings.push(PodFinding {
            severity: IncidentSeverity::Warning,
            issue: format!("Liveness probe failed {liveness} time(s)"),
            likely_cause: "The kubelet restarts the container on failure; check the probe \
                           path/port and initialDelaySeconds"
                .to_string(),
            next_command: describe_pod(pod),
        });
    }
    if readiness > 0 {
        findings.push(PodFinding {
            severity: IncidentSeverity::Warning,
            issue: format!("Readiness probe failed {readiness} time(s)"),
            likely_cause: "Removed from Service endpoints until it passes; a dependency may \
                           be down or startup is slow"
                .to_string(),
            next_command: describe_pod(pod),
        });
    }
    findings
}

fn diagnose_node(pod: &PodInfo, snapshot: &ClusterSnapshot) -> Option<PodFinding> {
    let node_name = pod.node_name.as_deref()?;
    let node = snapshot.nodes.iter().find(|n| n.name == node_name)?;

    let problem = if !node.ready {
        "is NotReady"
    } else if node.conditions.memory_pressure == ConditionStatus::True {
        "has MemoryPressure"
    } else if node.conditions.disk_pressure == ConditionStatus::True {
        "has DiskPressure"
    } else if node.conditions.pid_pressure == ConditionStatus::True {
        "has PIDPressure"
    } else {
        return None;
    };

    Some(PodFinding {
        severity: if node.ready { IncidentSeverity::Warning } else { IncidentSeverity::Critical },
        issue: format!("Node {node_name} {problem}"),
        likely_cause: "Pods on this node may be evicted or fail regardless of their own health"
            .to_string(),
        next_command: format!("kubectl describe node {node_name}"),
    })
}

fn diagnose_resources(pod: &PodInfo) -> Vec<PodFinding> {
    let mut findings = Vec::new();
    let oom_reported = pod.oom_killed
        || pod
            .running_containers()
            .any(|c| c.last_termination_reason.as_deref() == Some("OOMKilled"));

    if pod.memory_limit_mb > 0 && pod.memory_pct >= RESOURCE_PRESSURE_PCT && !oom_reported {
        findings.push(PodFinding {
            severity: IncidentSeverity::Warning,
            issue: format!("Memory at {}% of its limit", pod.memory_pct),
            likely_cause: format!(
                "Close to the {} MB limit; the container will be OOMKilled if it grows",
                pod.memory_limit_mb
            ),
            next_command: top_pod(pod),
        });
    }
    if pod.cpu_limit_millicores > 0 && pod.cpu_pct >= RESOURCE_PRESSURE_PCT {
        findings.push(PodFinding {
            severity: IncidentSeverity::Elevated,
            issue: format!("CPU at {}% of its limit", pod.cpu_pct),
            likely_cause: "The container is likely being CPU-throttled, which slows requests"
                .to_string(),
            next_command: top_pod(pod),
        });
    }
    findings
}

fn exit_code_cause(code: Option<i32>) -> String {
    match code {
        Some(0) => {
            "The process exits successfully, but the pod expects it to keep running".to_string()
        }
        Some(126) => "The command is not executable (exit 126)".to_string(),
        Some(127) => {
            "The command was not found (exit 127); check command/args and the image entrypoint"
                .to_string()
        }
        Some(137) => "Killed by SIGKILL (exit 137), usually a failed liveness probe or memory \
                      limit"
            .to_string(),
        Some(139) => "Segmentation fault (exit 139)".to_string(),
        Some(143) => "Terminated by SIGTERM (exit 143), often a failed liveness probe".to_string(),
        Some(code) => format!("The application exited with an error (exit {code}); check its logs"),
        None => "The container keeps exiting shortly after starting; check its logs".to_string(),
    }
}

/// Summarize a kubelet "Failed to pull image" message, whose useful part is
/// usually at the end of a long runtime error chain.
fn image_pull_cause(message: &str) -> String {
    let image = message.split('"').nth(1).unwrap_or("the image");
    let lower = message.to_lowercase();
    if lower.contains("not found")
        || lower.contains("does not exist")
        || lower.contains("manifest unknown")
    {
        format!(
            "Image {image} does not exist, or the registry needs credentials (imagePullSecrets)"
        )
    } else if lower.contains("unauthorized")
        || lower.contains("denied")
        || lower.contains("authentication required")
    {
        format!("The registry denied access to {image}; check imagePullSecrets")
    } else if lower.contains("timeout")
        || lower.contains("no such host")
        || lower.contains("connection refused")
    {
        format!("The registry for {image} could not be reached from the node")
    } else {
        message.to_string()
    }
}

fn scheduling_cause(message: &str, pod: &PodInfo) -> String {
    let lower = message.to_lowercase();
    if lower.contains("insufficient memory") {
        format!("No node has {} MB of free memory to satisfy the request", pod.memory_request_mb)
    } else if lower.contains("insufficient cpu") {
        format!("No node has {}m of free CPU to satisfy the request", pod.cpu_request_millicores)
    } else if lower.contains("persistentvolumeclaim") {
        "A PersistentVolumeClaim it uses is not bound".to_string()
    } else if lower.contains("taint") {
        "Nodes have taints the pod does not tolerate".to_string()
    } else if lower.contains("affinity") || lower.contains("selector") {
        "No node matches the pod's nodeSelector or affinity rules".to_string()
    } else {
        message.to_string()
    }
}

/// Extract `X` from a container state summary of the form `Waiting (X)`.
fn waiting_reason(state: &str) -> Option<&str> {
    state.strip_prefix("Waiting (")?.strip_suffix(')')
}

fn event_message(
    events: &[&ClusterEvent],
    matches: impl Fn(&ClusterEvent) -> bool,
) -> Option<String> {
    // fetch_events stores events newest first, so the first match is the latest.
    events.iter().find(|e| matches(e)).map(|e| e.message.clone())
}

fn previous_logs(pod: &PodInfo, container: &ContainerInfo, kind: ContainerKind) -> String {
    // kubectl defaults to the first app container, so name any other one.
    let flag = if kind != ContainerKind::App || pod.containers.len() > 1 {
        format!(" -c {}", container.name)
    } else {
        String::new()
    };
    format!("kubectl logs {} -n {}{flag} --previous", pod.name, pod.namespace)
}

fn describe_pod(pod: &PodInfo) -> String {
    format!("kubectl describe pod {} -n {}", pod.name, pod.namespace)
}

fn pod_events(pod: &PodInfo) -> String {
    format!(
        "kubectl get events -n {} --field-selector involvedObject.name={}",
        pod.namespace, pod.name
    )
}

fn top_pod(pod: &PodInfo) -> String {
    format!("kubectl top pod {} -n {} --containers", pod.name, pod.namespace)
}
