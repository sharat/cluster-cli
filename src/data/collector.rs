use std::collections::HashMap;
use std::fmt;
use std::io::ErrorKind;

use anyhow::Result;
use serde_json::Value;
use tokio::process::Command;

use crate::data::models::*;

#[derive(Debug, Clone)]
pub struct KubectlError {
    issue: Option<ConnectionIssue>,
    message: String,
}

impl KubectlError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            issue: None,
            message: message.into(),
        }
    }

    fn with_issue(issue: ConnectionIssue, message: impl Into<String>) -> Self {
        Self {
            issue: Some(issue),
            message: message.into(),
        }
    }

    pub fn connection_issue(&self) -> Option<&ConnectionIssue> {
        self.issue.as_ref()
    }
}

impl fmt::Display for KubectlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for KubectlError {}

async fn run_cmd(program: &str, args: &[&str]) -> Result<String, KubectlError> {
    ensure_readonly_kubectl_args(program, args).map_err(|err| KubectlError::new(err.to_string()))?;

    let output = Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|err| {
            if err.kind() == ErrorKind::NotFound {
                KubectlError::with_issue(
                    ConnectionIssue {
                        kind: ConnectionIssueKind::KubectlMissing,
                        namespace: None,
                        detail: "kubectl was not found in PATH".to_string(),
                    },
                    format!("Failed to run {program} {:?}: {err}", args),
                )
            } else {
                KubectlError::new(format!("Failed to run {program} {:?}: {err}", args))
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_trimmed = stderr.trim();
        let issue = classify_connection_issue(args, stderr_trimmed);
        return Err(match issue {
            Some(issue) => KubectlError::with_issue(
                issue,
                format!("{program} failed: {stderr_trimmed}"),
            ),
            None => KubectlError::new(format!("{program} failed: {stderr_trimmed}")),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn ensure_readonly_kubectl_args(program: &str, args: &[&str]) -> Result<()> {
    if program != "kubectl" {
        anyhow::bail!("Only kubectl is allowed, got {}", program);
    }

    match args {
        ["get", ..] | ["top", ..] | ["logs", ..] => Ok(()),
        ["config", "current-context", ..] | ["config", "view", ..] => Ok(()),
        [] => anyhow::bail!("kubectl command is empty"),
        _ => anyhow::bail!("Rejected non-read-only kubectl command: kubectl {}", args.join(" ")),
    }
}

pub async fn fetch_node_metrics() -> Result<Vec<NodeMetric>> {
    let (top_result, info_result) = tokio::join!(
        run_cmd("kubectl", &["top", "nodes", "--no-headers"]),
        run_cmd("kubectl", &["get", "nodes", "-o", "json"]),
    );

    let top_output = top_result.unwrap_or_default();
    let info_json: Value = serde_json::from_str(&info_result?)?;
    Ok(build_node_metrics(&top_output, &info_json))
}

#[derive(Clone, Copy)]
struct TopNodeMetrics {
    cpu_millicores: u64,
    cpu_pct: Option<u8>,
    memory_mb: u64,
    memory_pct: Option<u8>,
}

fn build_node_metrics(top_output: &str, info_json: &Value) -> Vec<NodeMetric> {
    let top_map = parse_top_node_metrics(top_output);
    let mut nodes = Vec::new();

    if let Some(items) = info_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(node) = build_node_metric(item, top_map.get(metadata_name(item).as_str()).copied()) {
                nodes.push(node);
            }
        }
    }

    if nodes.is_empty() {
        for (name, metrics) in top_map {
            nodes.push(NodeMetric {
                name,
                cpu_millicores: metrics.cpu_millicores,
                memory_mb: metrics.memory_mb,
                memory_total_mb: 0,
                memory_pct: metrics.memory_pct.unwrap_or(0),
                cpu_pct: metrics.cpu_pct.unwrap_or(0),
                status: derive_node_status(metrics.memory_pct.unwrap_or(0), false, 0),
                cpu_capacity: 0,
                memory_capacity_mb: 0,
                unhealthy_conditions: 0,
                ready: false,
                conditions: NodeConditions {
                    ready: ConditionStatus::Unknown,
                    memory_pressure: ConditionStatus::Unknown,
                    disk_pressure: ConditionStatus::Unknown,
                    pid_pressure: ConditionStatus::Unknown,
                    network_unavailable: ConditionStatus::Unknown,
                },
                cordoned: false,
                draining: false,
                node_info: None,
            });
        }
    }

    nodes
}

fn parse_top_node_metrics(output: &str) -> HashMap<String, TopNodeMetrics> {
    let mut top_map = HashMap::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        top_map.insert(
            parts[0].to_string(),
            TopNodeMetrics {
                cpu_millicores: parse_cpu(parts[1]),
                cpu_pct: parts[2].trim_end_matches('%').parse::<u8>().ok(),
                memory_mb: parse_memory_mb(parts[3]),
                memory_pct: parts[4].trim_end_matches('%').parse::<u8>().ok(),
            },
        );
    }

    top_map
}

fn build_node_metric(item: &Value, top_metrics: Option<TopNodeMetrics>) -> Option<NodeMetric> {
    let name = metadata_name(item);
    if name.is_empty() {
        return None;
    }

    let memory_total_mb = item
        .pointer("/status/allocatable/memory")
        .and_then(|v| v.as_str())
        .map(parse_memory_mb)
        .unwrap_or(0);
    let cpu_capacity = item
        .pointer("/status/capacity/cpu")
        .and_then(|v| v.as_str())
        .map(parse_cpu)
        .unwrap_or(0);
    let memory_capacity_mb = item
        .pointer("/status/capacity/memory")
        .and_then(|v| v.as_str())
        .map(parse_memory_mb)
        .unwrap_or(0);
    let (conditions, unhealthy_conditions, cordoned, draining) =
        get_node_health_state_for_item(item);
    let node_info = get_node_info_for_item(item);
    let ready = conditions.ready == ConditionStatus::True;
    let cpu_millicores = top_metrics.map(|m| m.cpu_millicores).unwrap_or(0);
    let memory_mb = top_metrics.map(|m| m.memory_mb).unwrap_or(0);
    let memory_pct = if memory_total_mb > 0 {
        ((memory_mb * 100) / memory_total_mb).min(100) as u8
    } else {
        top_metrics.and_then(|m| m.memory_pct).unwrap_or(0)
    };
    let cpu_pct = if cpu_capacity > 0 {
        ((cpu_millicores * 100) / cpu_capacity).min(100) as u8
    } else {
        top_metrics.and_then(|m| m.cpu_pct).unwrap_or(0)
    };

    Some(NodeMetric {
        name,
        cpu_millicores,
        memory_mb,
        memory_total_mb,
        memory_pct,
        cpu_pct,
        status: derive_node_status(memory_pct, ready, unhealthy_conditions),
        cpu_capacity,
        memory_capacity_mb,
        unhealthy_conditions,
        ready,
        conditions,
        cordoned,
        draining,
        node_info,
    })
}

fn derive_node_status(memory_pct: u8, ready: bool, unhealthy_conditions: u8) -> HealthStatus {
    if !ready || unhealthy_conditions > 0 {
        HealthStatus::Critical
    } else {
        HealthStatus::from_pct(memory_pct)
    }
}

#[allow(dead_code)]
fn get_node_allocatable_memory(nodes_json: &Value, node_name: &str) -> u64 {
    let items = nodes_json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default();

    for item in items {
        let name = item
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if name == node_name {
            let mem_str = item
                .pointer("/status/allocatable/memory")
                .and_then(|v| v.as_str())
                .unwrap_or("0Ki");
            return parse_memory_mb(mem_str);
        }
    }
    0
}

#[allow(dead_code)]
fn get_node_cpu_capacity(nodes_json: &Value, node_name: &str) -> u64 {
    let items = nodes_json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default();

    for item in items {
        let name = item
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if name == node_name {
            return item
                .pointer("/status/capacity/cpu")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
        }
    }
    0
}

#[allow(dead_code)]
fn get_node_memory_capacity(nodes_json: &Value, node_name: &str) -> u64 {
    let items = nodes_json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default();

    for item in items {
        let name = item
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if name == node_name {
            let mem_str = item
                .pointer("/status/capacity/memory")
                .and_then(|v| v.as_str())
                .unwrap_or("0Ki");
            return parse_memory_mb(mem_str);
        }
    }
    0
}

#[allow(dead_code)]
fn get_node_info(nodes_json: &Value, node_name: &str) -> Option<NodeInfo> {
    let items = nodes_json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default();

    items
        .iter()
        .find(|item| metadata_name(item) == node_name)
        .and_then(|item| get_node_info_for_item(item))
}

fn get_node_info_for_item(item: &Value) -> Option<NodeInfo> {
    let node_info = item.pointer("/status/nodeInfo")?;

    Some(NodeInfo {
        kernel_version: node_info
            .get("kernelVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        os_image: node_info
            .get("osImage")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        container_runtime: node_info
            .get("containerRuntimeVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        kubelet_version: node_info
            .get("kubeletVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        architecture: node_info
            .get("architecture")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        operating_system: node_info
            .get("operatingSystem")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        kernel: node_info
            .get("kernel")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[allow(dead_code)]
fn get_node_health_state(nodes_json: &Value, node_name: &str) -> (NodeConditions, u8, bool, bool) {
    let items = nodes_json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or_default();

    for item in items {
        let name = item
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if name != node_name {
            continue;
        }

        let mut conditions = NodeConditions {
            ready: ConditionStatus::Unknown,
            memory_pressure: ConditionStatus::Unknown,
            disk_pressure: ConditionStatus::Unknown,
            pid_pressure: ConditionStatus::Unknown,
            network_unavailable: ConditionStatus::Unknown,
        };
        let mut unhealthy_conditions = 0u8;
        let cordoned = item
            .pointer("/spec/unschedulable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let draining = false;

        if let Some(condition_entries) = item.pointer("/status/conditions").and_then(|v| v.as_array()) {
            for condition in condition_entries {
                let condition_type = condition
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status = parse_condition_status(
                    condition
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown"),
                );

                match condition_type {
                    "Ready" => {
                        conditions.ready = status;
                        if status != ConditionStatus::True {
                            unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                        }
                    }
                    "MemoryPressure" => {
                        conditions.memory_pressure = status;
                        if status == ConditionStatus::True {
                            unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                        }
                    }
                    "DiskPressure" => {
                        conditions.disk_pressure = status;
                        if status == ConditionStatus::True {
                            unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                        }
                    }
                    "PIDPressure" => {
                        conditions.pid_pressure = status;
                        if status == ConditionStatus::True {
                            unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                        }
                    }
                    "NetworkUnavailable" => {
                        conditions.network_unavailable = status;
                        if status == ConditionStatus::True {
                            unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                        }
                    }
                    _ => {}
                }
            }
        }

        return (conditions, unhealthy_conditions, cordoned, draining);
    }

    (
        NodeConditions {
            ready: ConditionStatus::Unknown,
            memory_pressure: ConditionStatus::Unknown,
            disk_pressure: ConditionStatus::Unknown,
            pid_pressure: ConditionStatus::Unknown,
            network_unavailable: ConditionStatus::Unknown,
        },
        0,
        false,
        false,
    )
}

fn get_node_health_state_for_item(item: &Value) -> (NodeConditions, u8, bool, bool) {
    let mut conditions = NodeConditions {
        ready: ConditionStatus::Unknown,
        memory_pressure: ConditionStatus::Unknown,
        disk_pressure: ConditionStatus::Unknown,
        pid_pressure: ConditionStatus::Unknown,
        network_unavailable: ConditionStatus::Unknown,
    };
    let mut unhealthy_conditions = 0u8;
    let cordoned = item
        .pointer("/spec/unschedulable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let draining = false;

    if let Some(condition_entries) = item.pointer("/status/conditions").and_then(|v| v.as_array()) {
        for condition in condition_entries {
            let condition_type = condition
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = parse_condition_status(
                condition
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown"),
            );

            match condition_type {
                "Ready" => {
                    conditions.ready = status;
                    if status != ConditionStatus::True {
                        unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                    }
                }
                "MemoryPressure" => {
                    conditions.memory_pressure = status;
                    if status == ConditionStatus::True {
                        unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                    }
                }
                "DiskPressure" => {
                    conditions.disk_pressure = status;
                    if status == ConditionStatus::True {
                        unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                    }
                }
                "PIDPressure" => {
                    conditions.pid_pressure = status;
                    if status == ConditionStatus::True {
                        unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                    }
                }
                "NetworkUnavailable" => {
                    conditions.network_unavailable = status;
                    if status == ConditionStatus::True {
                        unhealthy_conditions = unhealthy_conditions.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
    }

    (conditions, unhealthy_conditions, cordoned, draining)
}

fn parse_condition_status(status: &str) -> ConditionStatus {
    match status {
        "True" => ConditionStatus::True,
        "False" => ConditionStatus::False,
        _ => ConditionStatus::Unknown,
    }
}

pub async fn fetch_pod_info(namespace: &str) -> Result<Vec<PodInfo>> {
    let top_args = vec!["top", "pods", "-n", namespace, "--no-headers"];
    let info_args = vec!["get", "pods", "-n", namespace, "-o", "json"];
    let (top_result, info_result) = tokio::join!(
        run_cmd("kubectl", &top_args),
        run_cmd("kubectl", &info_args),
    );

    let top_output = top_result.unwrap_or_default();
    let info_json: Value = serde_json::from_str(&info_result?)?;

    // Build map: pod name -> (cpu_m, mem_mb)
    let mut top_map = std::collections::HashMap::new();
    for line in top_output.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        top_map.insert(parts[0].to_string(), (parse_cpu(parts[1]), parse_memory_mb(parts[2])));
    }

    let mut pods = Vec::new();

    if let Some(items) = info_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let name = item
                .pointer("/metadata/name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let uid = item
                .pointer("/metadata/uid")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let pod_namespace = item
                .pointer("/metadata/namespace")
                .and_then(|v| v.as_str())
                .unwrap_or(namespace)
                .to_string();

            let phase = item
                .pointer("/status/phase")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let node_name = item
                .pointer("/spec/nodeName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let restarts: u32 = item
                .pointer("/status/containerStatuses")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|c| c.get("restartCount").and_then(|v| v.as_u64()).unwrap_or(0) as u32)
                        .sum()
                })
                .unwrap_or(0);

            let age = item
                .pointer("/metadata/creationTimestamp")
                .and_then(|v| v.as_str())
                .map(calculate_age)
                .unwrap_or_else(|| "?".to_string());

            let spec_containers = item
                .pointer("/spec/containers")
                .and_then(|v| v.as_array());
            let init_containers = item
                .pointer("/spec/initContainers")
                .and_then(|v| v.as_array());

            let (cpu_request_millicores, cpu_limit_millicores) =
                effective_pod_cpu_resources(spec_containers, init_containers);
            let (memory_request_mb, memory_limit_mb) =
                effective_pod_memory_resources(spec_containers, init_containers);

            let (cpu_millicores, memory_mb) = top_map.get(&name).copied().unwrap_or((0, 0));

            let memory_pct = if memory_limit_mb > 0 {
                ((memory_mb * 100) / memory_limit_mb).min(100) as u8
            } else {
                0
            };
            let memory_request_pct = if memory_request_mb > 0 {
                ((memory_mb * 100) / memory_request_mb).min(100) as u8
            } else {
                0
            };

            let cpu_pct = if cpu_limit_millicores > 0 {
                ((cpu_millicores * 100) / cpu_limit_millicores).min(100) as u8
            } else {
                0
            };
            let cpu_request_pct = if cpu_request_millicores > 0 {
                ((cpu_millicores * 100) / cpu_request_millicores).min(100) as u8
            } else {
                0
            };

            let container_statuses = item
                .pointer("/status/containerStatuses")
                .and_then(|v| v.as_array());
            let containers = extract_container_info(container_statuses);
            let total_containers = container_statuses.map(|arr| arr.len() as u32).unwrap_or(0);
            let ready_containers = container_statuses
                .map(|arr| {
                    arr.iter()
                        .filter(|c| c.get("ready").and_then(|v| v.as_bool()).unwrap_or(false))
                        .count() as u32
                })
                .unwrap_or(0);
            let is_ready = total_containers > 0 && ready_containers == total_containers;
            let crash_looping = container_statuses
                .map(|arr| {
                    arr.iter().any(|c| {
                        c.pointer("/state/waiting/reason")
                            .and_then(|v| v.as_str())
                            == Some("CrashLoopBackOff")
                    })
                })
                .unwrap_or(false);
            let oom_killed = container_statuses
                .map(|arr| {
                    arr.iter().any(|c| {
                        c.pointer("/lastState/terminated/reason")
                            .and_then(|v| v.as_str())
                            == Some("OOMKilled")
                            || c.pointer("/state/terminated/reason")
                                .and_then(|v| v.as_str())
                                == Some("OOMKilled")
                    })
                })
                .unwrap_or(false);
            let status = derive_pod_status(memory_pct, &phase, is_ready, crash_looping, oom_killed);

            pods.push(PodInfo {
                uid,
                name,
                namespace: pod_namespace,
                phase,
                restarts,
                age,
                cpu_millicores,
                cpu_request_millicores,
                cpu_limit_millicores,
                memory_mb,
                memory_request_mb,
                memory_limit_mb,
                memory_request_pct,
                memory_pct,
                cpu_request_pct,
                cpu_pct,
                status,
                ready_containers,
                total_containers,
                is_ready,
                crash_looping,
                oom_killed,
                node_name,
                containers,
            });
        }
    }

    Ok(pods)
}

pub async fn fetch_workload_summaries(namespace: &str) -> Result<Vec<WorkloadSummary>> {
    let deployment_args = ["get", "deployments", "-n", namespace, "-o", "json"];
    let statefulset_args = ["get", "statefulsets", "-n", namespace, "-o", "json"];
    let daemonset_args = ["get", "daemonsets", "-n", namespace, "-o", "json"];
    let replicaset_args = ["get", "replicasets", "-n", namespace, "-o", "json"];

    let (deployments_result, statefulsets_result, daemonsets_result, replicasets_result) =
        tokio::join!(
            run_cmd("kubectl", &deployment_args),
            run_cmd("kubectl", &statefulset_args),
            run_cmd("kubectl", &daemonset_args),
            run_cmd("kubectl", &replicaset_args),
        );

    let deployment_json: Value =
        serde_json::from_str(&deployments_result.unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or(Value::Null);
    let statefulset_json: Value =
        serde_json::from_str(&statefulsets_result.unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or(Value::Null);
    let daemonset_json: Value =
        serde_json::from_str(&daemonsets_result.unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or(Value::Null);
    let replicasets_json: Value =
        serde_json::from_str(&replicasets_result.unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or(Value::Null);

    let deployment_rs_map = build_deployment_replicaset_map(&replicasets_json);
    let mut workloads = Vec::new();

    if let Some(items) = deployment_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let name = metadata_name(item);
            if name.is_empty() {
                continue;
            }

            let desired = status_u32(item, "/spec/replicas");
            let ready = status_u32(item, "/status/readyReplicas");
            let available = status_u32(item, "/status/availableReplicas");
            let updated = status_u32(item, "/status/updatedReplicas");
            let unavailable = status_u32(item, "/status/unavailableReplicas")
                .max(desired.saturating_sub(ready));
            let progressing = find_condition_reason(item, "Progressing");
            let available_condition = find_condition_status(item, "Available");
            let status = workload_health(unavailable, progressing.as_deref(), available_condition);
            let rollout_status = deployment_rollout_status(
                desired,
                ready,
                updated,
                unavailable,
                progressing.as_deref(),
                available_condition,
            );

            let mut related_event_targets =
                vec![(WorkloadKind::Deployment.as_str().to_string(), name.clone())];
            if let Some(replica_sets) = deployment_rs_map.get(&name) {
                for rs in replica_sets {
                    related_event_targets.push(("ReplicaSet".to_string(), rs.clone()));
                }
            }

            workloads.push(WorkloadSummary {
                kind: WorkloadKind::Deployment,
                name,
                namespace: namespace.to_string(),
                desired_replicas: desired,
                ready_replicas: ready,
                available_replicas: available,
                updated_replicas: Some(updated),
                current_replicas: None,
                unavailable_pods: unavailable,
                rollout_status,
                status,
                recent_events: Vec::new(),
                related_event_targets,
            });
        }
    }

    if let Some(items) = statefulset_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let name = metadata_name(item);
            if name.is_empty() {
                continue;
            }

            let desired = status_u32(item, "/spec/replicas");
            let ready = status_u32(item, "/status/readyReplicas");
            let available = ready;
            let updated = status_u32(item, "/status/updatedReplicas");
            let current = status_u32(item, "/status/currentReplicas");
            let unavailable = desired.saturating_sub(ready);
            let update_revision = item
                .pointer("/status/updateRevision")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let current_revision = item
                .pointer("/status/currentRevision")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let status = workload_health(unavailable, None, Some(ConditionStatus::True));
            let rollout_status = statefulset_rollout_status(
                desired,
                ready,
                updated,
                current,
                unavailable,
                update_revision,
                current_revision,
            );

            workloads.push(WorkloadSummary {
                kind: WorkloadKind::StatefulSet,
                name: name.clone(),
                namespace: namespace.to_string(),
                desired_replicas: desired,
                ready_replicas: ready,
                available_replicas: available,
                updated_replicas: Some(updated),
                current_replicas: Some(current),
                unavailable_pods: unavailable,
                rollout_status,
                status,
                recent_events: Vec::new(),
                related_event_targets: vec![(WorkloadKind::StatefulSet.as_str().to_string(), name)],
            });
        }
    }

    if let Some(items) = daemonset_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let name = metadata_name(item);
            if name.is_empty() {
                continue;
            }

            let desired = status_u32(item, "/status/desiredNumberScheduled");
            let ready = status_u32(item, "/status/numberReady");
            let available = status_u32(item, "/status/numberAvailable");
            let updated = status_u32(item, "/status/updatedNumberScheduled");
            let unavailable = status_u32(item, "/status/numberUnavailable")
                .max(desired.saturating_sub(ready));
            let status = workload_health(unavailable, None, Some(ConditionStatus::True));
            let rollout_status =
                daemonset_rollout_status(desired, ready, available, updated, unavailable);

            workloads.push(WorkloadSummary {
                kind: WorkloadKind::DaemonSet,
                name: name.clone(),
                namespace: namespace.to_string(),
                desired_replicas: desired,
                ready_replicas: ready,
                available_replicas: available,
                updated_replicas: Some(updated),
                current_replicas: None,
                unavailable_pods: unavailable,
                rollout_status,
                status,
                recent_events: Vec::new(),
                related_event_targets: vec![(WorkloadKind::DaemonSet.as_str().to_string(), name)],
            });
        }
    }

    workloads.sort_by(|a, b| {
        b.unavailable_pods
            .cmp(&a.unavailable_pods)
            .then_with(|| a.kind.as_str().cmp(b.kind.as_str()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(workloads)
}

pub fn attach_workload_events(
    workloads: &mut [WorkloadSummary],
    events: &[ClusterEvent],
) {
    for workload in workloads.iter_mut() {
        let mut recent_events: Vec<ClusterEvent> = events
            .iter()
            .filter(|event| {
                workload
                    .related_event_targets
                    .iter()
                    .any(|(kind, name)| event.kind == *kind && event.name == *name)
            })
            .take(3)
            .cloned()
            .collect();

        if recent_events.is_empty() {
            recent_events = events
                .iter()
                .filter(|event| {
                    event.kind == workload.kind.as_str() && event.name == workload.name
                })
                .take(3)
                .cloned()
                .collect();
        }

        workload.recent_events = recent_events;
    }
}

fn sum_container_cpu_resources(containers: Option<&Vec<Value>>) -> (u64, u64) {
    let mut requests = 0;
    let mut limits = 0;

    if let Some(containers) = containers {
        for container in containers {
            if let Some(value) = container.pointer("/resources/requests/cpu").and_then(|v| v.as_str()) {
                requests += parse_cpu(value);
            }
            if let Some(value) = container.pointer("/resources/limits/cpu").and_then(|v| v.as_str()) {
                limits += parse_cpu(value);
            }
        }
    }

    (requests, limits)
}

fn sum_container_memory_resources(containers: Option<&Vec<Value>>) -> (u64, u64) {
    let mut requests = 0;
    let mut limits = 0;

    if let Some(containers) = containers {
        for container in containers {
            if let Some(value) = container
                .pointer("/resources/requests/memory")
                .and_then(|v| v.as_str())
            {
                requests += parse_memory_mb(value);
            }
            if let Some(value) = container.pointer("/resources/limits/memory").and_then(|v| v.as_str()) {
                limits += parse_memory_mb(value);
            }
        }
    }

    (requests, limits)
}

fn max_container_cpu_resources(containers: Option<&Vec<Value>>) -> (u64, u64) {
    let mut max_requests = 0;
    let mut max_limits = 0;

    if let Some(containers) = containers {
        for container in containers {
            if let Some(value) = container.pointer("/resources/requests/cpu").and_then(|v| v.as_str()) {
                max_requests = max_requests.max(parse_cpu(value));
            }
            if let Some(value) = container.pointer("/resources/limits/cpu").and_then(|v| v.as_str()) {
                max_limits = max_limits.max(parse_cpu(value));
            }
        }
    }

    (max_requests, max_limits)
}

fn max_container_memory_resources(containers: Option<&Vec<Value>>) -> (u64, u64) {
    let mut max_requests = 0;
    let mut max_limits = 0;

    if let Some(containers) = containers {
        for container in containers {
            if let Some(value) = container
                .pointer("/resources/requests/memory")
                .and_then(|v| v.as_str())
            {
                max_requests = max_requests.max(parse_memory_mb(value));
            }
            if let Some(value) = container.pointer("/resources/limits/memory").and_then(|v| v.as_str()) {
                max_limits = max_limits.max(parse_memory_mb(value));
            }
        }
    }

    (max_requests, max_limits)
}

fn effective_pod_cpu_resources(
    app_containers: Option<&Vec<Value>>,
    init_containers: Option<&Vec<Value>>,
) -> (u64, u64) {
    let (app_requests, app_limits) = sum_container_cpu_resources(app_containers);
    let (init_requests, init_limits) = max_container_cpu_resources(init_containers);

    (app_requests.max(init_requests), app_limits.max(init_limits))
}

fn effective_pod_memory_resources(
    app_containers: Option<&Vec<Value>>,
    init_containers: Option<&Vec<Value>>,
) -> (u64, u64) {
    let (app_requests, app_limits) = sum_container_memory_resources(app_containers);
    let (init_requests, init_limits) = max_container_memory_resources(init_containers);

    (app_requests.max(init_requests), app_limits.max(init_limits))
}

fn derive_pod_status(
    memory_pct: u8,
    phase: &str,
    is_ready: bool,
    crash_looping: bool,
    oom_killed: bool,
) -> HealthStatus {
    if crash_looping || oom_killed || !is_ready || matches!(phase, "Failed" | "Unknown") {
        HealthStatus::Critical
    } else if phase == "Pending" {
        HealthStatus::Warning
    } else {
        HealthStatus::from_pct(memory_pct)
    }
}

fn extract_container_info(container_statuses: Option<&Vec<Value>>) -> Vec<ContainerInfo> {
    container_statuses
        .map(|statuses| {
            statuses
                .iter()
                .map(|status| ContainerInfo {
                    name: status
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    ready: status
                        .get("ready")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    restart_count: status
                        .get("restartCount")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    state: summarize_container_state(status),
                    last_termination_reason: status
                        .pointer("/lastState/terminated/reason")
                        .and_then(|v| v.as_str())
                        .or_else(|| {
                            status
                                .pointer("/state/terminated/reason")
                                .and_then(|v| v.as_str())
                        })
                        .map(|reason| reason.to_string()),
                    last_exit_code: status
                        .pointer("/lastState/terminated/exitCode")
                        .and_then(|v| v.as_i64())
                        .or_else(|| {
                            status
                                .pointer("/state/terminated/exitCode")
                                .and_then(|v| v.as_i64())
                        })
                        .map(|code| code as i32),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn summarize_container_state(status: &Value) -> String {
    if let Some(reason) = status
        .pointer("/state/waiting/reason")
        .and_then(|v| v.as_str())
    {
        return format!("Waiting ({reason})");
    }

    if status.pointer("/state/running").is_some() {
        return "Running".to_string();
    }

    if let Some(reason) = status
        .pointer("/state/terminated/reason")
        .and_then(|v| v.as_str())
    {
        return format!("Terminated ({reason})");
    }

    "Unknown".to_string()
}

pub async fn fetch_events(namespace: &str) -> Result<Vec<ClusterEvent>> {
    let output = run_cmd(
        "kubectl",
        &["get", "events", "-n", namespace, "--sort-by=.lastTimestamp", "-o", "json"],
    )
    .await?;

    let json: Value = serde_json::from_str(&output)?;
    let mut events = Vec::new();

    if let Some(items) = json.get("items").and_then(|v| v.as_array()) {
        for item in items.iter().rev().take(50) {
            let kind = item
                .pointer("/involvedObject/kind")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let name = item
                .pointer("/involvedObject/name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();

            let reason = item
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let message = item
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let event_type = if item.get("type").and_then(|v| v.as_str()) == Some("Warning") {
                EventType::Warning
            } else {
                EventType::Normal
            };

            let count = item.get("count").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            let timestamp = item
                .get("lastTimestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            events.push(ClusterEvent {
                kind,
                name,
                reason,
                message,
                event_type,
                count,
                timestamp,
            });
        }
    }

    Ok(events)
}

pub async fn fetch_namespaces() -> Result<Vec<String>> {
    let output = run_cmd("kubectl", &["get", "namespaces", "--no-headers"]).await?;
    
    let mut namespaces: Vec<String> = output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.first().map(|s| s.to_string())
        })
        .collect();
    
    namespaces.sort();
    Ok(namespaces)
}

pub async fn fetch_current_context() -> Result<String> {
    let output = run_cmd("kubectl", &["config", "current-context"]).await?;
    let context = output.trim().to_string();
    if context.is_empty() {
        return Err(KubectlError::with_issue(
            ConnectionIssue {
                kind: ConnectionIssueKind::NoContext,
                namespace: None,
                detail: "No active kubectl context is configured.".to_string(),
            },
            "No active kubectl context is configured.",
        )
        .into());
    }
    Ok(context)
}

pub async fn fetch_current_namespace() -> Result<String> {
    let output = run_cmd(
        "kubectl",
        &["config", "view", "--minify", "-o", "jsonpath={.contexts[0].context.namespace}"],
    )
    .await?;
    let ns = output.trim().to_string();
    if ns.is_empty() {
        return Err(KubectlError::with_issue(
            ConnectionIssue {
                kind: ConnectionIssueKind::NamespaceUnavailable,
                namespace: None,
                detail: "No namespace is configured in the current kubectl context."
                    .to_string(),
            },
            "No namespace is configured in the current kubectl context.",
        )
        .into());
    } else {
        Ok(ns)
    }
}

pub fn classify_kubectl_error(err: &anyhow::Error) -> Option<ConnectionIssue> {
    err.downcast_ref::<KubectlError>()
        .and_then(|kubectl_err| kubectl_err.connection_issue().cloned())
}

fn classify_connection_issue(args: &[&str], stderr: &str) -> Option<ConnectionIssue> {
    let stderr_lower = stderr.to_ascii_lowercase();

    if stderr_lower.contains("current-context is not set")
        || stderr_lower.contains("no current context")
        || stderr_lower.contains("no context")
    {
        return Some(ConnectionIssue {
            kind: ConnectionIssueKind::NoContext,
            namespace: None,
            detail: stderr_trim_or_default(stderr, "No active kubectl context is configured."),
        });
    }

    if stderr_lower.contains("namespace") && stderr_lower.contains("not found") {
        if let Some(namespace) = requested_namespace(args) {
            return Some(ConnectionIssue {
                kind: ConnectionIssueKind::NamespaceUnavailable,
                namespace: Some(namespace),
                detail: stderr_trim_or_default(stderr, "Requested namespace was not found."),
            });
        }
    }

    None
}

fn requested_namespace(args: &[&str]) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if *arg == "-n" || *arg == "--namespace" {
            return iter.next().map(|ns| ns.to_string());
        }
    }
    None
}

fn stderr_trim_or_default(stderr: &str, default: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn parse_cpu(s: &str) -> u64 {
    if let Some(stripped) = s.strip_suffix('m') {
        stripped.parse().unwrap_or(0)
    } else if let Ok(cores) = s.parse::<f64>() {
        (cores * 1000.0) as u64
    } else {
        0
    }
}

pub fn parse_memory_mb(s: &str) -> u64 {
    if let Some(stripped) = s.strip_suffix("Ki") {
        let kb: u64 = stripped.parse().unwrap_or(0);
        // Keep fractional MB by rounding up for small values
        (kb + 512) / 1024
    } else if let Some(stripped) = s.strip_suffix("Mi") {
        stripped.parse().unwrap_or(0)
    } else if let Some(stripped) = s.strip_suffix("Gi") {
        let gb: u64 = stripped.parse().unwrap_or(0);
        gb * 1024
    } else if let Some(stripped) = s.strip_suffix('M') {
        stripped.parse().unwrap_or(0)
    } else if let Some(stripped) = s.strip_suffix('G') {
        let gb: u64 = stripped.parse().unwrap_or(0);
        gb * 1024
    } else {
        s.parse().unwrap_or(0)
    }
}

fn metadata_name(item: &Value) -> String {
    item.pointer("/metadata/name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn status_u32(item: &Value, pointer: &str) -> u32 {
    item.pointer(pointer)
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

fn find_condition_reason(item: &Value, condition_type: &str) -> Option<String> {
    item.pointer("/status/conditions")
        .and_then(|v| v.as_array())
        .and_then(|conditions| {
            conditions.iter().find_map(|condition| {
                if condition.get("type").and_then(|v| v.as_str()) == Some(condition_type) {
                    condition
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                } else {
                    None
                }
            })
        })
}

fn find_condition_status(item: &Value, condition_type: &str) -> Option<ConditionStatus> {
    item.pointer("/status/conditions")
        .and_then(|v| v.as_array())
        .and_then(|conditions| {
            conditions.iter().find_map(|condition| {
                if condition.get("type").and_then(|v| v.as_str()) == Some(condition_type) {
                    Some(parse_condition_status(
                        condition
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown"),
                    ))
                } else {
                    None
                }
            })
        })
}

fn build_deployment_replicaset_map(replicasets_json: &Value) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();

    if let Some(items) = replicasets_json.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let rs_name = metadata_name(item);
            if rs_name.is_empty() {
                continue;
            }

            if let Some(owner_refs) = item.pointer("/metadata/ownerReferences").and_then(|v| v.as_array()) {
                for owner in owner_refs {
                    if owner.get("kind").and_then(|v| v.as_str()) == Some("Deployment") {
                        if let Some(deployment_name) = owner.get("name").and_then(|v| v.as_str()) {
                            map.entry(deployment_name.to_string())
                                .or_default()
                                .push(rs_name.clone());
                        }
                    }
                }
            }
        }
    }

    map
}

fn workload_health(
    unavailable: u32,
    progressing_reason: Option<&str>,
    available_condition: Option<ConditionStatus>,
) -> HealthStatus {
    if unavailable > 0 {
        HealthStatus::Warning
    } else if matches!(progressing_reason, Some("ProgressDeadlineExceeded"))
        || matches!(available_condition, Some(ConditionStatus::False))
    {
        HealthStatus::Critical
    } else {
        HealthStatus::Healthy
    }
}

fn deployment_rollout_status(
    desired: u32,
    ready: u32,
    updated: u32,
    unavailable: u32,
    progressing_reason: Option<&str>,
    available_condition: Option<ConditionStatus>,
) -> String {
    if matches!(progressing_reason, Some("ProgressDeadlineExceeded")) {
        return "deadline exceeded".to_string();
    }
    if desired == 0 {
        return "scaled to zero".to_string();
    }
    if unavailable > 0 {
        return format!("rollout in progress: {ready}/{desired} ready, {unavailable} unavailable");
    }
    if updated < desired {
        return format!("updating replicas: {updated}/{desired} updated");
    }
    if matches!(available_condition, Some(ConditionStatus::False)) {
        return "waiting for available replicas".to_string();
    }
    "rolled out".to_string()
}

fn statefulset_rollout_status(
    desired: u32,
    ready: u32,
    updated: u32,
    current: u32,
    unavailable: u32,
    update_revision: &str,
    current_revision: &str,
) -> String {
    if desired == 0 {
        return "scaled to zero".to_string();
    }
    if unavailable > 0 {
        return format!("rolling update: {ready}/{desired} ready, {unavailable} unavailable");
    }
    if updated < desired {
        return format!("updating pods: {updated}/{desired} updated");
    }
    if !update_revision.is_empty() && !current_revision.is_empty() && update_revision != current_revision
    {
        return format!("revision shift: {current}/{desired} current");
    }
    "steady".to_string()
}

fn daemonset_rollout_status(
    desired: u32,
    ready: u32,
    available: u32,
    updated: u32,
    unavailable: u32,
) -> String {
    if desired == 0 {
        return "no scheduled pods".to_string();
    }
    if unavailable > 0 {
        return format!("rolling update: {ready}/{desired} ready, {unavailable} unavailable");
    }
    if updated < desired {
        return format!("updating nodes: {updated}/{desired} updated");
    }
    if available < desired {
        return format!("waiting for availability: {available}/{desired}");
    }
    "steady".to_string()
}

fn calculate_age(timestamp: &str) -> String {
    // Try RFC3339 first, then RFC2822, then other common formats
    let parsed = chrono::DateTime::parse_from_rfc3339(timestamp)
        .or_else(|_| chrono::DateTime::parse_from_rfc2822(timestamp));
    
    if let Ok(t) = parsed {
        let now = chrono::Utc::now();
        // Use safe duration calculation to avoid panics
        let duration = now.signed_duration_since(t);
        let secs = duration.num_seconds().max(0);

        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m", secs / 60)
        } else if secs < 86400 {
            format!("{}h", secs / 3600)
        } else {
            format!("{}d", secs / 86400)
        }
    } else {
        "?".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attach_workload_events, build_node_metrics, deployment_rollout_status,
        derive_node_status, derive_pod_status, effective_pod_cpu_resources,
        effective_pod_memory_resources, ensure_readonly_kubectl_args, workload_health,
    };
    use crate::data::models::{
        ClusterEvent, ConditionStatus, EventType, HealthStatus, WorkloadKind, WorkloadSummary,
    };
    use serde_json::json;

    #[test]
    fn allows_readonly_kubectl_commands() {
        assert!(ensure_readonly_kubectl_args("kubectl", &["get", "pods"]).is_ok());
        assert!(ensure_readonly_kubectl_args("kubectl", &["top", "nodes"]).is_ok());
        assert!(ensure_readonly_kubectl_args("kubectl", &["logs", "pod-1", "-f"]).is_ok());
        assert!(ensure_readonly_kubectl_args("kubectl", &["config", "view"]).is_ok());
    }

    #[test]
    fn rejects_mutating_or_unknown_kubectl_commands() {
        assert!(ensure_readonly_kubectl_args("kubectl", &["delete", "pod", "x"]).is_err());
        assert!(ensure_readonly_kubectl_args("kubectl", &["apply", "-f", "x.yaml"]).is_err());
        assert!(ensure_readonly_kubectl_args("kubectl", &["config", "set-context", "x"]).is_err());
        assert!(ensure_readonly_kubectl_args("bash", &["-lc", "kubectl get pods"]).is_err());
    }

    #[test]
    fn deployment_rollout_status_reports_progress_and_deadline_exceeded() {
        assert_eq!(
            deployment_rollout_status(5, 4, 5, 1, None, None),
            "rollout in progress: 4/5 ready, 1 unavailable"
        );
        assert_eq!(
            deployment_rollout_status(
                5,
                2,
                2,
                3,
                Some("ProgressDeadlineExceeded"),
                None,
            ),
            "deadline exceeded"
        );
    }

    #[test]
    fn attach_workload_events_includes_deployment_replicaset_events() {
        let mut workloads = vec![WorkloadSummary {
            kind: WorkloadKind::Deployment,
            name: "api".to_string(),
            namespace: "default".to_string(),
            desired_replicas: 3,
            ready_replicas: 2,
            available_replicas: 2,
            updated_replicas: Some(3),
            current_replicas: None,
            unavailable_pods: 1,
            rollout_status: "rollout in progress".to_string(),
            status: HealthStatus::Warning,
            recent_events: vec![],
            related_event_targets: vec![
                ("Deployment".to_string(), "api".to_string()),
                ("ReplicaSet".to_string(), "api-5d9c7".to_string()),
            ],
        }];

        let events = vec![
            ClusterEvent {
                kind: "ReplicaSet".to_string(),
                name: "api-5d9c7".to_string(),
                reason: "SuccessfulCreate".to_string(),
                message: "Created pod: api-abc".to_string(),
                event_type: EventType::Normal,
                count: 1,
                timestamp: "2026-03-09T10:00:00Z".to_string(),
            },
            ClusterEvent {
                kind: "Pod".to_string(),
                name: "api-abc".to_string(),
                reason: "Started".to_string(),
                message: "Started container".to_string(),
                event_type: EventType::Normal,
                count: 1,
                timestamp: "2026-03-09T09:59:00Z".to_string(),
            },
        ];

        attach_workload_events(&mut workloads, &events);

        assert_eq!(workloads[0].recent_events.len(), 1);
        assert_eq!(workloads[0].recent_events[0].kind, "ReplicaSet");
    }

    #[test]
    fn workload_health_marks_unavailable_workloads_warning() {
        assert_eq!(workload_health(1, None, None), HealthStatus::Warning);
        assert_eq!(
            workload_health(0, Some("ProgressDeadlineExceeded"), None),
            HealthStatus::Critical
        );
    }

    #[test]
    fn pod_resources_use_max_of_init_containers_not_sum() {
        let app_containers = vec![json!({
            "resources": {
                "requests": { "cpu": "300m", "memory": "256Mi" },
                "limits": { "cpu": "600m", "memory": "512Mi" }
            }
        })];
        let init_containers = vec![
            json!({
                "resources": {
                    "requests": { "cpu": "500m", "memory": "1Gi" },
                    "limits": { "cpu": "700m", "memory": "1Gi" }
                }
            }),
            json!({
                "resources": {
                    "requests": { "cpu": "200m", "memory": "128Mi" },
                    "limits": { "cpu": "300m", "memory": "256Mi" }
                }
            }),
        ];

        assert_eq!(
            effective_pod_cpu_resources(Some(&app_containers), Some(&init_containers)),
            (500, 700)
        );
        assert_eq!(
            effective_pod_memory_resources(Some(&app_containers), Some(&init_containers)),
            (1024, 1024)
        );
    }

    #[test]
    fn includes_nodes_without_top_metrics() {
        let info_json = json!({
            "items": [{
                "metadata": { "name": "node-a" },
                "spec": { "unschedulable": false },
                "status": {
                    "allocatable": { "memory": "2048Mi" },
                    "capacity": { "cpu": "2", "memory": "4096Mi" },
                    "conditions": [
                        { "type": "Ready", "status": "False" }
                    ]
                }
            }]
        });

        let nodes = build_node_metrics("", &info_json);

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "node-a");
        assert!(!nodes[0].ready);
        assert_eq!(nodes[0].status, HealthStatus::Critical);
    }

    #[test]
    fn derived_statuses_reflect_runtime_health_not_just_memory() {
        assert_eq!(derive_node_status(10, false, 0), HealthStatus::Critical);
        assert_eq!(
            derive_pod_status(10, "Running", false, false, false),
            HealthStatus::Critical
        );
        assert_eq!(
            derive_pod_status(10, "Pending", true, false, false),
            HealthStatus::Warning
        );
        assert_eq!(
            derive_pod_status(10, "Running", true, false, false),
            HealthStatus::Healthy
        );
        assert_eq!(ConditionStatus::True.as_str(), "True");
    }
}
