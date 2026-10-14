# Overview

cluster-cli is a fast, interactive terminal UI (TUI) for monitoring Kubernetes cluster health in real-time. Built in Rust using ratatui and crossterm, it provides a unified dashboard for tracking nodes, pods, incidents, and cluster health.

## Key Features

### Real-Time Monitoring
- **Dashboard View**: Unified interface showing nodes, incidents, and pods
- **Health Scoring**: 0-100 score with A-F grading based on cluster state
- **Incident Queue**: Ranked warning and failure signals from nodes, pods, and events
- **Live Updates**: Configurable refresh intervals (1s - 24h)

### Node Monitoring
- CPU and memory usage with percentage indicators
- Node conditions (Ready, MemoryPressure, DiskPressure, PIDPressure)
- Hardware specifications and system information
- Node pool filtering for AKS, EKS, GKE, Karpenter, Cluster API, kOps

### Pod Management
- Pod status, resource consumption, restarts, and age tracking
- Multi-container support with per-container status
- Pod history with CPU/memory sparklines
- Crash loop and OOMKilled detection
- Sort modes: default, restarts, CPU, memory, not-ready, latest incident

### Workload Inspection
- Deployments, StatefulSets, DaemonSets, Jobs, CronJobs
- HorizontalPodAutoscalers and PodDisruptionBudgets
- Services, Ingresses, PersistentVolumeClaims
- Rollout status and workload-related events

### Log Viewing
- Live and previous container log streaming
- Multi-container log switching
- Search, timestamps, wrapping, and export
- Configurable tail lines

### Incident Timeline
- Chronological view of events, restarts, OOM kills, and rollouts
- Precursor highlighting (events that likely caused incidents)
- Filtering by kind, namespace, severity
- Time-based correlation and grouping

### Namespace Management
- Interactive namespace selector with pod counts
- Color-coded current namespace indicator
- Manual namespace entry

### Export & Integration
- CSV export for pod lists
- JSON API for scripts and automation
- MCP server for AI agents (Claude Desktop, Cursor, VS Code)

## Design Principles

### Read-Only by Design
cluster-cli only executes read-only `kubectl` commands. The enforced whitelist includes:
- `get` (nodes, pods, events, namespaces, workloads)
- `top` (node and pod metrics)
- `logs` (container logs)
- `config` (context queries)

All mutation verbs are blocked. The application does not persist local state or create files during normal operation (except when explicitly exporting data).

### Universal Kubernetes Support
Works with any Kubernetes cluster:
- **Cloud Providers**: AKS (Azure), EKS (AWS), GKE (GCP), OKE (Oracle)
- **On-Premises**: Any self-managed Kubernetes
- **Local Development**: minikube, kind, k3s, microk8s

The only requirement is a properly configured `kubectl` context with appropriate RBAC permissions.

### Performance
- Async architecture using Tokio runtime
- Concurrent data fetching with 30-second timeouts
- Efficient terminal rendering with ratatui
- Minimal memory footprint

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Terminal UI (ratatui)                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────┐ │
│  │Dashboard │  │Pod Detail│  │Node Detail│ │Timeline │ │
│  └──────────┘  └──────────┘  └──────────┘  └─────────┘ │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   AppState & Event Loop                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ User Input   │  │ Data Events  │  │ Tick Timer   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                  Background Fetcher Task                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ Node Metrics │  │  Pod Info    │  │   Events     │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
                    kubectl subprocess
                    (30s timeout, read-only)
```

## Data Flow

1. **User Input** → Event handler → State updates → UI render
2. **Periodic Refresh** → kubectl commands → Data parsing → State update
3. **Background Task** → Concurrent fetches (nodes, pods, events, workloads) → Channel → Main thread

No shared mutable state between tasks. All communication flows through channels.

## Troubleshooting

### "kubectl: command not found"

Ensure `kubectl` is installed and in your PATH:
```bash
which kubectl
kubectl version --client
```

If missing, install kubectl and press `r` to retry in the app.

### No Active Context or Namespace

Connect to a cluster first:
```bash
kubectl config current-context
kubectl config use-context <context>
kubectl get namespaces
```

Press `n` to choose a namespace, `N` to enter one manually, or `r` to retry after fixing the context.

### Permission Errors

Verify RBAC permissions:
```bash
kubectl get nodes
kubectl get pods -n <namespace>
kubectl top nodes
kubectl top pods -n <namespace>
kubectl get namespaces
kubectl get events -n <namespace>
```

### No Data Appears

1. Check current context: `kubectl config current-context`
2. Check current namespace: `kubectl config view --minify --output 'jsonpath={..namespace}'`
3. Verify resources exist: `kubectl get pods -n <namespace>` and `kubectl get nodes`
4. Run with debug logs: `RUST_LOG=debug cluster 2>cluster.log` and inspect the log file

If the dashboard shows a connection error, fix the underlying `kubectl` issue and press `r` to retry.

### Metrics Unavailable

If CPU/memory metrics show 0% or "N/A", ensure the [Metrics Server](https://github.com/kubernetes-sigs/metrics-server) is installed:
```bash
kubectl get deployment metrics-server -n kube-system
```

The app will still function but resource usage will be unavailable.

## Project Structure

```
cluster-cli/
├── src/
│   ├── main.rs              # Entry point, event loop
│   ├── app.rs               # Application state management
│   ├── cli.rs               # CLI argument parsing
│   ├── config.rs            # Configuration handling
│   ├── mcp.rs               # MCP server implementation
│   ├── headless.rs          # JSON API handlers
│   ├── json_output.rs       # JSON schema definitions
│   ├── updater.rs           # Release checking and upgrades
│   ├── data/
│   │   ├── collector.rs     # kubectl data collection
│   │   ├── fetcher.rs       # Async data fetching
│   │   ├── health.rs        # Health score calculation
│   │   ├── incidents.rs     # Incident bucketing
│   │   ├── timeline.rs      # Timeline building
│   │   └── models.rs        # Data structures
│   ├── events/
│   │   ├── handler.rs       # Input/keyboard handling
│   │   └── mod.rs           # Event definitions
│   └── ui/
│       ├── mod.rs           # UI rendering coordination
│       ├── theme.rs         # Colors and styling
│       ├── format.rs        # Text formatting utilities
│       ├── components/      # Reusable UI components
│       └── views/           # Dashboard, detail, timeline views
├── docs/                    # Documentation
├── examples/e2e/            # E2E testing infrastructure
└── Cargo.toml               # Rust dependencies
```

## Dependencies

- **ratatui** 0.30 - Terminal UI framework
- **crossterm** 0.29 - Terminal manipulation
- **tokio** 1.x - Async runtime
- **serde/serde_json** - JSON parsing
- **clap** 4.x - CLI argument parsing
- **chrono** 0.4 - Timestamp handling
- **anyhow** 1.x - Error handling
- **reqwest** 0.13 - HTTP client (for updates)
- **tracing** 0.1 - Logging framework

## License

MIT License. See [LICENSE](../LICENSE) for details.
