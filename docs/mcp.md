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

The server runs in the foreground, reading JSON-RPC requests from stdin and writing responses to stdout. Logs are written to stderr.

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
      "cpu_millicores": 150,
      "memory_mb": 512,
      "is_ready": true,
      "crash_looping": false,
      "oom_killed": false,
      "containers": [...]
    }
  ]
}
```

**Example prompts**:
- "List all pods in production"
- "Show me pods that are crash looping"
- "Which pods have high memory usage?"

### describe_pod

Get detailed information about a specific pod including containers, status, and events.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "name": "api-6d4f9-x2k",
    "namespace": "production",
    "phase": "Running",
    "containers": [
      {
        "name": "app",
        "ready": true,
        "restart_count": 2,
        "state": "Running",
        "last_termination_reason": "OOMKilled",
        "last_exit_code": 137
      }
    ]
  }
}
```

**Example prompts**:
- "Describe the api pod"
- "Show me details for worker-0"
- "What's the status of the nginx pod?"

### explain_pod

Explain the likely cause of pod issues by analyzing state, events, and related incidents.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace

**Returns**:
```json
{
  "version": "1.0.0",
  "data": {
    "pod_name": "api-6d4f9-x2k",
    "explanation": "Pod: api-6d4f9-x2k\nPhase: Running\nReady: false\n\nIssue: Container was OOMKilled\nLikely cause: Memory usage (512 MB) exceeded limits. Consider increasing memory limits.\n\nRelated incident: OOMKilled (3 occurrences)\n  Container was OOMKilled",
    "phase": "Running",
    "crash_looping": false,
    "oom_killed": true,
    "restarts": 2
  }
}
```

**Example prompts**:
- "Why is the api pod failing?"
- "Explain what's wrong with worker-0"
- "Diagnose the nginx pod issue"

### get_logs

Fetch recent logs from a pod container.

**Parameters**:
- `pod_name` (string, **required**): Name of the pod
- `namespace` (string, optional): Kubernetes namespace
- `container` (string, optional): Container name (required for multi-container pods)
- `tail` (integer, optional): Number of lines to fetch from the end (default: 100, max: 10000)

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
      "status": "healthy",
      "node_info": {...},
      "conditions": {...}
    }
  ]
}
```

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
    "contexts": "prod-cluster\ndev-cluster\nstaging-cluster"
  }
}
```

**Example prompts**:
- "What kubectl contexts are available?"
- "Which context am I using?"
- "List all clusters"

## Cursor Configuration

Add to `.cursor/mcp_config.json` in your workspace or home directory:

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
2. Look for the 🔌 icon showing connected MCP servers
3. cluster-cli tools will appear in the tools list

**Example queries in Claude**:
- "Using cluster-cli, check the health of my production namespace"
- "Show me all incidents in the cluster"
- "Explain why the api pod keeps crashing"

## VS Code Configuration

Install the MCP extension for VS Code, then configure:

Add to `.vscode/settings.json`:

```json
{
  "mcp.servers": {
    "cluster-cli": {
      "command": "cluster",
      "args": ["mcp"]
    }
  }
}
```

Or with absolute path:

```json
{
  "mcp.servers": {
    "cluster-cli": {
      "command": "/home/user/.local/bin/cluster",
      "args": ["mcp"]
    }
  }
}
```

## Multi-Cluster Setup

To work with multiple clusters, create separate MCP server entries:

```json
{
  "mcpServers": {
    "cluster-prod": {
      "command": "bash",
      "args": ["-c", "kubectl config use-context prod-cluster && cluster mcp"]
    },
    "cluster-dev": {
      "command": "bash",
      "args": ["-c", "kubectl config use-context dev-cluster && cluster mcp"]
    }
  }
}
```

Then specify which server to use:
- "@cluster-prod what's the health?"
- "@cluster-dev list all pods"

## Security Considerations

### Read-Only Access

The MCP server only exposes read-only operations:
- `get` (cluster state)
- `top` (metrics)
- `logs` (read logs)
- `config` (read context)

No mutation operations are available. Your cluster is safe.

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

The MCP server requires these Kubernetes permissions:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: cluster-cli-reader
rules:
  - apiGroups: [""]
    resources: ["nodes", "pods", "events", "namespaces"]
    verbs: ["get", "list"]
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

- MCP tools fetch data from kubectl with 30-second timeouts
- Large clusters may take longer
- Use namespace filtering to reduce data volume

### Context Issues

The MCP server uses the active kubectl context. To switch:
```bash
kubectl config use-context <context>
# Then restart the MCP server
```

For multi-cluster setups, use shell scripts in the MCP config (see Multi-Cluster Setup above).

## Example Workflows

### Health Check

**Prompt**: "Check the health of my production cluster"

**AI uses**: `get_cluster_health` → Returns score, grade, critical counts

### Incident Investigation

**Prompt**: "Show me all crash looping pods and explain why"

**AI uses**:
1. `list_incidents` → Find CrashLoopBackOff incidents
2. `describe_pod` → Get pod details
3. `explain_pod` → Analyze the issue
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
