#[cfg(test)]
use crate::data::collector::ensure_readonly_kubectl_args;

#[test]
fn test_allow_get_commands() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "pods"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "nodes", "-o", "json"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "pods", "-n", "default"]).is_ok());
}

#[test]
fn test_allow_top_commands() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["top", "nodes"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["top", "pods", "--no-headers"]).is_ok());
}

#[test]
fn test_allow_logs_commands() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["logs", "my-pod"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["logs", "my-pod", "-f"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["logs", "my-pod", "-n", "default"]).is_ok());
}

#[test]
fn test_allow_config_commands() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "current-context"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "view"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "view", "--minify"]).is_ok());
}

#[test]
fn test_reject_non_kubectl_program() {
    assert!(ensure_readonly_kubectl_args("helm", &["list"]).is_err());
    assert!(ensure_readonly_kubectl_args("bash", &["-c", "rm -rf /"]).is_err());
    assert!(ensure_readonly_kubectl_args("curl", &["http://example.com"]).is_err());
}

#[test]
fn test_reject_destructive_commands() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["delete", "pod", "my-pod"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["apply", "-f", "config.yaml"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["create", "deployment", "my-app"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["patch", "node", "my-node"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["edit", "deployment", "my-app"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["exec", "my-pod", "--", "bash"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["port-forward", "my-pod", "8080:80"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["cp", "my-pod:/file", "/local"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["run", "my-pod", "--image", "nginx"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["set", "image", "deployment/my-app", "nginx"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["scale", "deployment", "my-app", "--replicas=5"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["rollout", "restart", "deployment/my-app"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["cordon", "my-node"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["drain", "my-node"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["taint", "node", "my-node", "key=value"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["label", "pod", "my-pod", "env=prod"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["annotate", "pod", "my-pod", "description=test"]).is_err());
}

#[test]
fn test_reject_empty_command() {
    assert!(ensure_readonly_kubectl_args("kubectl", &[]).is_err());
}

#[test]
fn test_reject_config_subcommands() {
    // Only current-context and view are allowed
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "set-context", "my-context"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "use-context", "my-context"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "delete-context", "my-context"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "set-cluster", "my-cluster"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "set-credentials", "my-user"]).is_err());
    assert!(ensure_readonly_kubectl_args("kubectl", &["config", "unset", "current-context"]).is_err());
}

#[test]
fn test_allow_get_with_various_resources() {
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "pods"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "nodes"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "services"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "deployments"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "events"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "namespaces"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "configmaps"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "secrets"]).is_ok());
    assert!(ensure_readonly_kubectl_args("kubectl", &["get", "all"]).is_ok());
}
