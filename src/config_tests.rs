#[cfg(test)]
use crate::config::Config;

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.namespace, "");
    assert_eq!(config.resource_group, None);
    assert_eq!(config.cluster_name, None);
    assert_eq!(config.refresh_interval_secs, 60);
    assert_eq!(config.node_pool_filter, None);
}

#[test]
fn test_config_clone() {
    let config = Config {
        namespace: "test".to_string(),
        resource_group: Some("rg".to_string()),
        cluster_name: Some("cluster".to_string()),
        refresh_interval_secs: 120,
        node_pool_filter: Some("pool1".to_string()),
    };
    
    let cloned = config.clone();
    assert_eq!(cloned.namespace, "test");
    assert_eq!(cloned.resource_group, Some("rg".to_string()));
    assert_eq!(cloned.cluster_name, Some("cluster".to_string()));
    assert_eq!(cloned.refresh_interval_secs, 120);
    assert_eq!(cloned.node_pool_filter, Some("pool1".to_string()));
}

#[test]
fn test_config_debug() {
    let config = Config::default();
    let debug_str = format!("{:?}", config);
    assert!(debug_str.contains("Config"));
    assert!(debug_str.contains("namespace"));
}
