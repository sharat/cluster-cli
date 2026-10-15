# Headless Mode

Use cluster-cli in scripts, CI/CD pipelines, and automation with stable JSON output.

## Overview

cluster-cli provides a headless JSON API for programmatic access to cluster health, incidents, pods, nodes, and logs. All data commands support `--json` for machine-readable output with versioned schemas.

## JSON Schema

All JSON responses follow this structure:

```json
{
  "version": "1.0.0",
  "data": { ... }
}
```

The `version` field indicates the JSON schema version. Breaking changes increment the major version.

**Current Version**: `1.0.0`

Schema definitions: `src/json_output.rs`

## Commands

### cluster status

Get overall cluster health with score, grade, and critical resource counts.

```bash
cluster status --json
```

#### Output Structure

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
    "namespace": "production"
  }
}
```

`findings` is omitted unless `--verbose` is set.

#### With Verbose Flag

```bash
cluster status --json --verbose
```

Includes a findings array: critical node and pod counts, a restart finding when total restarts exceed 10, and the top 5 incidents:

```json
{
  "version": "1.0.0",
  "data": {
    "score": 75,
    "grade": "C",
    "critical_nodes": 1,
    "critical_pods": 3,
    "total_restarts": 12,
    "context": "prod-cluster",
    "namespace": "production",
    "findings": [
      {
        "category": "nodes",
        "severity": "critical",
        "message": "1 critical node(s) detected"
      },
      {
        "category": "pods",
        "severity": "critical",
        "message": "3 critical pod(s) detected"
      },
      {
        "category": "restarts",
        "severity": "warning",
        "message": "12 total pod restarts"
      },
      {
        "category": "incident",
        "severity": "critical",
        "message": "CrashLoopBackOff: 3 occurrence(s) affecting 1 resource(s)"
      }
    ]
  }
}
```

### cluster incidents

List cluster incidents ranked by severity.

```bash
cluster incidents --json
```

#### Output Structure

Returns top 10 incidents by default:

```json
{
  "version": "1.0.0",
  "data": [
    {
      "reason": "CrashLoopBackOff",
      "severity": "critical",
      "occurrences": 5,
      "affected_resources": ["Pod/api-6d4f9-x2k"],
      "targets": [
        {
          "kind": "Pod",
          "name": "api-6d4f9-x2k"
        }
      ],
      "latest_timestamp": "2026-10-14T06:30:00Z",
      "sample_message": "Back-off restarting failed container"
    },
    {
      "reason": "FailedScheduling",
      "severity": "warning",
      "occurrences": 3,
      "affected_resources": ["Pod/worker-0"],
      "targets": [
        {
          "kind": "Pod",
          "name": "worker-0"
        }
      ],
      "latest_timestamp": "2026-10-14T06:25:00Z",
      "sample_message": "0/3 nodes are available: insufficient memory"
    }
  ]
}
```

`severity` is `critical`, `warning`, or `elevated`. A target's `kind` is `Pod`, `Node`, `Container`, or a workload kind such as `Deployment`; `container` appears only on `Container` targets (whose `name` is the pod name), and `sample_message` only when one exists. `latest_timestamp` is an empty string for incidents derived from pod or node status rather than events.

#### With Verbose Flag

```bash
cluster incidents --json --verbose
```

Returns all incidents (no 10-item limit).

### cluster pods

List all pods with status and resource usage.

```bash
cluster pods --json
```

#### Output Structure

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
      "oom_killed": false
    }
  ]
}
```

- `cpu_millicores` and `memory_mb` are current usage from `kubectl top` (0 without metrics-server).
- `cpu_pct` and `memory_pct` are usage as a percentage of the pod's effective CPU and memory limits, or 0 when no limit is set. Effective limits include native sidecars and init containers the way Kubernetes computes them.
- `restarts`, `is_ready`, `crash_looping`, and `oom_killed` include native sidecars (init containers with `restartPolicy: Always`); `crash_looping` is also set when a regular init container is in CrashLoopBackOff.
- `status` is `healthy`, `elevated`, `warning`, or `critical`. `age` is a single unit (`45s`, `12m`, `3h`, `2d`).

#### With Verbose Flag

```bash
cluster pods --json --verbose
```

Includes per-container details for app containers and native sidecars (regular init containers are not listed):

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
      "containers": [
        {
          "name": "app",
          "ready": true,
          "restart_count": 2,
          "state": "Running",
          "last_termination_reason": "OOMKilled",
          "last_exit_code": 137
        },
        {
          "name": "log-shipper",
          "ready": true,
          "restart_count": 0,
          "state": "Running"
        }
      ]
    }
  ]
}
```

`state` is `Running`, `Waiting (<reason>)`, `Terminated (<reason>)`, or `Unknown`. `last_termination_reason` and `last_exit_code` come from the container's last termination (or its current one if it is terminated) and are omitted when there is none.

### cluster nodes

List all nodes with metrics and conditions.

```bash
cluster nodes --json
```

#### Output Structure

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
      "node_info": {
        "kernel_version": "5.15.0-1051-gcp",
        "os_image": "Ubuntu 22.04.3 LTS",
        "container_runtime": "containerd://1.7.2",
        "kubelet_version": "v1.28.3-gke.1286000",
        "architecture": "amd64",
        "operating_system": "linux"
      }
    }
  ]
}
```

- `cpu_capacity` is allocatable CPU in millicores; `cpu_pct` and `memory_pct` are usage relative to allocatable CPU and memory. `memory_capacity_mb` is the node's total memory capacity.
- `status` is `critical` when the node is NotReady or has an unhealthy condition, otherwise it follows memory usage (`elevated` from 60%, `warning` from 75%, `critical` from 85%).
- `node_info` is omitted when the node reports no system info. `draining` is currently always `false`.
- Headless commands ignore `node_pool_filter` and report every node.

#### With Verbose Flag

```bash
cluster nodes --json --verbose
```

Adds the node conditions:

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
      "node_info": {
        "kernel_version": "5.15.0-1051-gcp",
        "os_image": "Ubuntu 22.04.3 LTS",
        "container_runtime": "containerd://1.7.2",
        "kubelet_version": "v1.28.3-gke.1286000",
        "architecture": "amd64",
        "operating_system": "linux"
      },
      "conditions": {
        "ready": "True",
        "memory_pressure": "False",
        "disk_pressure": "False",
        "pid_pressure": "False",
        "network_unavailable": "False"
      }
    }
  ]
}
```

### cluster logs

Stream logs from a pod container.

```bash
cluster logs <pod-name> [OPTIONS]
```

#### Options

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--container` | `-c` | `<NAME>` | Container name (required for multi-container pods) |
| `--tail` | | `<LINES>` | Number of lines to show [default: 100] |
| `--follow` | `-f` | flag | Follow log output (live streaming) |
| `--timestamps` | | flag | Include Kubernetes timestamps |
| `--namespace` | `-n` | `<NAMESPACE>` | Kubernetes namespace (defaults to the config file namespace) |
| `--context` | | `<CONTEXT>` | kubectl context (passed to `kubectl logs`) |

#### Examples

```bash
# Get last 100 lines
cluster logs api-6d4f9-x2k --namespace production

# Get last 500 lines from specific container
cluster logs api-6d4f9-x2k -c app --tail 500

# Follow logs with timestamps
cluster logs api-6d4f9-x2k --follow --timestamps

# Native sidecar container in a multi-container pod
cluster logs worker-0 -c log-shipper --tail 200
```

`cluster logs` runs `kubectl logs` directly and streams its output. Output is plain text (not JSON):
```
2026-10-14T06:30:15.123Z Starting application...
2026-10-14T06:30:16.456Z Connected to database
2026-10-14T06:30:17.789Z Listening on port 8080
```

## kubectl Context Options

All headless commands accept kubectl context and namespace flags:

```bash
# Use specific context (accepted, but ignored by status/incidents/pods/nodes)
cluster status --json --context prod-cluster

# Use specific namespace
cluster status --json -n production

# Both together
cluster pods --json --context dev-cluster -n development
```

The namespace comes from `-n`/`--namespace`, then the config file `namespace`, then `"default"`. Unlike the TUI, headless commands do not read the namespace from the kubectl context.

**Note**: `--context` is passed to kubectl only by `cluster logs`. `status`, `incidents`, `pods`, and `nodes` accept it but always use the current kubectl context. Set your kubectl context before running them:

```bash
kubectl config use-context <context-name>
cluster status --json
```

## Scripting Examples

### Health Check Script

```bash
#!/bin/bash
# Check if cluster health is acceptable (Grade B or better)

HEALTH=$(cluster status --json -n production | jq -r '.data.score')

if [ "$HEALTH" -ge 75 ]; then
  echo "✓ Cluster health: $HEALTH (OK)"
  exit 0
else
  echo "✗ Cluster health: $HEALTH (CRITICAL)"
  exit 1
fi
```

### Incident Report

```bash
#!/bin/bash
# Generate incident report

cluster incidents --json --verbose -n production \
  | jq -r '.data[] | "\(.severity | ascii_upcase): \(.reason) (\(.occurrences)x) - \(.affected_resources | join(", "))"'
```

Output:
```
CRITICAL: CrashLoopBackOff (5x) - Pod/api-6d4f9-x2k
WARNING: FailedScheduling (3x) - Pod/worker-0
ELEVATED: FailedCreate (2x) - ReplicaSet/api-6d4f9
```

### Export All Pod Data

```bash
#!/bin/bash
# Export detailed pod information

cluster pods --json --verbose -n production \
  | jq -r '.data[] | [.name, .phase, .restarts, .cpu_millicores, .memory_mb, .is_ready] | @csv' \
  > pods-$(date +%Y%m%d).csv
```

### Find CrashLooping Pods

```bash
#!/bin/bash
# List all crash-looping pods

cluster pods --json -n production \
  | jq -r '.data[] | select(.crash_looping == true) | .name'
```

### Monitor Memory Usage

```bash
#!/bin/bash
# Alert if any pod exceeds 90% memory

cluster pods --json -n production \
  | jq -r '.data[] | select(.memory_pct > 90) | "\(.name): \(.memory_pct)%"'
```

## CI/CD Integration

### GitHub Actions

```yaml
name: Cluster Health Check
on:
  schedule:
    - cron: '0 */6 * * *'  # Every 6 hours
  workflow_dispatch:

jobs:
  health-check:
    runs-on: ubuntu-latest
    steps:
      - name: Install cluster-cli
        run: |
          curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash
          echo "$HOME/.local/bin" >> $GITHUB_PATH

      - name: Configure kubectl
        uses: azure/k8s-set-context@v3
        with:
          kubeconfig: ${{ secrets.KUBECONFIG }}

      - name: Check cluster health
        run: |
          cluster status --json -n production > health.json
          SCORE=$(jq -r '.data.score' health.json)
          if [ "$SCORE" -lt 70 ]; then
            echo "::error::Cluster health critical: $SCORE"
            exit 1
          fi

      - name: Upload health report
        uses: actions/upload-artifact@v4
        with:
          name: health-report
          path: health.json
```

### GitLab CI

```yaml
cluster-health-check:
  image: bitnami/kubectl:latest
  before_script:
    - curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash
    - export PATH="$HOME/.local/bin:$PATH"
  script:
    - cluster status --json -n production | tee health.json
    - |
      SCORE=$(jq -r '.data.score' health.json)
      if [ "$SCORE" -lt 70 ]; then
        echo "Cluster health critical: $SCORE"
        exit 1
      fi
  artifacts:
    paths:
      - health.json
    expire_in: 7 days
  only:
    - schedules
```

## Timeout Handling

Each kubectl command has a 30-second timeout, and the snapshot fetch for `status`, `incidents`, `pods`, and `nodes` has a 30-second overall timeout. If a command exceeds this:
- The command is terminated
- An error is returned
- Retry with `kubectl` manually to diagnose network/API issues

kubectl runs with `KUBECTL_KUBERC=false`, so kuberc defaults cannot change the output cluster-cli parses.

## Error Handling

Headless commands return:
- **Exit code 0**: Success
- **Exit code 1**: Error (parse failures, kubectl errors, timeouts)

Check stderr for error messages:
```bash
cluster status --json 2>errors.log
```

## Performance Considerations

### Fetching Concurrency

All data (nodes, pods, events) is fetched concurrently with a 30-second overall timeout. Every command fetches the same snapshot (all nodes, plus pods and the 50 most recent events in one namespace, Warnings kept first). For large clusters:
- Use namespace filtering: `-n <namespace>`
- Cache results and re-query periodically

### JSON Parsing

Use streaming JSON parsers for large datasets:
```bash
cluster pods --json --verbose | jq -c '.data[]' | while read pod; do
  # Process each pod individually
  echo "$pod" | jq -r '.name'
done
```

## Schema Versioning

The JSON schema version is included in every response. When upgrading cluster-cli, check for schema changes:

```bash
VERSION=$(cluster status --json | jq -r '.version')
if [ "$VERSION" != "1.0.0" ]; then
  echo "Warning: Unexpected schema version $VERSION"
fi
```

**Breaking changes** increment the major version (e.g., `1.0.0` → `2.0.0`).  
**Additive changes** increment the minor version (e.g., `1.0.0` → `1.1.0`).

## Next Steps

- [MCP Server](mcp.md) - Connect to AI agents for natural language queries
- [Configuration](configuration.md) - Set default namespaces and clusters
- [Health & Incidents](health.md) - Understanding the health scoring algorithm
