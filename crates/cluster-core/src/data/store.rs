//! Last-known cluster state that snapshots are derived from. A full poll
//! replaces it; watch updates patch it between polls. Objects are kept as raw
//! JSON and parsed at snapshot time, so a snapshot built after a poll and one
//! built after a watch update go through exactly the same code.

use std::collections::BTreeMap;
use std::time::Instant;

use serde_json::Value;

use crate::data::checks::problem_workload;
use crate::data::collector::{self, resource_version, ResourceList, WorkloadCollection};
use crate::data::health;
use crate::data::incidents;
use crate::data::models::{ClusterSignals, ClusterSnapshot, DataCoverage, ResourceProblem};
use crate::data::watch::{WatchAction, WatchKind, WatchUpdate};

/// Objects of one kind keyed by `namespace/name`, so iteration yields the
/// same order kubectl lists them in.
#[derive(Debug, Clone, Default)]
pub struct ResourceSet {
    items: BTreeMap<String, Value>,
    /// Whether the contents come from a successful list. Watch updates are
    /// ignored otherwise: patching an empty or cached set would present a
    /// handful of changed objects as the whole picture.
    listed: bool,
    /// Resource version of that list. Watch updates at or below it were
    /// already reflected by the list; they are typically a backlog queued
    /// while the poll ran and would roll objects back.
    listed_version: Option<u64>,
}

impl ResourceSet {
    pub fn replace(&mut self, list: ResourceList) {
        self.items = list
            .items
            .into_iter()
            .map(|item| (object_key(&item), item))
            .collect();
        self.listed = true;
        self.listed_version = list.resource_version;
    }

    /// Last-known objects kept for display after a failed list; not patched
    /// by watches until the next successful list.
    pub fn unlisted(items: impl IntoIterator<Item = Value>) -> Self {
        Self {
            items: items
                .into_iter()
                .map(|item| (object_key(&item), item))
                .collect(),
            listed: false,
            listed_version: None,
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &Value> {
        self.items.values()
    }

    pub fn apply(&mut self, action: WatchAction, object: Value) {
        if !self.listed {
            return;
        }
        let version = resource_version(&object);
        if is_not_newer(version, self.listed_version) {
            return;
        }
        let key = object_key(&object);
        match action {
            WatchAction::Upsert => {
                let current = self.items.get(&key).and_then(resource_version);
                if !is_not_newer(version, current) {
                    self.items.insert(key, object);
                }
            }
            WatchAction::Delete => {
                // A same-named replacement (e.g. a StatefulSet pod) may have
                // been added already; only drop the object that was deleted.
                if self.items.get(&key).map(uid) == Some(uid(&object)) {
                    self.items.remove(&key);
                }
            }
        }
    }
}

/// True when both versions are known and `version` is not past `than`.
fn is_not_newer(version: Option<u64>, than: Option<u64>) -> bool {
    matches!((version, than), (Some(version), Some(than)) if version <= than)
}

fn object_key(object: &Value) -> String {
    let field = |pointer| {
        object
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    format!(
        "{}/{}",
        field("/metadata/namespace"),
        field("/metadata/name")
    )
}

fn uid(object: &Value) -> &str {
    object
        .pointer("/metadata/uid")
        .and_then(Value::as_str)
        .unwrap_or("")
}

#[derive(Debug, Clone, Default)]
pub struct ClusterStore {
    pub namespace: String,
    pub nodes: ResourceSet,
    pub pods: ResourceSet,
    pub events: ResourceSet,
    /// Raw `kubectl top` output from the last poll.
    pub node_top: String,
    pub pod_top: String,
    /// Workloads change less often and span many kinds, so they are only
    /// refreshed by polls.
    pub workloads: WorkloadCollection,
    pub problems: Vec<ResourceProblem>,
    /// Errors from the last poll, carried into watch-driven snapshots.
    pub errors: Vec<String>,
    pub context_name: Option<String>,
    pub nodes_visible: bool,
    /// When the last poll finished. Watch snapshots keep it, since usage,
    /// workloads and checks are only as fresh as the poll.
    pub polled_at: Option<Instant>,
}

impl ClusterStore {
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            nodes_visible: true,
            ..Self::default()
        }
    }

    pub fn has_data(&self) -> bool {
        !self.nodes.is_empty()
            || !self.pods.is_empty()
            || !self.events.is_empty()
            || !self.workloads.summaries.is_empty()
    }

    pub fn apply(&mut self, update: WatchUpdate) {
        let set = match update.kind {
            WatchKind::Nodes => &mut self.nodes,
            WatchKind::Pods => &mut self.pods,
            WatchKind::Events => &mut self.events,
        };
        set.apply(update.action, update.object);
    }

    /// `metrics_sampled` marks a snapshot built right after a poll, i.e. one
    /// with a fresh `kubectl top` sample.
    pub fn snapshot(
        &self,
        node_pool_filter: Option<&str>,
        metrics_sampled: bool,
    ) -> ClusterSnapshot {
        let nodes =
            collector::build_node_metrics(&self.node_top, self.nodes.values(), node_pool_filter);
        let pods = collector::build_pods(self.pods.values(), &self.pod_top, &self.namespace);
        let events = collector::build_events(self.events.values());

        let mut workloads = self.workloads.summaries.clone();
        workloads.extend(self.problems.iter().map(problem_workload));
        collector::sort_workloads(&mut workloads);
        collector::attach_workload_events(&mut workloads, &events);

        // Without metrics-server every usage figure is zero; a cluster with any
        // workload never reads exactly zero across the board.
        let metrics_available = (nodes.is_empty() && pods.is_empty())
            || nodes
                .iter()
                .any(|n| n.cpu_millicores > 0 || n.memory_mb > 0)
            || pods.iter().any(|p| p.cpu_millicores > 0 || p.memory_mb > 0);
        let signals = ClusterSignals::new(&nodes, &pods, &events).with_problems(&self.problems);
        let health = health::calculate_health(&signals);
        let incident_buckets = incidents::build_incident_buckets(&signals);

        let mut errors = self.errors.clone();
        errors.extend(
            self.workloads
                .warnings
                .iter()
                .map(|warning| format!("Workloads/{warning}")),
        );

        ClusterSnapshot {
            nodes,
            workloads,
            pods,
            events,
            incident_buckets,
            health,
            fetched_at: self.polled_at.unwrap_or_else(Instant::now),
            error: (!errors.is_empty()).then(|| errors.join(" | ")),
            context_name: self.context_name.clone(),
            coverage: DataCoverage {
                metrics_available,
                nodes_visible: self.nodes_visible,
            },
            resource_problems: self.problems.clone(),
            metrics_sampled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::models::IncidentSeverity;
    use serde_json::json;

    fn pod(name: &str, uid: &str, waiting_reason: Option<&str>) -> Value {
        let state = match waiting_reason {
            Some(reason) => json!({"waiting": {"reason": reason}}),
            None => json!({"running": {}}),
        };
        json!({
            "metadata": {"name": name, "namespace": "default", "uid": uid},
            "status": {"phase": "Running", "containerStatuses": [{
                "name": "app", "ready": waiting_reason.is_none(), "restartCount": 0,
                "state": state
            }]}
        })
    }

    fn list(items: Vec<Value>, resource_version: Option<u64>) -> ResourceList {
        ResourceList {
            items,
            resource_version,
        }
    }

    fn versioned(mut object: Value, version: u64) -> Value {
        object["metadata"]["resourceVersion"] = json!(version.to_string());
        object
    }

    #[test]
    fn backlog_older_than_the_list_is_discarded() {
        let mut set = ResourceSet::default();
        set.replace(list(vec![versioned(pod("a", "a1", None), 90)], Some(100)));

        // Queued while the poll ran: a pre-list MODIFIED and a pod deleted
        // before the list must not roll back or resurrect anything.
        set.apply(
            WatchAction::Upsert,
            versioned(pod("a", "a1", Some("ContainerCreating")), 80),
        );
        set.apply(WatchAction::Upsert, versioned(pod("gone", "g1", None), 95));
        assert_eq!(set.len(), 1);
        assert!(
            set.values().next().unwrap()["status"]["containerStatuses"][0]["state"]
                .get("running")
                .is_some()
        );

        set.apply(
            WatchAction::Upsert,
            versioned(pod("a", "a1", Some("CrashLoopBackOff")), 120),
        );
        // An out-of-order older update after a newer one is ignored too.
        set.apply(WatchAction::Upsert, versioned(pod("a", "a1", None), 110));
        let state = &set.values().next().unwrap()["status"]["containerStatuses"][0]["state"];
        assert_eq!(state["waiting"]["reason"], "CrashLoopBackOff");
    }

    #[test]
    fn unlisted_sets_ignore_watch_updates() {
        let mut set = ResourceSet::default();
        set.apply(WatchAction::Upsert, pod("a", "a1", None));
        assert!(set.is_empty());

        let mut cached = ResourceSet::unlisted(vec![pod("old", "o1", None)]);
        cached.apply(WatchAction::Upsert, pod("new", "n1", None));
        assert_eq!(cached.len(), 1);
    }

    fn update(kind: WatchKind, action: WatchAction, object: Value) -> WatchUpdate {
        WatchUpdate {
            generation: 0,
            kind,
            action,
            object,
        }
    }

    #[test]
    fn watch_updates_patch_the_polled_state() {
        let mut store = ClusterStore::new("default");
        store
            .pods
            .replace(list(vec![pod("b", "b1", None), pod("a", "a1", None)], None));

        store.apply(update(
            WatchKind::Pods,
            WatchAction::Upsert,
            pod("a", "a1", Some("CrashLoopBackOff")),
        ));
        store.apply(update(
            WatchKind::Pods,
            WatchAction::Upsert,
            pod("c", "c1", None),
        ));
        store.apply(update(
            WatchKind::Pods,
            WatchAction::Delete,
            pod("b", "b1", None),
        ));

        let snapshot = store.snapshot(None, false);
        let names: Vec<&str> = snapshot.pods.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c"]);
        assert!(snapshot.pods[0].crash_looping);
        assert_eq!(snapshot.incident_buckets[0].reason, "CrashLoopBackOff");
        assert!(!snapshot.metrics_sampled);
    }

    #[test]
    fn delete_of_a_replaced_object_keeps_the_replacement() {
        let mut set = ResourceSet::default();
        set.replace(list(vec![pod("web-0", "new", None)], None));
        set.apply(WatchAction::Delete, pod("web-0", "old", None));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn problems_feed_workloads_incidents_and_health() {
        let mut store = ClusterStore::new("default");
        store.problems.push(ResourceProblem {
            kind: "APIService".to_string(),
            name: "v1beta1.metrics.k8s.io".to_string(),
            namespace: None,
            reason: "APIServiceUnavailable".to_string(),
            severity: IncidentSeverity::Critical,
            message: String::new(),
        });

        let snapshot = store.snapshot(None, true);

        assert_eq!(snapshot.workloads.len(), 1);
        assert_eq!(snapshot.incident_buckets[0].reason, "APIServiceUnavailable");
        assert!(snapshot.health.score < 100);
        assert!(snapshot.metrics_sampled);
    }
}
