use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::data::models::{
    ClusterSnapshot, HealthScore, IncidentBucket, IncidentSeverity, IncidentTarget, NodeMetric,
    PodInfo,
};

/// JSON schema version for all outputs. Increment when making breaking changes.
pub const JSON_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonOutput<T> {
    pub version: String,
    pub data: T,
}

impl<T> JsonOutput<T> {
    pub fn new(data: T) -> Self {
        Self { version: JSON_SCHEMA_VERSION.to_string(), data }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthOutput {
    pub score: u8,
    pub grade: char,
    pub critical_nodes: u32,
    pub critical_pods: u32,
    pub total_restarts: u32,
    pub context: Option<String>,
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub findings: Option<Vec<Finding>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Finding {
    pub category: String,
    pub severity: String,
    pub message: String,
}

impl From<&HealthScore> for HealthOutput {
    fn from(health: &HealthScore) -> Self {
        Self {
            score: health.score,
            grade: health.grade,
            critical_nodes: health.critical_nodes,
            critical_pods: health.critical_pods,
            total_restarts: health.total_restarts,
            context: None,
            namespace: String::new(),
            findings: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IncidentOutput {
    pub reason: String,
    pub severity: String,
    pub occurrences: u32,
    pub affected_resources: Vec<String>,
    pub targets: Vec<IncidentTargetOutput>,
    pub latest_timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IncidentTargetOutput {
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
}

impl From<&IncidentTarget> for IncidentTargetOutput {
    fn from(target: &IncidentTarget) -> Self {
        match target {
            IncidentTarget::Pod { pod_name } => {
                Self { kind: "Pod".to_string(), name: pod_name.clone(), container: None }
            }
            IncidentTarget::Node { node_name } => {
                Self { kind: "Node".to_string(), name: node_name.clone(), container: None }
            }
            IncidentTarget::Workload { kind, name } => {
                Self { kind: kind.clone(), name: name.clone(), container: None }
            }
            IncidentTarget::Container { pod_name, container_name } => Self {
                kind: "Container".to_string(),
                name: pod_name.clone(),
                container: Some(container_name.clone()),
            },
            IncidentTarget::Resource { kind, name, .. } => {
                Self { kind: kind.clone(), name: name.clone(), container: None }
            }
        }
    }
}

impl From<&IncidentBucket> for IncidentOutput {
    fn from(bucket: &IncidentBucket) -> Self {
        Self {
            reason: bucket.reason.clone(),
            severity: match bucket.severity {
                IncidentSeverity::Critical => "critical".to_string(),
                IncidentSeverity::Warning => "warning".to_string(),
                IncidentSeverity::Elevated => "elevated".to_string(),
            },
            occurrences: bucket.occurrences,
            affected_resources: bucket.affected_resources.clone(),
            targets: bucket.targets.iter().map(Into::into).collect(),
            latest_timestamp: bucket.latest_timestamp.clone(),
            sample_message: bucket.sample_message.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PodOutput {
    pub name: String,
    pub namespace: String,
    pub phase: String,
    pub restarts: u32,
    pub age: String,
    pub node: Option<String>,
    pub cpu_millicores: u64,
    pub memory_mb: u64,
    pub cpu_pct: u8,
    pub memory_pct: u8,
    pub status: String,
    pub is_ready: bool,
    pub crash_looping: bool,
    pub oom_killed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers: Option<Vec<ContainerOutput>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerOutput {
    pub name: String,
    pub ready: bool,
    pub restart_count: u32,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_termination_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exit_code: Option<i32>,
}

impl From<&PodInfo> for PodOutput {
    fn from(pod: &PodInfo) -> Self {
        Self {
            name: pod.name.clone(),
            namespace: pod.namespace.clone(),
            phase: pod.phase.clone(),
            restarts: pod.restarts,
            age: pod.age.clone(),
            node: pod.node_name.clone(),
            cpu_millicores: pod.cpu_millicores,
            memory_mb: pod.memory_mb,
            cpu_pct: pod.cpu_pct,
            memory_pct: pod.memory_pct,
            status: format!("{:?}", pod.status).to_lowercase(),
            is_ready: pod.is_ready,
            crash_looping: pod.crash_looping,
            oom_killed: pod.oom_killed,
            containers: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeOutput {
    pub name: String,
    pub ready: bool,
    pub cpu_millicores: u64,
    pub cpu_capacity: u64,
    pub cpu_pct: u8,
    pub memory_mb: u64,
    pub memory_capacity_mb: u64,
    pub memory_pct: u8,
    pub status: String,
    pub unhealthy_conditions: u8,
    pub cordoned: bool,
    pub draining: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_info: Option<NodeInfoOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeInfoOutput {
    pub kernel_version: String,
    pub os_image: String,
    pub container_runtime: String,
    pub kubelet_version: String,
    pub architecture: String,
    pub operating_system: String,
}

impl From<&NodeMetric> for NodeOutput {
    fn from(node: &NodeMetric) -> Self {
        Self {
            name: node.name.clone(),
            ready: node.ready,
            cpu_millicores: node.cpu_millicores,
            cpu_capacity: node.cpu_capacity,
            cpu_pct: node.cpu_pct,
            memory_mb: node.memory_mb,
            memory_capacity_mb: node.memory_capacity_mb,
            memory_pct: node.memory_pct,
            status: format!("{:?}", node.status).to_lowercase(),
            unhealthy_conditions: node.unhealthy_conditions,
            cordoned: node.cordoned,
            draining: node.draining,
            node_info: node.node_info.as_ref().map(|info| NodeInfoOutput {
                kernel_version: info.kernel_version.clone(),
                os_image: info.os_image.clone(),
                container_runtime: info.container_runtime.clone(),
                kubelet_version: info.kubelet_version.clone(),
                architecture: info.architecture.clone(),
                operating_system: info.operating_system.clone(),
            }),
            conditions: None,
        }
    }
}

pub fn build_health_output(
    snapshot: &ClusterSnapshot,
    namespace: &str,
    verbose: bool,
) -> HealthOutput {
    let mut output = HealthOutput::from(&snapshot.health);
    output.context = snapshot.context_name.clone();
    output.namespace = namespace.to_string();

    if verbose {
        let mut findings = Vec::new();

        if output.critical_nodes > 0 {
            findings.push(Finding {
                category: "nodes".to_string(),
                severity: "critical".to_string(),
                message: format!("{} critical node(s) detected", output.critical_nodes),
            });
        }

        if output.critical_pods > 0 {
            findings.push(Finding {
                category: "pods".to_string(),
                severity: "critical".to_string(),
                message: format!("{} critical pod(s) detected", output.critical_pods),
            });
        }

        if output.total_restarts > 10 {
            findings.push(Finding {
                category: "restarts".to_string(),
                severity: "warning".to_string(),
                message: format!("{} total pod restarts", output.total_restarts),
            });
        }

        for incident in snapshot.incident_buckets.iter().take(5) {
            let severity = match incident.severity {
                IncidentSeverity::Critical => "critical",
                IncidentSeverity::Warning => "warning",
                IncidentSeverity::Elevated => "elevated",
            };
            findings.push(Finding {
                category: "incident".to_string(),
                severity: severity.to_string(),
                message: format!(
                    "{}: {} occurrence(s) affecting {} resource(s)",
                    incident.reason,
                    incident.occurrences,
                    incident.affected_resources.len()
                ),
            });
        }

        output.findings = Some(findings);
    }

    output
}

pub fn build_incidents_output(snapshot: &ClusterSnapshot, verbose: bool) -> Vec<IncidentOutput> {
    let incidents: Vec<IncidentOutput> = snapshot.incident_buckets.iter().map(Into::into).collect();

    if verbose { incidents } else { incidents.into_iter().take(10).collect() }
}

pub fn build_pods_output(snapshot: &ClusterSnapshot, verbose: bool) -> Vec<PodOutput> {
    snapshot
        .pods
        .iter()
        .map(|pod| {
            let mut output = PodOutput::from(pod);
            if verbose {
                output.containers = Some(
                    pod.running_containers()
                        .map(|c| ContainerOutput {
                            name: c.name.clone(),
                            ready: c.ready,
                            restart_count: c.restart_count,
                            state: c.state.clone(),
                            last_termination_reason: c.last_termination_reason.clone(),
                            last_exit_code: c.last_exit_code,
                        })
                        .collect(),
                );
            }
            output
        })
        .collect()
}

pub fn build_nodes_output(snapshot: &ClusterSnapshot, verbose: bool) -> Vec<NodeOutput> {
    snapshot
        .nodes
        .iter()
        .map(|node| {
            let mut output = NodeOutput::from(node);
            if verbose {
                let mut conditions = HashMap::new();
                conditions.insert("ready".to_string(), node.conditions.ready.as_str().to_string());
                conditions.insert(
                    "memory_pressure".to_string(),
                    node.conditions.memory_pressure.as_str().to_string(),
                );
                conditions.insert(
                    "disk_pressure".to_string(),
                    node.conditions.disk_pressure.as_str().to_string(),
                );
                conditions.insert(
                    "pid_pressure".to_string(),
                    node.conditions.pid_pressure.as_str().to_string(),
                );
                conditions.insert(
                    "network_unavailable".to_string(),
                    node.conditions.network_unavailable.as_str().to_string(),
                );
                output.conditions = Some(conditions);
            }
            output
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::*;

    fn sample_health() -> HealthScore {
        HealthScore {
            score: 85,
            grade: 'B',
            critical_nodes: 1,
            critical_pods: 2,
            total_restarts: 5,
        }
    }

    fn sample_incident() -> IncidentBucket {
        IncidentBucket {
            reason: "CrashLoopBackOff".to_string(),
            severity: IncidentSeverity::Critical,
            occurrences: 3,
            targets: vec![IncidentTarget::Pod { pod_name: "test-pod".to_string() }],
            affected_resources: vec!["Pod/test-pod".to_string()],
            latest_timestamp: "2024-01-01T00:00:00Z".to_string(),
            sample_message: Some("Container crash loop".to_string()),
        }
    }

    fn sample_pod() -> PodInfo {
        PodInfo {
            status_reason: None,
            uid: "test-uid".to_string(),
            name: "test-pod".to_string(),
            namespace: "default".to_string(),
            phase: "Running".to_string(),
            restarts: 2,
            age: "1h".to_string(),
            cpu_millicores: 100,
            cpu_request_millicores: 100,
            cpu_limit_millicores: 200,
            memory_mb: 128,
            memory_request_mb: 128,
            memory_limit_mb: 256,
            memory_request_pct: 50,
            memory_pct: 50,
            cpu_request_pct: 50,
            cpu_pct: 50,
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
            running_limits: Default::default(),
            node_name: Some("node-1".to_string()),
            containers: vec![],
        }
    }

    #[test]
    fn test_json_schema_version() {
        assert_eq!(JSON_SCHEMA_VERSION, "1.0.0");
    }

    #[test]
    fn test_health_output_structure() {
        let health = sample_health();
        let output = HealthOutput::from(&health);

        assert_eq!(output.score, 85);
        assert_eq!(output.grade, 'B');
        assert_eq!(output.critical_nodes, 1);
        assert_eq!(output.critical_pods, 2);
        assert_eq!(output.total_restarts, 5);
    }

    #[test]
    fn test_incident_output_structure() {
        let incident = sample_incident();
        let output = IncidentOutput::from(&incident);

        assert_eq!(output.reason, "CrashLoopBackOff");
        assert_eq!(output.severity, "critical");
        assert_eq!(output.occurrences, 3);
        assert_eq!(output.affected_resources.len(), 1);
        assert_eq!(output.targets.len(), 1);
        assert!(output.sample_message.is_some());
    }

    #[test]
    fn test_pod_output_structure() {
        let pod = sample_pod();
        let output = PodOutput::from(&pod);

        assert_eq!(output.name, "test-pod");
        assert_eq!(output.namespace, "default");
        assert_eq!(output.phase, "Running");
        assert_eq!(output.restarts, 2);
        assert!(output.is_ready);
        assert!(!output.crash_looping);
        assert!(!output.oom_killed);
    }

    #[test]
    fn test_json_output_wrapper() {
        let output = JsonOutput::new("test");
        assert_eq!(output.version, "1.0.0");
        assert_eq!(output.data, "test");

        // Ensure it serializes correctly
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"version\":\"1.0.0\""));
        assert!(json.contains("\"data\":\"test\""));
    }

    #[test]
    fn test_incident_target_output_conversion() {
        let pod_target = IncidentTarget::Pod { pod_name: "test-pod".to_string() };
        let output = IncidentTargetOutput::from(&pod_target);
        assert_eq!(output.kind, "Pod");
        assert_eq!(output.name, "test-pod");
        assert!(output.container.is_none());

        let container_target = IncidentTarget::Container {
            pod_name: "test-pod".to_string(),
            container_name: "app".to_string(),
        };
        let output = IncidentTargetOutput::from(&container_target);
        assert_eq!(output.kind, "Container");
        assert_eq!(output.name, "test-pod");
        assert_eq!(output.container, Some("app".to_string()));
    }

    #[test]
    fn test_severity_mapping() {
        let incidents = [
            IncidentBucket {
                reason: "Test1".to_string(),
                severity: IncidentSeverity::Critical,
                occurrences: 1,
                targets: vec![],
                affected_resources: vec![],
                latest_timestamp: String::new(),
                sample_message: None,
            },
            IncidentBucket {
                reason: "Test2".to_string(),
                severity: IncidentSeverity::Warning,
                occurrences: 1,
                targets: vec![],
                affected_resources: vec![],
                latest_timestamp: String::new(),
                sample_message: None,
            },
            IncidentBucket {
                reason: "Test3".to_string(),
                severity: IncidentSeverity::Elevated,
                occurrences: 1,
                targets: vec![],
                affected_resources: vec![],
                latest_timestamp: String::new(),
                sample_message: None,
            },
        ];

        let outputs: Vec<IncidentOutput> = incidents.iter().map(Into::into).collect();
        assert_eq!(outputs[0].severity, "critical");
        assert_eq!(outputs[1].severity, "warning");
        assert_eq!(outputs[2].severity, "elevated");
    }
}
