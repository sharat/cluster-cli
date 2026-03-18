use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::time::Instant;

use crate::config::Config;
use crate::data::models::{
    ClusterSnapshot, ConnectionIssue, HealthStatus, IncidentBucket, IncidentTarget, PodInfo,
    ResourceUsageSummary,
};

const MAX_HISTORY_SAMPLES: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub enum AppView {
    Dashboard,
    PodDetail { pod_name: String },
    NodeDetail { node_name: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    Nodes,
    Pods,
    Events,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodSortMode {
    Default,
    Restarts,
    Cpu,
    Memory,
    NotReady,
    LatestIncident,
}

impl PodSortMode {
    pub fn next(self) -> Self {
        match self {
            Self::Default => Self::Restarts,
            Self::Restarts => Self::Cpu,
            Self::Cpu => Self::Memory,
            Self::Memory => Self::NotReady,
            Self::NotReady => Self::LatestIncident,
            Self::LatestIncident => Self::Default,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Restarts => "restarts",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::NotReady => "not-ready",
            Self::LatestIncident => "latest-incident",
        }
    }
}

impl Panel {
    pub fn next(&self) -> Self {
        match self {
            Panel::Nodes => Panel::Events,
            Panel::Events => Panel::Pods,
            Panel::Pods => Panel::Nodes,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PodDetailSection {
    Overview,
    Events,
    Logs,
}

impl PodDetailSection {
    pub fn next(&self) -> Self {
        match self {
            PodDetailSection::Overview => PodDetailSection::Events,
            PodDetailSection::Events => PodDetailSection::Logs,
            PodDetailSection::Logs => PodDetailSection::Overview,
        }
    }
}

pub struct AppState {
    pub config: Config,
    pub view: AppView,
    pub snapshot: Option<ClusterSnapshot>,
    pub pod_filter: String,
    pub pod_sort_mode: PodSortMode,
    pub filter_active: bool,
    pub ns_input_active: bool,
    pub ns_input: String,
    pub ns_list_active: bool,
    pub ns_list: Vec<String>,
    pub ns_list_cursor: usize,
    pub refresh_input_active: bool,
    pub refresh_input: String,
    pub workload_popup_active: bool,
    pub export_input_active: bool,
    pub export_input: String,
    pub focused_panel: Panel,
    pub node_cursor: usize,
    pub workload_cursor: usize,
    pub pod_cursor: usize,
    pub event_cursor: usize,
    pub pod_detail_section: PodDetailSection,
    pub log_buffer: VecDeque<String>,
    pub log_follow: bool,
    pub detail_scroll: usize,
    pub status_message: Option<(String, Instant)>,
    pub is_loading: bool,
    pub incident_focus: Option<IncidentFocus>,
    pub connection_issue: Option<ConnectionIssue>,
    pub pod_memory_history: HashMap<String, Vec<u8>>,
    pub pod_cpu_history: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct IncidentFocus {
    pub reason: String,
    pub pod_names: HashSet<String>,
    pub workload_names: HashSet<String>,
}

impl IncidentFocus {
    pub fn matches(&self, pod: &PodInfo) -> bool {
        if self.pod_names.is_empty() && self.workload_names.is_empty() {
            return true;
        }

        self.pod_names.contains(&pod.name)
            || self
                .workload_names
                .iter()
                .any(|name| contains_case_insensitive(&pod.name, name))
    }
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            view: AppView::Dashboard,
            snapshot: None,
            pod_filter: String::new(),
            pod_sort_mode: PodSortMode::Default,
            filter_active: false,
            ns_input_active: false,
            ns_input: String::new(),
            ns_list_active: false,
            ns_list: Vec::new(),
            ns_list_cursor: 0,
            refresh_input_active: false,
            refresh_input: String::new(),
            workload_popup_active: false,
            export_input_active: false,
            export_input: String::new(),
            focused_panel: Panel::Pods,
            node_cursor: 0,
            workload_cursor: 0,
            pod_cursor: 0,
            event_cursor: 0,
            pod_detail_section: PodDetailSection::Overview,
            log_buffer: VecDeque::new(),
            log_follow: true,
            detail_scroll: 0,
            status_message: None,
            is_loading: true,
            incident_focus: None,
            connection_issue: None,
            pod_memory_history: HashMap::new(),
            pod_cpu_history: HashMap::new(),
        }
    }

    pub fn update_pod_history(&mut self, pods: &[PodInfo]) {
        let active_keys: HashSet<String> = pods.iter().map(pod_history_key).collect();
        self.pod_memory_history
            .retain(|key, _| active_keys.contains(key));
        self.pod_cpu_history
            .retain(|key, _| active_keys.contains(key));

        for pod in pods {
            let key = pod_history_key(pod);
            let memory_history = self
                .pod_memory_history
                .entry(key.clone())
                .or_default();
            memory_history.push(pod.memory_pct);
            if memory_history.len() > MAX_HISTORY_SAMPLES {
                memory_history.remove(0);
            }

            let cpu_history = self.pod_cpu_history.entry(key).or_default();
            cpu_history.push(pod.cpu_pct);
            if cpu_history.len() > MAX_HISTORY_SAMPLES {
                cpu_history.remove(0);
            }
        }
    }

    pub fn get_pod_memory_history(&self, pod: &PodInfo) -> Option<&Vec<u8>> {
        self.pod_memory_history.get(&pod_history_key(pod))
    }

    pub fn get_pod_cpu_history(&self, pod: &PodInfo) -> Option<&Vec<u8>> {
        self.pod_cpu_history.get(&pod_history_key(pod))
    }

    pub fn filtered_pods(&self) -> Vec<&PodInfo> {
        let Some(snap) = &self.snapshot else {
            return vec![];
        };

        let incident_focus = self.incident_focus.as_ref();
        let mut pods: Vec<&PodInfo> = snap
            .pods
            .iter()
            .filter(|pod| {
                let matches_manual = self.pod_filter.is_empty()
                    || contains_case_insensitive(&pod.name, &self.pod_filter);
                let matches_incident = incident_focus
                    .map(|focus| focus.matches(pod))
                    .unwrap_or(true);
                matches_manual && matches_incident
            })
            .collect();

        if self.pod_sort_mode != PodSortMode::Default {
            let latest_incidents = matches!(self.pod_sort_mode, PodSortMode::LatestIncident)
                .then(|| self.latest_pod_incidents());
            pods.sort_by(|a, b| self.compare_pods(a, b, latest_incidents.as_ref()));
        }

        pods
    }

    pub fn filtered_pod_count(&self) -> usize {
        self.filtered_pods().len()
    }

    pub fn namespace_resource_summary(&self) -> ResourceUsageSummary {
        let mut summary = ResourceUsageSummary::default();

        if let Some(snap) = &self.snapshot {
            // Filter pods by current namespace to ensure accurate summary
            let namespace_pods: Vec<&PodInfo> = snap
                .pods
                .iter()
                .filter(|p| p.namespace == self.config.namespace)
                .collect();

            summary.pod_count = namespace_pods.len();
            for pod in namespace_pods {
                summary.cpu_usage_millicores += pod.cpu_millicores;
                summary.cpu_request_millicores += pod.cpu_request_millicores;
                summary.cpu_limit_millicores += pod.cpu_limit_millicores;
                summary.memory_usage_mb += pod.memory_mb;
                summary.memory_request_mb += pod.memory_request_mb;
                summary.memory_limit_mb += pod.memory_limit_mb;
            }
        }

        summary
    }

    pub fn enter_pod_detail(&mut self, pod_name: String) {
        self.view = AppView::PodDetail { pod_name };
        self.log_buffer.clear();
        self.detail_scroll = 0;
        self.log_follow = true;
        self.pod_detail_section = PodDetailSection::Overview;
    }

    pub fn current_pod(&self) -> Option<&PodInfo> {
        if let AppView::PodDetail { pod_name } = &self.view {
            self.snapshot
                .as_ref()?
                .pods
                .iter()
                .find(|p| &p.name == pod_name)
        } else {
            None
        }
    }

    pub fn current_node(&self) -> Option<&crate::data::models::NodeMetric> {
        if let AppView::NodeDetail { node_name } = &self.view {
            self.snapshot
                .as_ref()?
                .nodes
                .iter()
                .find(|n| &n.name == node_name)
        } else {
            None
        }
    }

    pub fn is_connecting_to_cluster(&self) -> bool {
        self.is_loading && self.snapshot.is_none() && self.connection_issue.is_none()
    }

    pub fn cycle_pod_sort_mode(&mut self) {
        self.pod_sort_mode = self.pod_sort_mode.next();
        self.pod_cursor = 0;
    }

    pub fn clear_incident_focus(&mut self) {
        self.incident_focus = None;
        self.pod_cursor = 0;
    }

    pub fn apply_incident_focus(&mut self, bucket: &IncidentBucket) {
        let pod_names = bucket
            .targets
            .iter()
            .filter_map(IncidentTarget::pod_name)
            .map(ToOwned::to_owned)
            .collect();
        let workload_names = bucket
            .targets
            .iter()
            .filter_map(IncidentTarget::workload_key)
            .map(|(_, name)| name.to_owned())
            .collect();
        self.incident_focus = Some(IncidentFocus {
            reason: bucket.reason.clone(),
            pod_names,
            workload_names,
        });
        self.pod_cursor = 0;
    }

    pub fn selected_incident(&self) -> Option<&IncidentBucket> {
        self.snapshot
            .as_ref()?
            .incident_buckets
            .get(self.event_cursor)
    }

    pub fn has_blocking_connection_issue(&self) -> bool {
        self.snapshot.is_none() && self.connection_issue.is_some()
    }

    pub fn blocking_connection_message(&self) -> Option<String> {
        let issue = self.connection_issue.as_ref()?;
        Some(match issue.kind {
            crate::data::models::ConnectionIssueKind::KubectlMissing => {
                format!("{} Press `r` to retry.", issue.detail)
            }
            crate::data::models::ConnectionIssueKind::NoContext => {
                format!("{} Press `r` to retry.", issue.detail)
            }
            crate::data::models::ConnectionIssueKind::NamespaceUnavailable => {
                if issue.namespace.is_none() && self.config.namespace.is_empty() {
                    issue.detail.clone()
                } else {
                    let namespace = issue
                        .namespace
                        .as_deref()
                        .filter(|namespace| !namespace.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            if self.config.namespace.is_empty() {
                                "current".to_string()
                            } else {
                                self.config.namespace.clone()
                            }
                        });
                    format!(
                        "Namespace `{}` is unavailable. Press `n` to choose a namespace, `N` to enter one manually, or `r` to retry.",
                        namespace
                    )
                }
            }
            crate::data::models::ConnectionIssueKind::Generic => {
                format!("{} Press `r` to retry.", issue.detail)
            }
        })
    }

    fn latest_pod_incidents(&self) -> HashMap<String, i64> {
        let mut incidents = HashMap::new();
        let Some(snapshot) = &self.snapshot else {
            return incidents;
        };

        for event in &snapshot.events {
            if event.kind != "Pod" {
                continue;
            }
            let Some(timestamp) = parse_timestamp(&event.timestamp) else {
                continue;
            };
            incidents
                .entry(event.name.clone())
                .and_modify(|current| *current = (*current).max(timestamp))
                .or_insert(timestamp);
        }

        for bucket in &snapshot.incident_buckets {
            let Some(timestamp) = parse_timestamp(&bucket.latest_timestamp) else {
                continue;
            };
            for target in &bucket.targets {
                if let Some(pod_name) = target.pod_name() {
                    incidents
                        .entry(pod_name.to_string())
                        .and_modify(|current| *current = (*current).max(timestamp))
                        .or_insert(timestamp);
                }
            }
        }

        incidents
    }

    fn compare_pods(
        &self,
        a: &PodInfo,
        b: &PodInfo,
        latest_incidents: Option<&HashMap<String, i64>>,
    ) -> std::cmp::Ordering {
        match self.pod_sort_mode {
            PodSortMode::Default => status_rank(&b.status)
                .cmp(&status_rank(&a.status))
                .then_with(|| b.memory_pct.cmp(&a.memory_pct))
                .then_with(|| a.name.cmp(&b.name)),
            PodSortMode::Restarts => b
                .restarts
                .cmp(&a.restarts)
                .then_with(|| status_rank(&b.status).cmp(&status_rank(&a.status)))
                .then_with(|| a.name.cmp(&b.name)),
            PodSortMode::Cpu => b
                .cpu_pct
                .cmp(&a.cpu_pct)
                .then_with(|| status_rank(&b.status).cmp(&status_rank(&a.status)))
                .then_with(|| a.name.cmp(&b.name)),
            PodSortMode::Memory => b
                .memory_pct
                .cmp(&a.memory_pct)
                .then_with(|| status_rank(&b.status).cmp(&status_rank(&a.status)))
                .then_with(|| a.name.cmp(&b.name)),
            PodSortMode::NotReady => {
                let a_rank = readiness_rank(a);
                let b_rank = readiness_rank(b);
                b_rank
                    .cmp(&a_rank)
                    .then_with(|| status_rank(&b.status).cmp(&status_rank(&a.status)))
                    .then_with(|| a.name.cmp(&b.name))
            }
            PodSortMode::LatestIncident => {
                let latest_incidents = latest_incidents.expect("latest incident map required");
                let a_ts = latest_incidents.get(&a.name).copied().unwrap_or(i64::MIN);
                let b_ts = latest_incidents.get(&b.name).copied().unwrap_or(i64::MIN);
                b_ts.cmp(&a_ts)
                    .then_with(|| status_rank(&b.status).cmp(&status_rank(&a.status)))
                    .then_with(|| a.name.cmp(&b.name))
            }
        }
    }
}

/// Case-insensitive substring search without allocating new strings
fn contains_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    let mut haystack_chars = haystack.chars();
    let mut needle_chars = needle.chars();

    let Some(first_needle) = needle_chars.next() else {
        return true;
    };
    let first_needle_lower = first_needle.to_lowercase().next().unwrap_or(first_needle);

    'outer: loop {
        // Find first matching character
        loop {
            match haystack_chars.next() {
                Some(c) if c.to_lowercase().next().unwrap_or(c) == first_needle_lower => break,
                Some(_) => continue,
                None => return false,
            }
        }

        // Check if rest of needle matches
        let mut haystack_clone = haystack_chars.clone();
        let mut needle_clone = needle_chars.clone();

        loop {
            match (haystack_clone.next(), needle_clone.next()) {
                (Some(h), Some(n)) => {
                    let h_lower = h.to_lowercase().next().unwrap_or(h);
                    let n_lower = n.to_lowercase().next().unwrap_or(n);
                    if h_lower != n_lower {
                        continue 'outer;
                    }
                }
                (None, Some(_)) => return false, // haystack ended before needle
                (Some(_), None) => return true,  // matched all of needle
                (None, None) => return true,     // matched all of needle
            }
        }
    }
}

fn pod_history_key(pod: &PodInfo) -> String {
    format!("{}\0{}\0{}", pod.namespace, pod.name, pod.uid)
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp())
}

fn readiness_rank(pod: &PodInfo) -> u8 {
    match (pod.is_ready, pod.crash_looping, pod.oom_killed) {
        (_, true, _) | (_, _, true) => 3,
        (false, _, _) => 2,
        (true, false, false) => 1,
    }
}

fn status_rank(status: &HealthStatus) -> u8 {
    match status {
        HealthStatus::Critical => 4,
        HealthStatus::Warning => 3,
        HealthStatus::Elevated => 2,
        HealthStatus::Healthy => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, PodSortMode};
    use crate::config::Config;
    use crate::data::models::{
        ClusterEvent, ClusterSnapshot, ConnectionIssue, ConnectionIssueKind, ContainerInfo,
        EventType, HealthScore, HealthStatus, IncidentBucket, IncidentSeverity, IncidentTarget,
        PodInfo,
    };
    use std::time::Instant;

    fn pod(name: &str, namespace: &str, cpu_pct: u8, memory_pct: u8) -> PodInfo {
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
            memory_request_pct: memory_pct,
            memory_pct,
            cpu_request_pct: cpu_pct,
            cpu_pct,
            status: HealthStatus::Healthy,
            ready_containers: 1,
            total_containers: 1,
            is_ready: true,
            crash_looping: false,
            oom_killed: false,
            node_name: None,
            containers: vec![ContainerInfo {
                name: "app".to_string(),
                ready: true,
                restart_count: 0,
                state: "Running".to_string(),
                last_termination_reason: None,
                last_exit_code: None,
            }],
        }
    }

    fn pod_with_state(
        name: &str,
        namespace: &str,
        status: HealthStatus,
        restarts: u32,
        cpu_pct: u8,
        memory_pct: u8,
        ready: bool,
        ready_containers: u32,
        total_containers: u32,
    ) -> PodInfo {
        PodInfo {
            uid: format!("{namespace}-{name}-uid"),
            name: name.to_string(),
            namespace: namespace.to_string(),
            phase: "Running".to_string(),
            restarts,
            age: "1m".to_string(),
            cpu_millicores: 0,
            cpu_request_millicores: 0,
            cpu_limit_millicores: 0,
            memory_mb: 0,
            memory_request_mb: 0,
            memory_limit_mb: 0,
            memory_request_pct: memory_pct,
            memory_pct,
            cpu_request_pct: cpu_pct,
            cpu_pct,
            status,
            ready_containers,
            total_containers,
            is_ready: ready,
            crash_looping: false,
            oom_killed: false,
            node_name: None,
            containers: vec![ContainerInfo {
                name: "app".to_string(),
                ready,
                restart_count: restarts,
                state: "Running".to_string(),
                last_termination_reason: None,
                last_exit_code: None,
            }],
        }
    }

    fn snapshot_with(pods: Vec<PodInfo>, events: Vec<ClusterEvent>) -> ClusterSnapshot {
        ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods,
            events,
            incident_buckets: vec![IncidentBucket {
                reason: "CrashLoopBackOff".to_string(),
                severity: IncidentSeverity::Critical,
                occurrences: 1,
                targets: vec![IncidentTarget::Pod {
                    pod_name: "api".to_string(),
                }],
                affected_resources: vec!["Pod/api".to_string()],
                latest_timestamp: "2026-03-09T10:00:00Z".to_string(),
                sample_message: Some("container started".to_string()),
            }],
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
    fn pod_history_prunes_pods_missing_from_latest_snapshot() {
        let mut app = AppState::new(Config::default());

        app.update_pod_history(&[pod("pod-a", "ns", 20, 40), pod("pod-b", "ns", 30, 50)]);
        app.update_pod_history(&[pod("pod-b", "ns", 35, 60)]);

        assert!(app
            .get_pod_memory_history(&pod("pod-a", "ns", 20, 40))
            .is_none());
        assert!(app
            .get_pod_cpu_history(&pod("pod-a", "ns", 20, 40))
            .is_none());
        assert_eq!(
            app.get_pod_memory_history(&pod("pod-b", "ns", 35, 60)),
            Some(&vec![50, 60])
        );
        assert_eq!(
            app.get_pod_cpu_history(&pod("pod-b", "ns", 35, 60)),
            Some(&vec![30, 35])
        );
    }

    #[test]
    fn pod_history_resets_when_uid_changes() {
        let mut app = AppState::new(Config::default());
        let mut original = pod("pod-a", "ns", 20, 40);
        original.uid = "uid-a".to_string();
        let mut replacement = pod("pod-a", "ns", 30, 50);
        replacement.uid = "uid-b".to_string();

        app.update_pod_history(&[original]);
        app.update_pod_history(&[replacement.clone()]);

        assert_eq!(app.get_pod_memory_history(&replacement), Some(&vec![50]));
        assert_eq!(app.get_pod_cpu_history(&replacement), Some(&vec![30]));
    }

    #[test]
    fn default_sort_preserves_snapshot_order() {
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(snapshot_with(
            vec![
                pod_with_state("worker", "ns", HealthStatus::Warning, 1, 90, 10, true, 1, 1),
                pod_with_state("api", "ns", HealthStatus::Critical, 5, 20, 50, false, 0, 1),
                pod_with_state(
                    "batch",
                    "ns",
                    HealthStatus::Elevated,
                    3,
                    20,
                    80,
                    false,
                    0,
                    1,
                ),
            ],
            vec![],
        ));

        let names: Vec<&str> = app
            .filtered_pods()
            .iter()
            .map(|pod| pod.name.as_str())
            .collect();

        assert_eq!(names, vec!["worker", "api", "batch"]);
    }

    #[test]
    fn latest_incident_sort_uses_pod_events_only() {
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(snapshot_with(
            vec![
                pod_with_state("api", "ns", HealthStatus::Critical, 1, 20, 50, false, 0, 1),
                pod_with_state(
                    "batch",
                    "ns",
                    HealthStatus::Elevated,
                    3,
                    20,
                    80,
                    false,
                    0,
                    1,
                ),
                pod_with_state("worker", "ns", HealthStatus::Warning, 1, 90, 10, true, 1, 1),
            ],
            vec![
                ClusterEvent {
                    kind: "Pod".to_string(),
                    name: "api".to_string(),
                    reason: "CrashLoopBackOff".to_string(),
                    message: "restarting".to_string(),
                    event_type: EventType::Warning,
                    count: 1,
                    timestamp: "2026-03-09T10:00:00Z".to_string(),
                },
                ClusterEvent {
                    kind: "Pod".to_string(),
                    name: "worker".to_string(),
                    reason: "FailedScheduling".to_string(),
                    message: "pending".to_string(),
                    event_type: EventType::Warning,
                    count: 1,
                    timestamp: "2026-03-09T11:00:00Z".to_string(),
                },
                ClusterEvent {
                    kind: "Deployment".to_string(),
                    name: "worker".to_string(),
                    reason: "Progressing".to_string(),
                    message: "rollout".to_string(),
                    event_type: EventType::Normal,
                    count: 1,
                    timestamp: "2026-03-09T12:00:00Z".to_string(),
                },
                ClusterEvent {
                    kind: "Pod".to_string(),
                    name: "batch".to_string(),
                    reason: "OOMKilled".to_string(),
                    message: "oom".to_string(),
                    event_type: EventType::Warning,
                    count: 1,
                    timestamp: "2026-03-09T12:30:00Z".to_string(),
                },
            ],
        ));
        app.pod_sort_mode = PodSortMode::LatestIncident;

        let names: Vec<&str> = app
            .filtered_pods()
            .iter()
            .map(|pod| pod.name.as_str())
            .collect();

        assert_eq!(names, vec!["batch", "worker", "api"]);
    }

    #[test]
    fn pod_filter_is_case_insensitive() {
        let mut app = AppState::new(Config::default());
        app.snapshot = Some(ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![
                pod("api-server", "default", 20, 40),
                pod("worker", "default", 10, 20),
            ],
            events: vec![],
            incident_buckets: vec![],
            health: crate::data::models::HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: std::time::Instant::now(),
            error: None,
            context_name: None,
        });
        app.pod_filter = "API".to_string();

        let filtered = app.filtered_pods();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "api-server");
    }

    #[test]
    fn cycle_pod_sort_mode_resets_cursor() {
        let mut app = AppState::new(Config::default());
        app.pod_cursor = 7;

        let expected = [
            PodSortMode::Restarts,
            PodSortMode::Cpu,
            PodSortMode::Memory,
            PodSortMode::NotReady,
            PodSortMode::LatestIncident,
            PodSortMode::Default,
        ];

        for mode in expected {
            app.cycle_pod_sort_mode();
            assert_eq!(app.pod_sort_mode, mode);
            assert_eq!(app.pod_cursor, 0);
        }
    }

    #[test]
    fn latest_incident_sort_prefers_newest_pod_events() {
        let mut app = AppState::new(Config::default());
        app.pod_sort_mode = PodSortMode::LatestIncident;
        app.snapshot = Some(ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![
                pod("api", "default", 10, 10),
                pod("worker", "default", 10, 10),
                pod("batch", "default", 10, 10),
            ],
            events: vec![
                ClusterEvent {
                    kind: "Pod".to_string(),
                    name: "api".to_string(),
                    reason: "Started".to_string(),
                    message: "ok".to_string(),
                    event_type: EventType::Normal,
                    count: 1,
                    timestamp: "2026-03-09T10:00:00Z".to_string(),
                },
                ClusterEvent {
                    kind: "Pod".to_string(),
                    name: "worker".to_string(),
                    reason: "FailedScheduling".to_string(),
                    message: "pending".to_string(),
                    event_type: EventType::Warning,
                    count: 1,
                    timestamp: "2026-03-09T10:05:00Z".to_string(),
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
            context_name: None,
        });

        let filtered = app.filtered_pods();

        assert_eq!(
            filtered
                .iter()
                .map(|pod| pod.name.as_str())
                .collect::<Vec<_>>(),
            vec!["worker", "api", "batch"]
        );
    }

    #[test]
    fn apply_incident_focus_limits_visible_pods_to_matching_targets() {
        let mut app = AppState::new(Config::default());
        let bucket = IncidentBucket {
            reason: "FailedScheduling".to_string(),
            severity: IncidentSeverity::Warning,
            occurrences: 1,
            targets: vec![
                IncidentTarget::Pod {
                    pod_name: "api".to_string(),
                },
                IncidentTarget::Node {
                    node_name: "node-a".to_string(),
                },
            ],
            affected_resources: vec!["Node/node-a".to_string(), "Pod/api".to_string()],
            latest_timestamp: "2026-03-09T10:05:00Z".to_string(),
            sample_message: Some("0/3 nodes are available".to_string()),
        };
        app.snapshot = Some(ClusterSnapshot {
            nodes: vec![],
            workloads: vec![],
            pods: vec![
                pod("api", "default", 10, 10),
                pod("worker", "default", 10, 10),
            ],
            events: vec![],
            incident_buckets: vec![bucket.clone()],
            health: HealthScore {
                score: 100,
                grade: 'A',
                critical_nodes: 0,
                critical_pods: 0,
                total_restarts: 0,
            },
            fetched_at: Instant::now(),
            error: None,
            context_name: None,
        });

        app.apply_incident_focus(&bucket);

        let filtered = app.filtered_pods();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "api");
        assert_eq!(app.pod_cursor, 0);
    }

    #[test]
    fn blocking_connection_message_reflects_connection_issue_kind() {
        let mut app = AppState::new(Config::default());
        app.connection_issue = Some(ConnectionIssue {
            kind: ConnectionIssueKind::KubectlMissing,
            namespace: None,
            detail: "kubectl not found".to_string(),
        });

        assert!(app.has_blocking_connection_issue());
        let message = app.blocking_connection_message().unwrap();
        assert!(message.contains("kubectl"));
        assert!(message.contains("Press `r`"));
    }
}
