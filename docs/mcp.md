# MCP Server

Connect cluster-cli to AI agents (Claude Desktop, Cursor, VS Code) using the Model Context Protocol (MCP).

## Overview

cluster-cli includes a built-in MCP server that exposes read-only Kubernetes cluster operations as tools for AI agents. This enables natural language queries about your cluster health, incidents, pods, nodes, and logs.

**Protocol**: [Model Context Protocol](https://modelcontextprotocol.io) 2024-11-05  
**Transport**: stdio (standard input/output)  
**Server Name**: `cluster-cli`  
**Server Version**: Matches cluster-cli version

## Starting the MCP Server

```bash
cluster mcp
```

The server runs in the foreground, reading one JSON-RPC request per line from stdin and writing one response per line to stdout. A startup line and request parse errors are written to stderr.

When a tool call omits `namespace`, the server uses the config file `namespace`, then `"default"`. It does not read the namespace from the kubectl context.

**Important**: The MCP server uses your current kubectl context. Switch contexts before starting:

```bash
kubectl config use-context <context-name>
cluster mcp
```

## Available Tools

### get_cluster_health

Get overall cluster health score, grade, and critical resource counts.

**Parameters**:
- `namespace` (string, optional): Kubernetes namespace (defaults to configured namespace or "default")
- `verbose` (boolean, optional): Include detailed findings (default: false)

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "score": 85,
    "grade": "B",
    "critical_nodes": 0,
    "critical_pods": 2,
    "total_restarts": 5,
    "context": "prod-cluster",
    "namespace": "production",
    "findings": [...]
  }
}
```

`findings` is present only with `verbose=true`. Its shape matches `cluster status --json --verbose` (see [Headless Mode](headless.md#cluster-status)).

**Example prompts**:
- "What's the health of my production cluster?"
- "Show me critical pods and nodes"
- "Is the cluster healthy?"

### list_incidents

List cluster incidents ranked by severity and occurrence count.

**Parameters**:
- `namespace` (string, optional): Kubernetes namespace
- `verbose` (boolean, optional): Include all incidents instead of top 10 (default: false)

**Returns**:
```json
{
  "version": "1.0.0",
  "data": [
    {
      "reason": "CrashLoopBackOff",
      "severity": "critical",
      "occurrences": 5,
      "affected_resources": ["Pod/api-6d4f9-x2k"],
      "targets": [...],
      "latest_timestamp": "2026-10-14T06:30:00Z",
      "sample_message": "Back-off restarting failed container"
    }
  ]
}
```

**Example prompts**:
- "What incidents are happening in production?"
- "Show me all critical issues"
- "List problems in the cluster"

### list_pods

List all pods with status, resource usage, and readiness.

**Parameters**:
- `namespace` (string, optional): Kubernetes namespace
- `verbose` (boolean, optional): Include per-container details (default: false)

**Returns**:
```json
{
  "version": "1.0.0",
  "data": [
    {
      "name": "api-6d4f9-x2k",
      "namespace": "production",
      "phase": "Running",
      "restarts": 2,
      "age": "3h",
      "node": "node-1",
      "cpu_millicores": 150,
      "memory_mb": 512,
      "cpu_pct": 30,
      "memory_pct": 50,
      "status": "healthy",
      "is_ready": true,
      "crash_looping": false,
      "oom_killed": false,
      "containers": [...]
    }
  ]
}
```

Fields match `cluster pods --json` (see [Headless Mode](headless.md#cluster-pods)). `containers` is present only with `verbose=true` and lists app containers and native sidecars (init containers with `restartPolicy: Always`). Restarts, readiness, `crash_looping`, and `oom_killed` include sidecars.

**Example prompts**:
- "List all pods in production"
- "Show me pods that are crash looping"
- "Which pods have high memory usage?"

### describe_pod

Get detailed information about a specific pod: the same fields as `list_pods`, always with containers. Events are not included; use `explain_pod` for an event-based diagnosis.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace

**Returns** (pod fields abbreviated):
```json
{
  "version": "1.0.0",
  "data": {
    "name": "api-6d4f9-x2k",
    "namespace": "production",
    "phase": "Running",
    "restarts": 2,
    "status": "critical",
    "is_ready": false,
    "crash_looping": false,
    "oom_killed": true,
    "containers": [
      {
        "name": "app",
        "ready": false,
        "restart_count": 2,
        "state": "Running",
        "last_termination_reason": "OOMKilled",
        "last_exit_code": 137
      }
    ]
  }
}
```

`containers` lists app containers and native sidecars. Regular init containers are not listed.

**Example prompts**:
- "Describe the api pod"
- "Show me details for worker-0"
- "What's the status of the nginx pod?"

### explain_pod

Explain the likely cause of pod issues by analyzing the pod's container states, its events, its node, and related incidents. It uses the same rule-based diagnosis as the pod detail view and makes no extra kubectl calls beyond the snapshot.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "pod_name": "api-6d4f9-x2k",
    "explanation": "Pod: api-6d4f9-x2k\nPhase: Running\nReady: false\n\nIssue (critical): Container app was OOMKilled\nLikely cause: Memory use exceeded its 512 MB limit; raise the limit or reduce usage\nNext: kubectl logs api-6d4f9-x2k -n production --previous\n\nRelated incident: OOMKilled (1 occurrences)\n  container was terminated by the kernel OOM killer",
    "phase": "Running",
    "crash_looping": false,
    "oom_killed": true,
    "restarts": 2,
    "findings": [
      {
        "severity": "critical",
        "issue": "Container app was OOMKilled",
        "likely_cause": "Memory use exceeded its 512 MB limit; raise the limit or reduce usage",
        "next_command": "kubectl logs api-6d4f9-x2k -n production --previous"
      }
    ]
  }
}
```

`findings` is ordered most severe first and is empty when no problems are detected (the explanation then says "No problems detected"). Each finding has:
- `severity`: `critical`, `warning`, or `elevated`
- `issue`: what is wrong
- `likely_cause`: the most likely cause
- `next_command`: a read-only kubectl command to run next (`kubectl describe pod`, `kubectl logs --previous`, `kubectl get events --field-selector ...`, `kubectl top pod --containers`, or `kubectl describe node`)

The diagnosis covers app containers, native sidecars, and init containers (image pull failures, config and start errors, OOM kills, crash loops with exit-code hints, repeated restarts), pending pods (scheduling and volume failures, a running init container or a sidecar that has not started), failed and evicted pods, failing liveness and readiness probes, problems on the pod's node, and memory or CPU at 85% or more of its limit.

`explanation` is a plain-text rendering of the same findings plus any incidents that target the pod.

**Example prompts**:
- "Why is the api pod failing?"
- "Explain what's wrong with worker-0"
- "Diagnose the nginx pod issue"

### get_logs

Fetch recent logs from a pod container.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace
- `container` (string, optional): Container name (required for multi-container pods, including pods with native sidecars)
- `tail` (integer, optional): Number of lines to fetch from the end (default: 100; the input schema advertises a maximum of 10000)

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "pod_name": "api-6d4f9-x2k",
    "container": "app",
    "lines": 100,
    "logs": "2026-10-14T06:30:15Z Starting application...\n2026-10-14T06:30:16Z Connected to database\n..."
  }
}
```

`lines` echoes the requested `tail`, and `container` is `null` when not given.

**Example prompts**:
- "Show me logs for the api pod"
- "Get the last 500 lines from worker-0"
- "Fetch logs from the sidecar container"

### list_nodes

List all nodes with CPU/memory usage, conditions, and health status.

**Parameters**:
- `verbose` (boolean, optional): Include detailed node conditions (default: false)

**Returns**:
```json
{
  "version": "1.0.0",
  "data": [
    {
      "name": "node-1",
      "ready": true,
      "cpu_millicores": 3400,
      "cpu_capacity": 4000,
      "cpu_pct": 85,
      "memory_mb": 13500,
      "memory_capacity_mb": 16000,
      "memory_pct": 84,
      "status": "warning",
      "unhealthy_conditions": 0,
      "cordoned": false,
      "draining": false,
      "node_info": {...},
      "conditions": {...}
    }
  ]
}
```

Fields match `cluster nodes --json` (see [Headless Mode](headless.md#cluster-nodes)). `conditions` is present only with `verbose=true`. All nodes are listed; `node_pool_filter` does not apply.

**Example prompts**:
- "List all nodes"
- "Which nodes have memory pressure?"
- "Show me node resource usage"

### list_contexts

List available kubectl contexts and show the current context.

**Parameters**: None

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "current": "prod-cluster",
    "contexts": ["prod-cluster", "dev-cluster", "staging-cluster"]
  }
}
```


**Example prompts**:
- "What kubectl contexts are available?"
- "Which context am I using?"
- "List all clusters"

## Cursor Configuration

Add to `.cursor/mcp.json` in your workspace, or `~/.cursor/mcp.json` for all workspaces:

```json
{
  "mcpServers": {
    "cluster-cli": {
      "command": "cluster",
      "args": ["mcp"]
    }
  }
}
```

Or use the absolute path to the binary:

```json
{
  "mcpServers": {
    "cluster-cli": {
      "command": "/home/user/.local/bin/cluster",
      "args": ["mcp"]
    }
  }
}
```

After adding the configuration:
1. Restart Cursor or reload the window
2. Open Cursor Composer or Agent chat
3. The cluster-cli tools will be available automatically

**Example queries in Cursor**:
- "@cluster-cli what's the health of production?"
- "@cluster-cli show me all crash looping pods"
- "@cluster-cli explain why the api pod is failing"

## Claude Desktop Configuration

Add to the Claude Desktop config file:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`  
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`  
**Linux**: `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "cluster-cli": {
      "command": "/Users/username/.local/bin/cluster",
      "args": ["mcp"]
    }
  }
}
```

**Important**: Use the absolute path to the cluster binary. Find it with:
```bash
which cluster
```

After adding the configuration:
1. Restart Claude Desktop
2. Check that cluster-cli appears among the connected MCP servers
3. cluster-cli tools will appear in the tools list

**Example queries in Claude**:
- "Using cluster-cli, check the health of my production namespace"
- "Show me all incidents in the cluster"
- "Explain why the api pod keeps crashing"

## VS Code Configuration

Add to `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "cluster-cli": {
      "type": "stdio",
      "command": "cluster",
      "args": ["mcp"]
    }
  }
}
```

Or with absolute path:

```json
{
  "servers": {
    "cluster-cli": {
      "type": "stdio",
      "command": "/home/user/.local/bin/cluster",
      "args": ["mcp"]
    }
  }
}
```

## Multi-Cluster Setup

To work with multiple clusters, create separate MCP server entries, each pointing kubectl at its own kubeconfig through `KUBECONFIG`:

```json
{
  "mcpServers": {
    "cluster-prod": {
      "command": "cluster",
      "args": ["mcp"],
      "env": { "KUBECONFIG": "/Users/username/.kube/prod-config" }
    },
    "cluster-dev": {
      "command": "cluster",
      "args": ["mcp"],
      "env": { "KUBECONFIG": "/Users/username/.kube/dev-config" }
    }
  }
}
```

Avoid running `kubectl config use-context` in the server command: it changes the current context in the shared kubeconfig, so the last server started would switch every other server to its cluster.

Then specify which server to use:
- "@cluster-prod what's the health?"
- "@cluster-dev list all pods"

## Security Considerations

### Read-Only Access

The MCP server only runs read-only kubectl commands, enforced by a whitelist:
- `get` (cluster state)
- `top` (metrics)
- `logs` (read logs)
- `config current-context` and `config view` (read context)

Mutation verbs are rejected, as are flags that could redirect requests or swap credentials (`--raw`, `--kubeconfig`, `--server`, `--token`, `--as`). kubectl runs with `KUBECTL_KUBERC=false`, so kuberc aliases and default flags do not apply.

### Credential Access

The MCP server inherits kubectl's authentication:
- Uses your `~/.kube/config` or `KUBECONFIG`
- Respects RBAC permissions
- No credentials are stored, logged, or transmitted

### Network Access

The MCP server:
- Does NOT make any network requests except to the Kubernetes API (via kubectl)
- Does NOT send data to external services
- Does NOT require internet access (except for kubectl → API communication)

### RBAC Requirements

The MCP tools read nodes, pods, pod logs, events, and metrics. The role below also covers the TUI, which additionally reads namespaces and workloads:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: cluster-cli-reader
rules:
  - apiGroups: [""]
    resources: ["nodes", "pods", "events", "namespaces"]
    verbs: ["get", "list"]
  - apiGroups: [""]
    resources: ["pods/log"]
    verbs: ["get"]
  - apiGroups: ["apps"]
    resources: ["deployments", "statefulsets", "daemonsets", "replicasets"]
    verbs: ["get", "list"]
  - apiGroups: ["batch"]
    resources: ["jobs", "cronjobs"]
    verbs: ["get", "list"]
  - apiGroups: ["autoscaling"]
    resources: ["horizontalpodautoscalers"]
    verbs: ["get", "list"]
  - apiGroups: ["policy"]
    resources: ["poddisruptionbudgets"]
    verbs: ["get", "list"]
  - apiGroups: [""]
    resources: ["services", "persistentvolumeclaims"]
    verbs: ["get", "list"]
  - apiGroups: ["networking.k8s.io"]
    resources: ["ingresses"]
    verbs: ["get", "list"]
  - apiGroups: ["metrics.k8s.io"]
    resources: ["nodes", "pods"]
    verbs: ["get", "list"]
```

## Troubleshooting

### MCP Server Not Connecting

1. **Check cluster binary location**:
   ```bash
   which cluster
   ```
   Use the absolute path in the MCP config.

2. **Test the server manually**:
   ```bash
   echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cluster mcp
   ```
   Should return a JSON response with server info.

3. **Check kubectl access**:
   ```bash
   kubectl cluster-info
   kubectl get nodes
   ```

4. **Check MCP config syntax**:
   Ensure JSON is valid (no trailing commas, proper quotes).

### Tools Not Appearing

1. **Restart the AI client** (Cursor, Claude Desktop, VS Code)
2. **Check stderr logs** in the terminal where the MCP server runs
3. **Verify RBAC permissions** with kubectl

### Slow Responses

- Each kubectl command the MCP tools run has a 30-second timeout
- Most tools fetch a full snapshot (all nodes, plus pods and events in one namespace), so large clusters may take longer
- Use namespace filtering to reduce data volume

### Context Issues

The MCP server uses the active kubectl context. To switch:
```bash
kubectl config use-context <context>
# Then restart the MCP server
```

For multi-cluster setups, give each server entry its own `KUBECONFIG` (see Multi-Cluster Setup above).

## Example Workflows

### Health Check

**Prompt**: "Check the health of my production cluster"

**AI uses**: `get_cluster_health` → Returns score, grade, critical counts

### Incident Investigation

**Prompt**: "Show me all crash looping pods and explain why"

**AI uses**:
1. `list_incidents` → Find CrashLoopBackOff incidents
2. `describe_pod` → Get pod details
3. `explain_pod` → Get findings with likely causes and next commands
4. `get_logs` → Fetch recent logs

### Resource Monitoring

**Prompt**: "Which pods are using the most memory?"

**AI uses**: `list_pods` with `verbose=true` → Sorts by memory_mb

### Node Health

**Prompt**: "Are any nodes having issues?"

**AI uses**: `list_nodes` with `verbose=true` → Checks conditions, pressure

## API Reference

Full JSON-RPC 2.0 protocol. See [MCP Specification](https://modelcontextprotocol.io) for details.

### Request Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "get_cluster_health",
    "arguments": {
      "namespace": "production",
      "verbose": true
    }
  }
}
```

### Response Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"version\":\"1.0.0\",\"data\":{...}}"
      }
    ]
  }
}
```

### Error Format

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32000,
    "message": "Pod not found: api-xyz"
  }
}
```

## Next Steps

- [Headless Mode](headless.md) - Direct JSON API for scripts
- [Health & Incidents](health.md) - Understanding health scoring
- [User Interface](ui.md) - Interactive TUI for manual inspection
