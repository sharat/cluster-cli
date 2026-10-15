use anyhow::Result;
use std::time::Duration;

use crate::cli::KubectlOptions;
use crate::config::Config;
use crate::data::collector;
use crate::data::health::calculate_health;
use crate::data::incidents::build_incident_buckets;
use crate::data::models::{ClusterSignals, ClusterSnapshot};
use crate::json_output::{
    JsonOutput, build_health_output, build_incidents_output, build_nodes_output, build_pods_output,
};

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Fetch a complete cluster snapshot for headless operations
async fn fetch_snapshot(kubectl_opts: &KubectlOptions, config: &Config) -> Result<ClusterSnapshot> {
    let namespace =
        kubectl_opts
            .namespace
            .clone()
            .or_else(|| {
                if !config.namespace.is_empty() { Some(config.namespace.clone()) } else { None }
            })
            .unwrap_or_else(|| "default".to_string());

    // Note: context parameter in kubectl_opts is currently not used because the underlying
    // collector functions don't support passing --context. Users should set their kubectl
    // context before invoking these commands.
    let context_name = collector::fetch_current_context().await.ok();

    // Fetch all data concurrently with timeout
    let fetch_result = tokio::time::timeout(FETCH_TIMEOUT, async {
        tokio::try_join!(
            collector::fetch_node_metrics(None),
            collector::fetch_pod_info(&namespace),
            collector::fetch_events(&namespace),
        )
    })
    .await??;

    let (nodes_result, pods_result, events) = fetch_result;

    let (nodes, node_warning) = nodes_result;
    let (pods, _pod_warning) = pods_result;

    let health = calculate_health(&ClusterSignals::new(&nodes, &pods, &events));
    let incident_buckets = build_incident_buckets(&ClusterSignals::new(&nodes, &pods, &events));

    Ok(ClusterSnapshot {
        nodes,
        workloads: vec![],
        pods,
        events,
        incident_buckets,
        health,
        fetched_at: std::time::Instant::now(),
        error: node_warning,
        context_name,
        coverage: Default::default(),
        resource_problems: vec![],
        metrics_sampled: true,
    })
}

pub async fn handle_status(
    kubectl_opts: &KubectlOptions,
    config: &Config,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let snapshot = fetch_snapshot(kubectl_opts, config).await?;
    let namespace = kubectl_opts.namespace.as_deref().unwrap_or(&config.namespace);

    if json {
        let output = build_health_output(&snapshot, namespace, verbose);
        let json_output = JsonOutput::new(output);
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("Cluster Health Status");
        println!("=====================");
        if let Some(ctx) = &snapshot.context_name {
            println!("Context:  {}", ctx);
        }
        println!("Namespace: {}", namespace);
        println!();
        println!("Score: {} (Grade: {})", snapshot.health.score, snapshot.health.grade);
        println!("Critical Nodes: {}", snapshot.health.critical_nodes);
        println!("Critical Pods:  {}", snapshot.health.critical_pods);
        println!("Total Restarts: {}", snapshot.health.total_restarts);

        if !snapshot.incident_buckets.is_empty() {
            println!();
            println!("Top Incidents:");
            for (i, incident) in snapshot.incident_buckets.iter().take(5).enumerate() {
                println!(
                    "  {}. {} ({:?}): {} occurrences, {} resources affected",
                    i + 1,
                    incident.reason,
                    incident.severity,
                    incident.occurrences,
                    incident.affected_resources.len()
                );
            }
        }
    }

    Ok(())
}

pub async fn handle_incidents(
    kubectl_opts: &KubectlOptions,
    config: &Config,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let snapshot = fetch_snapshot(kubectl_opts, config).await?;

    if json {
        let output = build_incidents_output(&snapshot, verbose);
        let json_output = JsonOutput::new(output);
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("Cluster Incidents");
        println!("=================");
        println!();
        let limit = if verbose { usize::MAX } else { 10 };
        for (i, incident) in snapshot.incident_buckets.iter().take(limit).enumerate() {
            println!("{}. {} [{:?}]", i + 1, incident.reason, incident.severity);
            println!("   Occurrences: {}", incident.occurrences);
            println!("   Affected: {}", incident.affected_resources.join(", "));
            if let Some(msg) = &incident.sample_message {
                println!("   Message: {}", msg);
            }
            println!();
        }
    }

    Ok(())
}

pub async fn handle_pods(
    kubectl_opts: &KubectlOptions,
    config: &Config,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let snapshot = fetch_snapshot(kubectl_opts, config).await?;

    if json {
        let output = build_pods_output(&snapshot, verbose);
        let json_output = JsonOutput::new(output);
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("Pods");
        println!("====");
        println!();
        println!("{:<40} {:<12} {:<8} {:<6} {:<12}", "NAME", "PHASE", "RESTARTS", "READY", "NODE");
        for pod in &snapshot.pods {
            println!(
                "{:<40} {:<12} {:<8} {:<6} {:<12}",
                pod.name,
                pod.phase,
                pod.restarts,
                if pod.is_ready { "Yes" } else { "No" },
                pod.node_name.as_deref().unwrap_or("N/A")
            );
        }
    }

    Ok(())
}

pub async fn handle_nodes(
    kubectl_opts: &KubectlOptions,
    config: &Config,
    json: bool,
    verbose: bool,
) -> Result<()> {
    let snapshot = fetch_snapshot(kubectl_opts, config).await?;

    if json {
        let output = build_nodes_output(&snapshot, verbose);
        let json_output = JsonOutput::new(output);
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!("Nodes");
        println!("=====");
        println!();
        println!("{:<30} {:<8} {:<10} {:<10} {:<8}", "NAME", "READY", "CPU%", "MEMORY%", "STATUS");
        for node in &snapshot.nodes {
            println!(
                "{:<30} {:<8} {:<10} {:<10} {:<8}",
                node.name,
                if node.ready { "Yes" } else { "No" },
                format!("{}%", node.cpu_pct),
                format!("{}%", node.memory_pct),
                format!("{:?}", node.status)
            );
        }
    }

    Ok(())
}

pub async fn handle_logs(
    kubectl_opts: &KubectlOptions,
    config: &Config,
    pod: &str,
    container: Option<&str>,
    tail: usize,
    follow: bool,
    timestamps: bool,
) -> Result<()> {
    let namespace = kubectl_opts.namespace.as_deref().unwrap_or(&config.namespace);

    let tail_str = tail.to_string();
    let mut args = vec!["logs", pod, "-n", namespace, "--tail", &tail_str];

    if let Some(c) = container {
        args.push("-c");
        args.push(c);
    }

    if follow {
        args.push("--follow");
    }

    if timestamps {
        args.push("--timestamps");
    }

    if let Some(ctx) = &kubectl_opts.context {
        args.push("--context");
        args.push(ctx);
    }

    // For logs, we stream directly instead of capturing
    let mut cmd = crate::data::collector::kubectl_command();
    cmd.args(&args).kill_on_drop(true);

    let status = cmd.status().await?;

    if !status.success() {
        anyhow::bail!("kubectl logs failed with exit code: {:?}", status.code());
    }

    Ok(())
}
