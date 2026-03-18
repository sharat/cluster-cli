#[cfg(test)]
use crate::data::models::*;

#[test]
fn test_health_status_from_pct() {
    // Updated thresholds aligned with grade boundaries: A>=90, B>=75, C>=60, D>=45, F<45
    assert_eq!(HealthStatus::from_pct(95), HealthStatus::Critical);
    assert_eq!(HealthStatus::from_pct(90), HealthStatus::Critical);
    assert_eq!(HealthStatus::from_pct(89), HealthStatus::Warning);
    assert_eq!(HealthStatus::from_pct(75), HealthStatus::Warning);
    assert_eq!(HealthStatus::from_pct(74), HealthStatus::Elevated);
    assert_eq!(HealthStatus::from_pct(60), HealthStatus::Elevated);
    assert_eq!(HealthStatus::from_pct(59), HealthStatus::Healthy);
    assert_eq!(HealthStatus::from_pct(0), HealthStatus::Healthy);
}

#[test]
fn test_condition_status_as_str() {
    assert_eq!(ConditionStatus::True.as_str(), "True");
    assert_eq!(ConditionStatus::False.as_str(), "False");
    assert_eq!(ConditionStatus::Unknown.as_str(), "Unknown");
}

#[test]
fn test_workload_kind_as_str() {
    assert_eq!(WorkloadKind::Deployment.as_str(), "Deployment");
    assert_eq!(WorkloadKind::StatefulSet.as_str(), "StatefulSet");
    assert_eq!(WorkloadKind::DaemonSet.as_str(), "DaemonSet");
}

#[test]
fn test_workload_kind_short_label() {
    assert_eq!(WorkloadKind::Deployment.short_label(), "Deploy");
    assert_eq!(WorkloadKind::StatefulSet.short_label(), "Sts");
    assert_eq!(WorkloadKind::DaemonSet.short_label(), "Ds");
}

#[test]
fn test_incident_severity_rank() {
    assert_eq!(IncidentSeverity::Critical.rank(), 3);
    assert_eq!(IncidentSeverity::Warning.rank(), 2);
    assert_eq!(IncidentSeverity::Elevated.rank(), 1);
}

#[test]
fn test_incident_severity_label() {
    assert_eq!(IncidentSeverity::Critical.label(), "critical");
    assert_eq!(IncidentSeverity::Warning.label(), "warning");
    assert_eq!(IncidentSeverity::Elevated.label(), "elevated");
}

#[test]
fn test_incident_severity_ordering() {
    // Note: Derived Ord orders by enum variant position (first < second < third)
    // So Critical < Warning < Elevated in derived ordering
    // But rank() gives Critical=3, Warning=2, Elevated=1 (higher = more severe)
    assert!(IncidentSeverity::Critical < IncidentSeverity::Warning);
    assert!(IncidentSeverity::Warning < IncidentSeverity::Elevated);
    assert!(IncidentSeverity::Critical < IncidentSeverity::Elevated);
}

#[test]
fn test_pod_history_new() {
    let history = PodHistory::new("test-pod".to_string(), "default".to_string());
    assert_eq!(history.name, "test-pod");
    assert_eq!(history.namespace, "default");
    assert!(history.memory_samples.is_empty());
}

#[test]
fn test_pod_history_add_sample() {
    let mut history = PodHistory::new("test-pod".to_string(), "default".to_string());
    
    history.add_sample(50, 5);
    assert_eq!(history.memory_samples.len(), 1);
    assert_eq!(history.memory_samples.front(), Some(&50));
    
    history.add_sample(75, 5);
    assert_eq!(history.memory_samples.len(), 2);
    
    // Test max samples limit
    for i in 0..10 {
        history.add_sample(i as u8, 5);
    }
    assert_eq!(history.memory_samples.len(), 5);
    // With VecDeque, the last element is at the back
    assert_eq!(history.memory_samples.back(), Some(&9));
}

#[test]
fn test_resource_usage_summary_default() {
    let summary = ResourceUsageSummary::default();
    assert_eq!(summary.pod_count, 0);
    assert_eq!(summary.cpu_usage_millicores, 0);
    assert_eq!(summary.memory_usage_mb, 0);
}

#[test]
fn test_event_type_equality() {
    assert_eq!(EventType::Warning, EventType::Warning);
    assert_eq!(EventType::Normal, EventType::Normal);
    assert_ne!(EventType::Warning, EventType::Normal);
}
