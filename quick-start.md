# Quick Start

Get up and running with cluster-cli in 5 minutes.

## 1. Install cluster-cli

```bash
curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash
```

## 2. Verify kubectl Access

Ensure `kubectl` is configured and you have cluster access:

```bash
kubectl cluster-info
kubectl get nodes
```

CPU/memory usage and the right-sizing view need metrics-server in the cluster (`kubectl top nodes` should work). Without it, cluster-cli still shows status, health, and incidents.

## 3. Launch cluster-cli

```bash
cluster
```

The TUI will launch and display your cluster's health dashboard.

## First Look: The Dashboard

When you first launch cluster-cli, you'll see the health score, then three panels: Nodes and the Incident Queue side by side, and the Pods table below them:

```
┌─ Nodes (2) ──────────────┐┌─ Incident Queue (2) ──────────────┐
│ node-1     Ready  85%    ││ CrashLoopBackOff    (3)           │
│ node-2     Ready  72%    ││ FailedScheduling    (2)           │
└──────────────────────────┘└───────────────────────────────────┘
┌─ Pods ────────────────────────────────────────────────────────┐
│ api-6d4f9-x2k  Running                                        │
│ worker-0       Running                                        │
└───────────────────────────────────────────────────────────────┘
```

- **Nodes**: CPU/memory usage and health status
- **Incident Queue**: Ranked problems requiring attention
- **Pods**: Pod status and resource consumption

## Essential Keyboard Shortcuts

### Navigation
- `q` or `Ctrl+C` - Quit
- `Tab` - Cycle between panels
- `↑`/`k` and `↓`/`j` - Move cursor
- `Enter` - View details or drill into incident

### Views
- `t` - Open timeline
- `T` - Open timeline for the selected incident (Incident Queue focused)
- `u` - Open the right-sizing view (needs metrics-server)
- `w` - Toggle workload popup

### Namespace
- `n` - Open namespace selector
- `N` - Enter namespace manually

### Refresh
- `r` - Manual refresh
- `R` - Change refresh interval

### Export
- `E` - Export pods to CSV

## Common Workflows

### View Pod Details

1. Navigate to the Pods panel (press `3` or `Tab` to focus)
2. Use `↑`/`↓` or `j`/`k` to select a pod
3. Press `Enter` to view details

In pod detail view:
- `Tab` - Cycle sections (Overview → Events → Logs)
- `f` - Follow logs (Logs section)
- `c` - Switch containers, including native sidecars (Logs section)
- `/` - Search logs (Logs section)
- `Esc` or `q` - Return to dashboard

### View Node Details

1. Navigate to the Nodes panel (press `1`)
2. Select a node with `↑`/`↓`
3. Press `Enter`

Node detail shows:
- Hardware specs (CPU, memory capacity)
- System info (kernel, OS, container runtime, kubelet version)
- Real-time resource usage
- Conditions (Ready, MemoryPressure, DiskPressure, PIDPressure, NetworkUnavailable) and scheduling state

### Investigate an Incident

1. Navigate to the Incident Queue (press `2`)
2. Select an incident
3. Press `Enter` to jump to the related resource

Or press `T` to open the timeline for that incident.

### Switch Namespace

#### Using the selector (recommended):
1. Press `n` to open namespace selector
2. Use `↑`/`↓` to navigate
3. Press `Enter` to switch

The selector shows pod counts for each namespace.

#### Manual entry:
1. Press `N`
2. Type the namespace name
3. Press `Enter`

### Sort Pods

Press `s` to cycle through sort modes:
- **Default**: As returned by kubectl
- **Restarts**: Most restarts first
- **CPU**: Highest CPU usage first
- **Memory**: Highest memory usage first
- **Not Ready**: Unready pods first
- **Latest Incident**: Pods with recent incidents first

### Export Pod Data

1. Press `E` in the dashboard
2. Enter a filename (or accept the suggested default, `<context>-<namespace>-<timestamp>.csv`)
3. Press `Enter`

The file is written to the current directory; the name cannot include a path, and an existing file is never overwritten.

CSV columns: `status` (health and phase), `pod`, `cpu_pct`, `cpu_use`, `cpu_req`, `cpu_lim`, `mem_pct`, `mem_use`, `mem_req`, `mem_lim`, `restarts`, `age`.

## Command-Line Usage

### Monitor a Specific Namespace

```bash
cluster -n production
```

### Set Refresh Interval

```bash
cluster --refresh 30  # 30 seconds
cluster --frequency 120  # 2 minutes (alternate flag name)
```

### Filter Node Pool (AKS/EKS/GKE)

```bash
cluster --node-pool-filter nodepool1
```

### Set Cluster Name (for Display)

```bash
cluster -n production -c prod-cluster-us-east --refresh 60
```

## Trying cluster-cli Without a Real Cluster

Don't have a Kubernetes cluster ready?

1. **Install minikube or kind**:
   ```bash
   # minikube
   minikube start
   
   # or kind
   kind create cluster
   ```

2. **Deploy example workloads**:
   ```bash
   kubectl create deployment nginx --image=nginx --replicas=3
   kubectl expose deployment nginx --port=80
   ```

3. **Launch cluster-cli**:
   ```bash
   cluster
   ```

You can also check out the [Kubernetes Examples](https://github.com/kubernetes/examples) repository for more sample applications.

## Configuration File

For persistent settings, create `config.toml` in the config directory (`~/.config/cluster-cli/` on Linux, `~/Library/Application Support/cluster-cli/` on macOS; `cluster --info` prints it):

```toml
namespace = "production"
refresh_interval_secs = 60
cluster_name = "prod-us-east"
node_pool_filter = "nodepool1"
```

See [Configuration](configuration.md) for all available options.

## Next Steps

- [User Interface Guide](ui.md) - Complete key binding reference
- [Health & Incidents](health.md) - Understanding cluster health scores
- [Configuration](configuration.md) - Advanced configuration options
- [MCP Server](mcp.md) - Connect to AI agents
