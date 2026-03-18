# cluster-rs

[![CI](https://github.com/sharat/cluster-rs/workflows/CI/badge.svg)](https://github.com/sharat/cluster-rs/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A fast, interactive terminal UI for monitoring Kubernetes cluster health in real-time. Works with any Kubernetes cluster (AKS, EKS, GKE, on-premises, etc.).

## Features

For the current detailed feature inventory, see [features.md](features.md).

- **Real-time Dashboard**: Monitor nodes, incidents, and pods in a unified TUI interface
- **Node Monitoring**: View CPU/memory usage, conditions, and health status for all nodes
- **Node Details**: Deep-dive into node specifications including:
  - Hardware specs (CPU capacity, memory capacity)
  - System information (kernel version, OS image, container runtime, kubelet version)
  - Real-time resource usage with visual indicators
- **Pod Insights**: Track pod status, resource consumption, restarts, and age
- **Workload Popup**: Inspect deployment, statefulset, and daemonset rollout health with `w`
- **Pod History**: Visual CPU and memory sparklines in pod detail view
- **Memory Usage Bars**: Horizontal bar graphs with percentage and absolute values
- **Incident Queue**: Monitor ranked warning and failure signals built from nodes, pods, and events
- **Incident Drill-Down**: Press `Enter` on an incident to jump to the related pod, node, or workload when possible
- **Pod Details**: Deep-dive into individual pods with:
  - Overview (metadata, resource limits/requests)
  - Per-container status, restarts, last termination reason, and exit code
  - Readiness/liveness probe failure counts when they can be inferred from pod events
  - Event timeline
  - Live log streaming
- **Pod Prioritization**: Cycle pod sort modes with `s` to surface restarts, CPU pressure, memory pressure, readiness, or latest incident activity first
- **CSV Export**: Export the current pod list to CSV with `E`
- **Health Scoring**: Aggregate cluster health score weighted by pod readiness and phase, failed scheduling, node conditions, crash loops, OOM kills, warning volume, and rollout failures
- **Namespace Management**: Interactive namespace selector with color-coded current namespace
- **Cluster Identification**: Connected cluster name highlighted in header
- **Read-Only by Design**: Only read-only `kubectl` commands are allowed; the app does not persist local state
- **Filtering**: Search pods by name and switch namespaces interactively
- **Configurable**: Adjust refresh rates, namespaces, and node pool filters

## Prerequisites

- Rust 1.70+ (for building from source)
- `kubectl` configured with cluster access (any Kubernetes cluster)
- Terminal with truecolor support (recommended)

## Installation

### Quick Install (Recommended)

Install the latest release directly using curl:

```bash
curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | bash
```

Or install to a custom directory:

```bash
curl -fsSL https://raw.githubusercontent.com/sharat/cluster-cli/main/install.sh | INSTALL_DIR=$HOME/.local/bin bash
```

### Build from Source

```bash
git clone https://github.com/sharat/cluster-rs.git
cd cluster-rs
cargo build --release
```

The binary will be available at `target/release/cluster`.

## Usage

### Basic Usage

```bash
cluster
```

Launches the TUI with the configured namespace. If no namespace is configured, it falls back to the current `kubectl` context namespace, then to `default`.

If `kubectl` is missing, the context is unavailable, or the configured namespace cannot be loaded on startup, the app shows a blocking connection-required state with retry guidance instead of rendering an empty dashboard.

### Command Line Options

```bash
cluster [OPTIONS]

Options:
  -n, --namespace <NAMESPACE>              Kubernetes namespace to monitor
  -g, --resource-group <RESOURCE_GROUP>    Resource group (optional, for reference)
  -c, --cluster <CLUSTER>                  Cluster name (optional, for display)
  -r, --refresh <REFRESH>                  Refresh interval in seconds [default: 60] [aliases: --frequency]
      --node-pool-filter <NODE_POOL_FILTER>
                                            Node pool name filter (e.g. "nodepool1")
  -h, --help                               Print help
```

### Examples

```bash
# Monitor specific namespace
cluster -n my-namespace

# Monitor with custom refresh interval (60 seconds)
cluster --refresh 60

# Same option, alternate name
cluster --frequency 120

# Filter specific node pool
cluster --node-pool-filter nodepool1

# Set cluster name for reference
cluster -n my-namespace -c my-cluster --refresh 300

# Works with any Kubernetes cluster (AKS, EKS, GKE, minikube, etc.)
# Just ensure kubectl is configured with the correct context
```

### Trying it Out

Don't have a Kubernetes cluster with workloads yet? You can try `cluster-cli` with example applications from the [Kubernetes Examples repository](https://github.com/kubernetes/examples). Deploy any example (like the guestbook or nginx apps) to your cluster, then run `cluster-cli` to see the dashboard in action.

## Configuration

### Configuration File

The application looks for a config file at:

- Linux/macOS: `~/.config/cluster/config.toml`
- Windows: `%APPDATA%\cluster\config.toml`

Example `config.toml`:

```toml
namespace = "default"
resource_group = "my-resource-group"
cluster_name = "my-cluster"
refresh_interval_secs = 60
node_pool_filter = "nodepool1"
```

Command-line arguments override config file settings.

## Keyboard Shortcuts

### Navigation

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit |
| `1` / `2` / `3` | Focus Nodes / Incidents / Pods panel |
| `Tab` | Cycle between panels (Nodes → Incidents → Pods) |
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `Enter` | View selected item details (Pods or Nodes), or drill into the selected incident |
| `w` | Toggle workload popup |
| `s` | Cycle pod sort mode |
| `E` | Export pods to CSV |

### Pod Detail View

| Key | Action |
|-----|--------|
| `Tab` | Cycle sections (Overview → Events → Logs) |
| `f` | Toggle log follow mode |
| `Esc` / `q` | Return to dashboard |

### Node Detail View

| Key | Action |
|-----|--------|
| `↑` / `k` | Scroll up |
| `↓` / `j` | Scroll down |
| `Esc` / `q` | Return to dashboard |

### Workload Popup

| Key | Action |
|-----|--------|
| `w` | Open or close popup |
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `Esc` / `q` | Close popup |

### Pod Filtering

| Key | Action |
|-----|--------|
| `/` | Open pod name filter |
| `Esc` / `Enter` | Close filter input |

### Pod Sorting

| Key | Action |
|-----|--------|
| `s` | Cycle pod sort modes: default, restarts, CPU, memory, not-ready, latest incident |

### Incident Drill-Down

| Key | Action |
|-----|--------|
| `Enter` | Open the best matching pod, node, or workload for the selected incident |

### Namespace Switching

| Key | Action |
|-----|--------|
| `n` | Open namespace list selector |
| `N` | Open namespace input (manual entry) |
| `Enter` | Apply new namespace (when selector/input active) |
| `Esc` | Cancel namespace selection |

### Refresh

| Key | Action |
|-----|--------|
| `r` | Manual refresh |
| `R` | Set refresh interval |

### Export

| Key | Action |
|-----|--------|
| `E` | Export the current pod list to CSV using a suggested filename |

## Project Structure

```
cluster-rs/
├── src/
│   ├── main.rs              # Entry point, event loop
│   ├── app.rs               # Application state management
│   ├── config.rs            # Configuration handling
│   ├── data/
│   │   ├── collector.rs     # kubectl data collection
│   │   ├── fetcher.rs       # Async data fetching
│   │   └── models.rs        # Data structures
│   ├── events/
│   │   ├── handler.rs       # Input/keyboard handling
│   │   └── mod.rs           # Event definitions
│   └── ui/
│       ├── mod.rs           # UI rendering
│       ├── theme.rs         # Colors and styling
│       ├── components/      # Reusable UI components
│       └── views/           # Dashboard and detail views
├── cluster-health.sh        # Standalone health check script
└── Cargo.toml               # Rust dependencies
```

## Data Visualization

### Pod History Sparklines

Pod detail displays historical CPU and memory sparklines:
- Last 20 samples displayed (oldest → newest)
- Built from recent refresh history retained in memory
- Useful for quick pressure trend inspection without leaving the TUI

### Memory Usage Bars

Pod and node resource usage are shown with compact horizontal bars:
- Gradient bar plus percentage
- Pod rows show `usage / request / limit`
- The pressure bar reflects the higher of request saturation or limit saturation

### Health Score

Single-line health bar showing:
- Cluster health score (0-100) with grade (A-F)
- Number of critical nodes and pods
- Total restarts count
- Weighted by readiness, scheduling failures, node conditions, crash loops, OOM kills, warning volume, and rollout failures
- Color-coded based on severity

### Workload Popup

Popup workload view includes:
- Deployments, StatefulSets, and DaemonSets
- Desired versus ready replica counts
- Availability and rollout status
- Recent workload-related events

### Namespace Selection

Interactive list showing:
- Current namespace highlighted in **magenta** with `●` indicator
- Selected item in **yellow** (bold)
- Other namespaces in gray
- Sorted alphabetically for easy navigation

## Architecture

The application follows an async architecture using Tokio:

1. **Main Thread**: Handles terminal rendering and user input
2. **Fetcher Task**: Background task that executes kubectl commands
3. **Event Channels**: Unbounded channels for communication between tasks

Data flows:
- User input → Event handler → State updates → UI render
- Periodic refresh → kubectl commands → Data parsing → State update

## Logging

Logs are emitted to stderr only when enabled with `RUST_LOG`. The app does not create log files or write local runtime state.

## Development

### Getting Started

Clone the repository and set up the development environment:

```bash
git clone https://github.com/sharat/cluster-cli.git
cd cluster-cli

# Set up git hooks (optional but recommended)
./setup-hooks.sh
```

**Note**: Git hooks are not automatically installed when cloning (Git security feature). Run `./setup-hooks.sh` to enable pre-commit checks for formatting and linting.

### Build

```bash
cargo build
```

### Run

```bash
cargo run
```

### Run with Logs

```bash
RUST_LOG=debug cargo run
```

### Check Code

```bash
cargo clippy
cargo fmt --check
```

## Dependencies

- **ratatui**: Terminal UI framework
- **crossterm**: Terminal manipulation
- **tokio**: Async runtime
- **serde/serde_json**: JSON parsing
- **clap**: CLI argument parsing
- **anyhow**: Error handling
- **tracing**: Logging framework

## Troubleshooting

### "kubectl: command not found"

Ensure `kubectl` is installed and in your PATH.

If the app is showing the blocking connection-required state, install `kubectl`, verify it is on your `PATH`, and press `r` to retry.

### No Active Context or Namespace

If the app starts without a usable kubeconfig context or namespace, connect to a cluster first and retry:
- `kubectl config current-context`
- `kubectl config use-context <context>`
- `kubectl config view --minify --output 'jsonpath={..namespace}'`
- Press `n` to choose a namespace, `N` to enter one manually, or `r` to retry after fixing the context

### Permission Errors

Verify your kubeconfig has appropriate RBAC permissions:
- `kubectl get nodes`
- `kubectl get pods -n <namespace>`
- `kubectl top nodes`
- `kubectl top pods -n <namespace>`
- `kubectl get namespaces`
- `kubectl get nodes -o json`
- `kubectl get pods -n <namespace> -o json`
- `kubectl get events -n <namespace>`

### No Data Appears

1. Check your current context: `kubectl config current-context`
2. Check your current namespace: `kubectl config view --minify --output 'jsonpath={..namespace}'`
3. Verify pods exist: `kubectl get pods -n <namespace>`
4. Verify nodes exist: `kubectl get nodes`
5. Re-run with `RUST_LOG=debug cargo run` to inspect stderr diagnostics

If the dashboard is blocked by connection/setup errors, fix the underlying `kubectl` issue first and then press `r` to retry from inside the app.

## Kubernetes Compatibility

cluster is designed to work with any Kubernetes cluster:
- **Cloud Providers**: AKS (Azure), EKS (AWS), GKE (GCP), OKE (Oracle), etc.
- **On-premises**: Any self-managed Kubernetes cluster
- **Local Development**: minikube, kind, k3s, microk8s, etc.

The only requirement is a properly configured `kubectl` context with appropriate permissions.

## Disclaimer

**This software is provided "as is", without warranty of any kind.**  
Use at your own risk. This tool may contain bugs or inaccuracies. Always verify critical information using official Kubernetes tools (`kubectl`, dashboard, etc.). The authors are not responsible for any issues arising from the use of this software.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. For major changes, please open an issue first to discuss what you would like to change.
