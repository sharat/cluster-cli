//! Health checks beyond pods and nodes: cluster-scoped objects whose failure
//! breaks the whole cluster, and user-configured custom resources judged by the
//! common `.status.conditions` / Argo `.status.health` conventions. Only
//! failing objects are reported.

use serde_json::Value;

use crate::data::collector::list_items;
use crate::data::models::{
    HealthStatus, IncidentSeverity, ResourceProblem, WorkloadKind, WorkloadSummary,
};

/// A namespace still terminating after this long is stuck, not just slow.
const STUCK_TERMINATING_SECS: i64 = 10 * 60;

/// Operators report Ready=False while an object is being set up (a
/// Certificate being issued, a NodeClaim launching); only a condition that
/// has stayed False this long counts as a problem.
const CONDITION_GRACE_SECS: i64 = 5 * 60;

/// Condition types that mean "this object is working" across popular
/// operators (cert-manager, Flux, Crossplane, Karpenter, ...), and the
/// incident reason suffix used when one is `False`.
const READINESS_CONDITIONS: &[(&str, &str)] = &[
    ("Ready", "NotReady"),
    ("Available", "Unavailable"),
    ("Healthy", "Unhealthy"),
    ("Synced", "NotSynced"),
];

#[derive(Debug, Default)]
pub struct CheckResults {
    pub problems: Vec<ResourceProblem>,
    /// Checks that could not run, e.g. a misspelled resource in `crd_checks`.
    pub warnings: Vec<String>,
}

/// Built-in checks of cluster-scoped objects. They are independent of the
/// selected namespace and list every APIService, PV and Namespace, so the
/// fetcher runs them on a slower cadence than the main poll.
pub async fn run_cluster_checks() -> Vec<ResourceProblem> {
    let (apiservices, volumes, namespaces) = tokio::join!(
        list(&["get", "apiservices", "-o", "json"]),
        list(&["get", "persistentvolumes", "-o", "json"]),
        list(&["get", "namespaces", "-o", "json"]),
    );

    let now = chrono::Utc::now();
    let mut problems = Vec::new();
    // Cluster-scoped reads are often forbidden to namespace-scoped users;
    // these checks are a bonus, so a failure just skips them.
    if let Ok(items) = apiservices {
        problems.extend(items.iter().filter_map(check_apiservice));
    }
    if let Ok(items) = volumes {
        problems.extend(items.iter().filter_map(check_persistent_volume));
    }
    if let Ok(items) = namespaces {
        problems.extend(items.iter().filter_map(|item| check_namespace(item, now)));
    }
    problems
}

/// One check per configured custom resource (`plural.group`, e.g.
/// `certificates.cert-manager.io`) in `namespace`.
pub async fn run_custom_resource_checks(
    namespace: &str,
    custom_resources: &[String],
) -> CheckResults {
    let custom = futures::future::join_all(custom_resources.iter().map(|resource| async move {
        let result = list(&["get", resource, "-n", namespace, "-o", "json"]).await;
        (resource, result)
    }))
    .await;

    let now = chrono::Utc::now();
    let mut results = CheckResults::default();
    for (resource, result) in custom {
        match result {
            Ok(items) => results.problems.extend(
                items
                    .iter()
                    .filter_map(|item| check_custom_resource(item, resource, now)),
            ),
            Err(err) => results.warnings.push(format!("Checks/{resource}: {err}")),
        }
    }
    results
}

async fn list(args: &[&str]) -> Result<Vec<Value>, String> {
    list_items(args)
        .await
        .map(|list| list.items)
        .map_err(|err| err.to_string())
}

/// An aggregated API that is down breaks discovery for everything, and for
/// `v1beta1.metrics.k8s.io` specifically `kubectl top` and every HPA.
fn check_apiservice(item: &Value) -> Option<ResourceProblem> {
    let condition = find_condition(item, "Available")?;
    if condition_status(condition) != Some("False") {
        return None;
    }
    Some(problem(
        item,
        "APIService",
        "APIServiceUnavailable",
        IncidentSeverity::Critical,
        condition_message(condition),
    ))
}

fn check_persistent_volume(item: &Value) -> Option<ResourceProblem> {
    if item.pointer("/status/phase").and_then(Value::as_str) != Some("Failed") {
        return None;
    }
    let message = item
        .pointer("/status/message")
        .and_then(Value::as_str)
        .unwrap_or("volume reclamation failed")
        .to_string();
    Some(problem(
        item,
        "PersistentVolume",
        "PersistentVolumeFailed",
        IncidentSeverity::Warning,
        message,
    ))
}

fn check_namespace(item: &Value, now: chrono::DateTime<chrono::Utc>) -> Option<ResourceProblem> {
    let deleted_at = item
        .pointer("/metadata/deletionTimestamp")
        .and_then(Value::as_str)?;
    let deleted_at = chrono::DateTime::parse_from_rfc3339(deleted_at).ok()?;
    if now.signed_duration_since(deleted_at).num_seconds() < STUCK_TERMINATING_SECS {
        return None;
    }
    // The namespace controller explains what blocks deletion in conditions
    // like NamespaceFinalizersRemaining / NamespaceContentRemaining.
    let blockers: Vec<String> = conditions(item)
        .filter(|condition| condition_status(condition) == Some("True"))
        .map(condition_message)
        .filter(|message| !message.is_empty())
        .collect();
    let message = if blockers.is_empty() {
        "namespace has been terminating for over 10 minutes".to_string()
    } else {
        blockers.join("; ")
    };
    Some(problem(
        item,
        "Namespace",
        "NamespaceStuckTerminating",
        IncidentSeverity::Warning,
        message,
    ))
}

fn check_custom_resource(
    item: &Value,
    resource: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<ResourceProblem> {
    let kind = item
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or(resource)
        .to_string();

    for (condition_type, suffix) in READINESS_CONDITIONS {
        if let Some(condition) = find_condition(item, condition_type) {
            if condition_status(condition) == Some("False")
                && !changed_within(condition, now, CONDITION_GRACE_SECS)
            {
                return Some(problem(
                    item,
                    &kind,
                    &format!("{kind}{suffix}"),
                    IncidentSeverity::Warning,
                    condition_message(condition),
                ));
            }
        }
    }

    // Argo CD Applications report health outside of conditions.
    let health = item
        .pointer("/status/health/status")
        .and_then(Value::as_str);
    if let Some(health @ ("Degraded" | "Missing")) = health {
        let message = item
            .pointer("/status/health/message")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        return Some(problem(
            item,
            &kind,
            &format!("{kind}{health}"),
            IncidentSeverity::Warning,
            message,
        ));
    }

    None
}

fn problem(
    item: &Value,
    kind: &str,
    reason: &str,
    severity: IncidentSeverity,
    message: String,
) -> ResourceProblem {
    ResourceProblem {
        kind: kind.to_string(),
        name: item
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        namespace: item
            .pointer("/metadata/namespace")
            .and_then(Value::as_str)
            .map(str::to_string),
        reason: reason.to_string(),
        severity,
        message,
    }
}

fn conditions(item: &Value) -> impl Iterator<Item = &Value> {
    item.pointer("/status/conditions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn find_condition<'a>(item: &'a Value, condition_type: &str) -> Option<&'a Value> {
    conditions(item)
        .find(|condition| condition.get("type").and_then(Value::as_str) == Some(condition_type))
}

/// Whether the condition's `lastTransitionTime` is under `secs` old. A
/// missing or unparsable time counts as old, so the check still applies.
fn changed_within(condition: &Value, now: chrono::DateTime<chrono::Utc>, secs: i64) -> bool {
    condition
        .get("lastTransitionTime")
        .and_then(Value::as_str)
        .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())
        .is_some_and(|time| now.signed_duration_since(time).num_seconds() < secs)
}

fn condition_status(condition: &Value) -> Option<&str> {
    condition.get("status").and_then(Value::as_str)
}

/// `reason: message`, or whichever of the two is present.
fn condition_message(condition: &Value) -> String {
    let field = |name| {
        condition
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    match (field("reason"), field("message")) {
        (Some(reason), Some(message)) => format!("{reason}: {message}"),
        (Some(value), None) | (None, Some(value)) => value.to_string(),
        (None, None) => String::new(),
    }
}

/// Lists a failing resource in the workload popup so an incident drill-down
/// on it lands on its details and related events.
pub fn problem_workload(problem: &ResourceProblem) -> WorkloadSummary {
    let mut details = vec![
        ("Kind".to_string(), problem.kind.clone()),
        ("Reason".to_string(), problem.reason.clone()),
    ];
    if let Some(namespace) = &problem.namespace {
        details.push(("Namespace".to_string(), namespace.clone()));
    }
    if !problem.message.is_empty() {
        details.push(("Message".to_string(), problem.message.clone()));
    }
    WorkloadSummary {
        kind: WorkloadKind::Resource,
        name: problem.name.clone(),
        namespace: problem.namespace.clone().unwrap_or_default(),
        summary: format!("{} {}", problem.kind, problem.reason),
        details,
        status: match problem.severity {
            IncidentSeverity::Critical => HealthStatus::Critical,
            IncidentSeverity::Warning => HealthStatus::Warning,
            IncidentSeverity::Elevated => HealthStatus::Elevated,
        },
        recent_events: Vec::new(),
        related_event_targets: vec![(problem.kind.clone(), problem.name.clone())],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-09-26T12:00:00Z".parse().unwrap()
    }

    #[test]
    fn unavailable_apiservice_is_critical() {
        let item = json!({
            "metadata": {"name": "v1beta1.metrics.k8s.io"},
            "status": {"conditions": [{
                "type": "Available", "status": "False",
                "reason": "FailedDiscoveryCheck", "message": "no response"
            }]}
        });
        let problem = check_apiservice(&item).expect("problem");
        assert_eq!(problem.reason, "APIServiceUnavailable");
        assert_eq!(problem.severity, IncidentSeverity::Critical);
        assert_eq!(problem.message, "FailedDiscoveryCheck: no response");
        assert_eq!(problem.namespace, None);

        let healthy = json!({"status": {"conditions": [{"type": "Available", "status": "True"}]}});
        assert!(check_apiservice(&healthy).is_none());
    }

    #[test]
    fn only_failed_persistent_volumes_are_reported() {
        let failed = json!({"metadata": {"name": "pv-1"}, "status": {"phase": "Failed"}});
        let released = json!({"metadata": {"name": "pv-2"}, "status": {"phase": "Released"}});
        assert_eq!(
            check_persistent_volume(&failed).unwrap().reason,
            "PersistentVolumeFailed"
        );
        assert!(check_persistent_volume(&released).is_none());
    }

    #[test]
    fn namespace_is_stuck_only_after_the_grace_period() {
        let stuck = json!({
            "metadata": {"name": "old", "deletionTimestamp": "2026-09-26T11:00:00Z"},
            "status": {"phase": "Terminating", "conditions": [
                {"type": "NamespaceFinalizersRemaining", "status": "True",
                 "reason": "SomeFinalizersRemain", "message": "example.com/finalizer in 1 resource"},
                {"type": "NamespaceDeletionDiscoveryFailure", "status": "False"}
            ]}
        });
        let recent = json!({
            "metadata": {"name": "new", "deletionTimestamp": "2026-09-26T11:58:00Z"}
        });
        let problem = check_namespace(&stuck, now()).expect("problem");
        assert_eq!(problem.reason, "NamespaceStuckTerminating");
        assert_eq!(
            problem.message,
            "SomeFinalizersRemain: example.com/finalizer in 1 resource"
        );
        assert!(check_namespace(&recent, now()).is_none());
        assert!(check_namespace(&json!({"metadata": {"name": "live"}}), now()).is_none());
    }

    #[test]
    fn custom_resource_conditions_and_argo_health_are_checked() {
        let certificate = json!({
            "kind": "Certificate",
            "metadata": {"name": "web-tls", "namespace": "prod"},
            "status": {"conditions": [{"type": "Ready", "status": "False", "reason": "Expired"}]}
        });
        let problem =
            check_custom_resource(&certificate, "certificates.cert-manager.io", now()).unwrap();
        assert_eq!(problem.reason, "CertificateNotReady");
        assert_eq!(problem.namespace.as_deref(), Some("prod"));
        assert_eq!(problem.message, "Expired");

        let application = json!({
            "kind": "Application",
            "metadata": {"name": "api"},
            "status": {"health": {"status": "Degraded", "message": "Deployment has 0 ready"}}
        });
        let problem =
            check_custom_resource(&application, "applications.argoproj.io", now()).unwrap();
        assert_eq!(problem.reason, "ApplicationDegraded");

        let progressing = json!({
            "kind": "Certificate",
            "status": {"conditions": [{"type": "Ready", "status": "Unknown"}]}
        });
        assert!(
            check_custom_resource(&progressing, "certificates.cert-manager.io", now()).is_none()
        );
    }

    #[test]
    fn recently_false_conditions_are_given_time_to_settle() {
        let issuing = json!({
            "kind": "Certificate",
            "metadata": {"name": "web-tls"},
            "status": {"conditions": [{
                "type": "Ready", "status": "False", "reason": "Issuing",
                "lastTransitionTime": "2026-09-26T11:58:00Z"
            }]}
        });
        let stuck = json!({
            "kind": "Certificate",
            "metadata": {"name": "web-tls"},
            "status": {"conditions": [{
                "type": "Ready", "status": "False", "reason": "Issuing",
                "lastTransitionTime": "2026-09-26T11:00:00Z"
            }]}
        });
        let resource = "certificates.cert-manager.io";
        assert!(check_custom_resource(&issuing, resource, now()).is_none());
        assert!(check_custom_resource(&stuck, resource, now()).is_some());
    }

    #[test]
    fn problem_workload_is_resolvable_by_its_real_kind() {
        let workload = problem_workload(&ResourceProblem {
            kind: "Certificate".to_string(),
            name: "web-tls".to_string(),
            namespace: Some("prod".to_string()),
            reason: "CertificateNotReady".to_string(),
            severity: IncidentSeverity::Warning,
            message: "Expired".to_string(),
        });
        assert_eq!(workload.kind, WorkloadKind::Resource);
        assert_eq!(
            workload.related_event_targets,
            vec![("Certificate".to_string(), "web-tls".to_string())]
        );
        assert_eq!(workload.status, HealthStatus::Warning);
    }
}
