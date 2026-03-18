#![allow(dead_code)]

use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Critical,
    Warning,
    Elevated,
    Healthy,
}

impl HealthStatus {
    pub fn from_pct(pct: u8) -> Self {
        // Align thresholds with health score grade boundaries for consistency
        // Grade: A>=90, B>=75, C>=60, D>=45, F<45
        if pct >= 90 {
            Self::Critical
        } else if pct >= 75 {
            Self::Warning
        } else if pct >= 60 {
            Self::Elevated
        } else {
            Self::Healthy
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}

impl ConditionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::True => "True",
            Self::False => "False",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NodeConditions {
    pub ready: ConditionStatus,
    pub memory_pressure: ConditionStatus,
    pub disk_pressure: ConditionStatus,
    pub pid_pressure: ConditionStatus,
    pub network_unavailable: ConditionStatus,
}

#[derive(Debug, Clone)]
pub struct NodeMetric {
    pub name: String,
    pub cpu_millicores: u64,
    pub memory_mb: u64,
    pub memory_total_mb: u64,
    pub memory_pct: u8,
    pub cpu_pct: u8,
    pub status: HealthStatus,
    pub cpu_capacity: u64,
    pub memory_capacity_mb: u64,
    pub unhealthy_conditions: u8,
    pub ready: bool,
    pub conditions: NodeConditions,
    pub cordoned: bool,
    pub draining: bool,
    pub node_info: Option<NodeInfo>,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub kernel_version: String,
    pub os_image: String,
    pub container_runtime: String,
    pub kubelet_version: String,
    pub architecture: String,
    pub operating_system: String,
    pub kernel: String,
}

#[derive(Debug, Clone)]
pub struct ContainerInfo {
    pub name: String,
    pub ready: bool,
    pub restart_count: u32,
    pub state: String,
    pub last_termination_reason: Option<String>,
    pub last_exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PodInfo {
    pub uid: String,
    pub name: String,
    pub namespace: String,
    pub phase: String,
    pub restarts: u32,
    pub age: String,
    pub cpu_millicores: u64,
    pub cpu_request_millicores: u64,
    pub cpu_limit_millicores: u64,
    pub memory_mb: u64,
    pub memory_request_mb: u64,
    pub memory_limit_mb: u64,
    pub memory_request_pct: u8,
    pub memory_pct: u8,
    pub cpu_request_pct: u8,
    pub cpu_pct: u8,
    pub status: HealthStatus,
    pub ready_containers: u32,
    pub total_containers: u32,
    pub is_ready: bool,
    pub crash_looping: bool,
    pub oom_killed: bool,
    pub node_name: Option<String>,
    pub containers: Vec<ContainerInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkloadKind {
    Deployment,
    StatefulSet,
    DaemonSet,
}

impl WorkloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Deployment => "Deploy",
            Self::StatefulSet => "Sts",
            Self::DaemonSet => "Ds",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkloadSummary {
    pub kind: WorkloadKind,
    pub name: String,
    pub namespace: String,
    pub desired_replicas: u32,
    pub ready_replicas: u32,
    pub available_replicas: u32,
    pub updated_replicas: Option<u32>,
    pub current_replicas: Option<u32>,
    pub unavailable_pods: u32,
    pub rollout_status: String,
    pub status: HealthStatus,
    pub recent_events: Vec<ClusterEvent>,
    pub related_event_targets: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ResourceUsageSummary {
    pub pod_count: usize,
    pub cpu_usage_millicores: u64,
    pub cpu_request_millicores: u64,
    pub cpu_limit_millicores: u64,
    pub memory_usage_mb: u64,
    pub memory_request_mb: u64,
    pub memory_limit_mb: u64,
}

#[derive(Debug, Clone)]
pub struct PodHistory {
    pub name: String,
    pub namespace: String,
    pub memory_samples: std::collections::VecDeque<u8>,
}

impl PodHistory {
    pub fn new(name: String, namespace: String) -> Self {
        Self {
            name,
            namespace,
            memory_samples: std::collections::VecDeque::new(),
        }
    }

    pub fn add_sample(&mut self, memory_pct: u8, max_samples: usize) {
        self.memory_samples.push_back(memory_pct);
        if self.memory_samples.len() > max_samples {
            self.memory_samples.pop_front();
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventType {
    Warning,
    Normal,
}

#[derive(Debug, Clone)]
pub struct ClusterEvent {
    pub kind: String,
    pub name: String,
    pub reason: String,
    pub message: String,
    pub event_type: EventType,
    pub count: u32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncidentSeverity {
    Critical,
    Warning,
    Elevated,
}

impl IncidentSeverity {
    pub fn rank(self) -> u8 {
        match self {
            Self::Critical => 3,
            Self::Warning => 2,
            Self::Elevated => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Warning => "warning",
            Self::Elevated => "elevated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncidentTarget {
    Pod {
        pod_name: String,
    },
    Node {
        node_name: String,
    },
    Workload {
        kind: String,
        name: String,
    },
    Container {
        pod_name: String,
        container_name: String,
    },
}

impl IncidentTarget {
    pub fn display_label(&self) -> String {
        match self {
            Self::Pod { pod_name } => format!("Pod/{pod_name}"),
            Self::Node { node_name } => format!("Node/{node_name}"),
            Self::Workload { kind, name } => format!("{kind}/{name}"),
            Self::Container {
                pod_name,
                container_name,
            } => format!("Pod/{pod_name}/{container_name}"),
        }
    }

    pub fn pod_name(&self) -> Option<&str> {
        match self {
            Self::Pod { pod_name } | Self::Container { pod_name, .. } => Some(pod_name),
            _ => None,
        }
    }

    pub fn node_name(&self) -> Option<&str> {
        match self {
            Self::Node { node_name } => Some(node_name),
            _ => None,
        }
    }

    pub fn workload_key(&self) -> Option<(&str, &str)> {
        match self {
            Self::Workload { kind, name } => Some((kind, name)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IncidentBucket {
    pub reason: String,
    pub severity: IncidentSeverity,
    pub occurrences: u32,
    pub targets: Vec<IncidentTarget>,
    pub affected_resources: Vec<String>,
    pub latest_timestamp: String,
    pub sample_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionIssueKind {
    KubectlMissing,
    NoContext,
    NamespaceUnavailable,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionIssue {
    pub kind: ConnectionIssueKind,
    pub namespace: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct HealthScore {
    pub score: u8,
    pub grade: char,
    pub critical_nodes: u32,
    pub critical_pods: u32,
    pub total_restarts: u32,
}

#[derive(Debug, Clone)]
pub struct ClusterSnapshot {
    pub nodes: Vec<NodeMetric>,
    pub workloads: Vec<WorkloadSummary>,
    pub pods: Vec<PodInfo>,
    pub events: Vec<ClusterEvent>,
    pub incident_buckets: Vec<IncidentBucket>,
    pub health: HealthScore,
    pub fetched_at: Instant,
    pub error: Option<String>,
    pub context_name: Option<String>,
}
