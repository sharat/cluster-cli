use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

use crate::config::Config;
use crate::data::collector;
use crate::data::diagnosis::diagnose_pod;
use crate::data::health::calculate_health;
use crate::data::incidents::build_incident_buckets;
use crate::data::models::ClusterSnapshot;
use crate::json_output::{
    JSON_SCHEMA_VERSION, JsonOutput, build_health_output, build_incidents_output,
    build_nodes_output, build_pods_output,
};

const MCP_SERVER_NAME: &str = "cluster-cli";
const MCP_SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

struct ClusterMcpServer {
    config: Config,
}

impl ClusterMcpServer {
    fn new(config: Config) -> Self {
        Self { config }
    }

    async fn fetch_snapshot(&self, namespace: Option<&str>) -> Result<ClusterSnapshot> {
        let namespace = namespace
            .map(String::from)
            .or_else(|| {
                if !self.config.namespace.is_empty() {
                    Some(self.config.namespace.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "default".to_string());

        let context_name = collector::fetch_current_context().await.ok();

        let fetch_result = tokio::try_join!(
            collector::fetch_node_metrics(None),
            collector::fetch_pod_info(&namespace),
            collector::fetch_events(&namespace),
        )?;

        let (nodes_result, pods_result, events) = fetch_result;
        let (nodes, node_warning) = nodes_result;
        let (pods, _) = pods_result;

        let health = calculate_health(&nodes, &pods, &events);
        let incident_buckets = build_incident_buckets(&nodes, &pods, &events);

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
        })
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": MCP_SERVER_NAME,
                "version": MCP_SERVER_VERSION
            },
            "capabilities": {
                "tools": {}
            }
        })
    }

    fn handle_list_tools(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "get_cluster_health",
                    "description": "Get overall cluster health score, grade, and critical resource counts. Returns ranked findings when verbose=true. Uses the current kubectl context.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "namespace": {
                                "type": "string",
                                "description": "Kubernetes namespace to check (defaults to configured namespace or 'default')"
                            },
                            "verbose": {
                                "type": "boolean",
                                "description": "Include detailed findings",
                                "default": false
                            }
                        }
                    }
                },
                {
                    "name": "list_incidents",
                    "description": "List cluster incidents ranked by severity and occurrence count. Returns top 10 by default or all with verbose=true.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "namespace": {"type": "string", "description": "Kubernetes namespace to check"},
                            "verbose": {"type": "boolean", "description": "Include all incidents instead of top 10", "default": false}
                        }
                    }
                },
                {
                    "name": "list_pods",
                    "description": "List all pods with status, resource usage, and readiness. Include container details with verbose=true.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "namespace": {"type": "string", "description": "Kubernetes namespace"},
                            "verbose": {"type": "boolean", "description": "Include per-container details", "default": false}
                        }
                    }
                },
                {
                    "name": "describe_pod",
                    "description": "Get detailed information about a specific pod including its containers, status, and events.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "pod_name": {"type": "string", "description": "Name of the pod"},
                            "namespace": {"type": "string", "description": "Kubernetes namespace"}
                        },
                        "required": ["pod_name"]
                    }
                },
                {
                    "name": "explain_pod",
                    "description": "Explain the likely cause of pod issues by analyzing its state, events, node, and related incidents. Returns findings ordered most severe first, each with a likely cause and a read-only kubectl command to run next.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "pod_name": {"type": "string", "description": "Name of the pod"},
                            "namespace": {"type": "string", "description": "Kubernetes namespace"}
                        },
                        "required": ["pod_name"]
                    }
                },
                {
                    "name": "get_logs",
                    "description": "Fetch recent logs from a pod container. Returns last 100 lines by default.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "pod_name": {"type": "string", "description": "Name of the pod"},
                            "namespace": {"type": "string", "description": "Kubernetes namespace"},
                            "container": {"type": "string", "description": "Container name (required if pod has multiple containers)"},
                            "tail": {"type": "integer", "description": "Number of lines to fetch from the end", "default": 100, "minimum": 1, "maximum": 10000}
                        },
                        "required": ["pod_name"]
                    }
                },
                {
                    "name": "list_nodes",
                    "description": "List all nodes with CPU/memory usage, conditions, and health status. Include full conditions with verbose=true.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "verbose": {"type": "boolean", "description": "Include detailed node conditions", "default": false}
                        }
                    }
                },
                {
                    "name": "list_contexts",
                    "description": "List available kubectl contexts and show the current context.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {}
                    }
                }
            ]
        })
    }

    async fn handle_call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let namespace = arguments.get("namespace").and_then(|v| v.as_str());
        let verbose = arguments.get("verbose").and_then(|v| v.as_bool()).unwrap_or(false);

        match name {
            "get_cluster_health" => {
                let snapshot = self.fetch_snapshot(namespace).await?;
                let ns = namespace.unwrap_or(&self.config.namespace);
                let output = build_health_output(&snapshot, ns, verbose);
                let json_output = JsonOutput::new(output);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json_output)?
                    }]
                }))
            }

            "list_incidents" => {
                let snapshot = self.fetch_snapshot(namespace).await?;
                let output = build_incidents_output(&snapshot, verbose);
                let json_output = JsonOutput::new(output);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json_output)?
                    }]
                }))
            }

            "list_pods" => {
                let snapshot = self.fetch_snapshot(namespace).await?;
                let output = build_pods_output(&snapshot, verbose);
                let json_output = JsonOutput::new(output);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json_output)?
                    }]
                }))
            }

            "describe_pod" => {
                let pod_name = arguments
                    .get("pod_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("pod_name is required"))?;

                let snapshot = self.fetch_snapshot(namespace).await?;
                let pod = snapshot
                    .pods
                    .iter()
                    .find(|p| p.name == pod_name)
                    .ok_or_else(|| anyhow::anyhow!("Pod not found: {}", pod_name))?;

                let mut output = crate::json_output::PodOutput::from(pod);
                output.containers = Some(
                    pod.containers
                        .iter()
                        .map(|c| crate::json_output::ContainerOutput {
                            name: c.name.clone(),
                            ready: c.ready,
                            restart_count: c.restart_count,
                            state: c.state.clone(),
                            last_termination_reason: c.last_termination_reason.clone(),
                            last_exit_code: c.last_exit_code,
                        })
                        .collect(),
                );

                let json_output = JsonOutput::new(output);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json_output)?
                    }]
                }))
            }

            "explain_pod" => {
                let pod_name = arguments
                    .get("pod_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("pod_name is required"))?;

                let snapshot = self.fetch_snapshot(namespace).await?;
                let pod = snapshot
                    .pods
                    .iter()
                    .find(|p| p.name == pod_name)
                    .ok_or_else(|| anyhow::anyhow!("Pod not found: {}", pod_name))?;

                let findings = diagnose_pod(pod, &snapshot);

                let mut explanation = Vec::new();
                explanation.push(format!("Pod: {}", pod.name));
                explanation.push(format!("Phase: {}", pod.phase));
                explanation.push(format!("Ready: {}", pod.is_ready));

                if findings.is_empty() {
                    explanation.push("\nNo problems detected".to_string());
                }
                for finding in &findings {
                    explanation.push(format!(
                        "\nIssue ({}): {}",
                        finding.severity.label(),
                        finding.issue
                    ));
                    explanation.push(format!("Likely cause: {}", finding.likely_cause));
                    explanation.push(format!("Next: {}", finding.next_command));
                }

                for incident in &snapshot.incident_buckets {
                    if incident.targets.iter().any(|t| {
                        matches!(t, crate::data::models::IncidentTarget::Pod { pod_name: name } if name == &pod.name)
                    }) {
                        explanation.push(format!("\nRelated incident: {} ({} occurrences)", incident.reason, incident.occurrences));
                        if let Some(msg) = &incident.sample_message {
                            explanation.push(format!("  {}", msg));
                        }
                    }
                }

                let result = json!({
                    "version": JSON_SCHEMA_VERSION,
                    "data": {
                        "pod_name": pod.name,
                        "explanation": explanation.join("\n"),
                        "phase": pod.phase,
                        "crash_looping": pod.crash_looping,
                        "oom_killed": pod.oom_killed,
                        "restarts": pod.restarts,
                        "findings": findings
                            .iter()
                            .map(|f| json!({
                                "severity": f.severity.label(),
                                "issue": f.issue,
                                "likely_cause": f.likely_cause,
                                "next_command": f.next_command,
                            }))
                            .collect::<Vec<_>>(),
                    }
                });

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }

            "get_logs" => {
                let pod_name = arguments
                    .get("pod_name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("pod_name is required"))?;
                let container = arguments.get("container").and_then(|v| v.as_str());
                let tail = arguments.get("tail").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

                let ns = namespace.unwrap_or(&self.config.namespace);
                let tail_str = tail.to_string();

                let mut args = vec!["logs", pod_name, "-n", ns, "--tail", &tail_str];

                if let Some(c) = container {
                    args.push("-c");
                    args.push(c);
                }

                let output = collector::run_kubectl_output(&args).await?;

                let result = json!({
                    "version": JSON_SCHEMA_VERSION,
                    "data": {
                        "pod_name": pod_name,
                        "container": container,
                        "lines": tail,
                        "logs": output
                    }
                });

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }

            "list_nodes" => {
                let snapshot = self.fetch_snapshot(namespace).await?;
                let output = build_nodes_output(&snapshot, verbose);
                let json_output = JsonOutput::new(output);
                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&json_output)?
                    }]
                }))
            }

            "list_contexts" => {
                let contexts_output = collector::fetch_contexts().await?;
                let current_context = collector::fetch_current_context().await.ok();

                let result = json!({
                    "version": JSON_SCHEMA_VERSION,
                    "data": {
                        "current": current_context,
                        "contexts": contexts_output
                    }
                });

                Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&result)?
                    }]
                }))
            }

            _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
        }
    }

    async fn handle_request(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        let id = req.id.clone();

        let result = match req.method.as_str() {
            "initialize" => Ok(self.handle_initialize()),
            "tools/list" => Ok(self.handle_list_tools()),
            "tools/call" => {
                let params = req.params.unwrap_or(json!({}));
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                self.handle_call_tool(name, arguments).await
            }
            _ => Err(anyhow::anyhow!("Unknown method: {}", req.method)),
        };

        match result {
            Ok(res) => {
                JsonRpcResponse { jsonrpc: "2.0".to_string(), id, result: Some(res), error: None }
            }
            Err(err) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError { code: -32000, message: err.to_string() }),
            },
        }
    }
}

pub async fn run_mcp_server(config: Config) -> Result<()> {
    let server = ClusterMcpServer::new(config);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    eprintln!("Starting cluster-cli MCP server v{}", MCP_SERVER_VERSION);

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Failed to parse request: {}", e);
                continue;
            }
        };

        let response = server.handle_request(request).await;
        let response_str = serde_json::to_string(&response)?;

        writeln!(stdout, "{}", response_str)?;
        stdout.flush()?;
    }

    Ok(())
}
