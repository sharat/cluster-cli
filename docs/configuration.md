# Configuration

Configure cluster-cli via config file, command-line arguments, or environment variables.

## Configuration File

cluster-cli looks for a configuration file at:

- **Linux/macOS**: `~/.config/cluster-cli/config.toml`
- **Windows**: `%APPDATA%\cluster-cli\config.toml`

### Example Configuration

```toml
# Kubernetes namespace to monitor
namespace = "production"

# Resource group (optional, for display/reference only)
resource_group = "prod-rg"

# Cluster name (optional, for display in header)
cluster_name = "prod-us-east-1"

# Refresh interval in seconds (1-86400)
refresh_interval_secs = 60

# Node pool filter (AKS, EKS, GKE, Karpenter, Cluster API, kOps)
node_pool_filter = "nodepool1"
```

### Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `namespace` | string | `""` | Kubernetes namespace to monitor |
| `resource_group` | string | `""` | Resource group (display only) |
| `cluster_name` | string | `""` | Cluster name (display only) |
| `refresh_interval_secs` | integer | `60` | Refresh interval (1-86400 seconds) |
| `node_pool_filter` | string | `""` | Node pool label filter |

### Namespace Resolution

When `namespace` is not set or empty, cluster-cli uses this fallback chain:
1. Command-line `--namespace` flag
2. Config file `namespace` setting
3. Current kubectl context namespace
4. `"default"` namespace

If none of these provide a valid namespace, the app shows a connection error with guidance.

### Refresh Interval Clamping

Refresh intervals are automatically clamped to safe ranges:
- **Minimum**: 1 second
- **Maximum**: 86400 seconds (24 hours)

Values outside this range are adjusted to the nearest bound.

## Command-Line Arguments

Command-line arguments override config file settings.

### TUI Mode

```bash
cluster [OPTIONS]
```

#### Options

| Flag | Short | Type | Description |
|------|-------|------|-------------|
| `--namespace` | `-n` | `<NAMESPACE>` | Kubernetes namespace to monitor |
| `--resource-group` | `-g` | `<RESOURCE_GROUP>` | Resource group (display only) |
| `--cluster` | `-c` | `<CLUSTER>` | Cluster name (display only) |
| `--refresh` | `-r` | `<SECONDS>` | Refresh interval [default: 60] |
| `--frequency` | | `<SECONDS>` | Alias for `--refresh` |
| `--node-pool-filter` | | `<FILTER>` | Node pool name to filter |
| `--check-update` | | flag | Check for newer release |
| `--upgrade` | | flag | Upgrade to latest release |
| `--info` | | flag | Show version and install info |
| `--help` | `-h` | flag | Print help |
| `--version` | `-V` | flag | Print version |

### Headless Mode

See [Headless Mode](headless.md) for JSON API subcommands.

## Node Pool Filtering

Filter nodes by node pool labels for multi-pool clusters.

### Supported Label Patterns

cluster-cli automatically detects labels from these managed Kubernetes services:

| Service | Label Pattern |
|---------|---------------|
| **AKS** | `agentpool=<name>` |
| **EKS** | `eks.amazonaws.com/nodegroup=<name>` |
| **GKE** | `cloud.google.com/gke-nodepool=<name>` |
| **Karpenter** | `karpenter.sh/nodepool=<name>` |
| **Cluster API** | `cluster.x-k8s.io/deployment-name=<name>` |
| **kOps** | `kops.k8s.io/instancegroup=<name>` |

### Usage

```bash
# Filter to specific AKS pool
cluster --node-pool-filter nodepool1

# Filter to specific EKS nodegroup
cluster --node-pool-filter prod-workers

# With config file
echo 'node_pool_filter = "nodepool1"' >> ~/.config/cluster-cli/config.toml
```

Only nodes matching the specified pool are shown in the dashboard and included in health calculations.

## Environment Variables

### Logging

cluster-cli uses the `RUST_LOG` environment variable for log level control:

```bash
# Enable debug logs (stderr)
RUST_LOG=debug cluster 2>cluster.log

# Enable trace logs
RUST_LOG=trace cluster 2>cluster.log

# Filter by module
RUST_LOG=cluster_cli::data=debug cluster 2>cluster.log
```

**Important**: Logs go to stderr. Since stderr shares the terminal with the TUI, redirect it to a file.

### No Other Environment Variables

cluster-cli does not read any other environment variables. It relies on:
- kubectl configuration (via `~/.kube/config` or `KUBECONFIG`)
- Its own config file
- Command-line arguments

## kubectl Context

cluster-cli uses your current kubectl context. To switch contexts:

```bash
# List contexts
kubectl config get-contexts

# Switch context
kubectl config use-context <context-name>

# Verify current context
kubectl config current-context
```

cluster-cli does not provide built-in context switching. Use kubectl or the [MCP Server](mcp.md) `list_contexts` tool for context management.

## Upgrade Management

### Installation Method Detection

cluster-cli remembers how it was installed:
- **curl**: Installed via install.sh script
- **homebrew**: Installed via Homebrew
- **cargo**: Installed via `cargo install`

### Checking for Updates

```bash
cluster --check-update
```

Queries the GitHub releases API and reports:
- Current version
- Latest available version
- Whether an update is available

### Upgrading

```bash
cluster --upgrade
```

Automatically upgrades using the detected installation method:
- **curl**: Re-runs the installer from the immutable release tag
- **homebrew**: Runs `brew upgrade cluster-cli`
- **cargo**: Runs `cargo install --force cluster-cli`

The curl-based upgrade fetches the installer script from the currently running binary's release tag and verifies it against the bundled copy before execution.

### Installation Info

```bash
cluster --info
```

Shows:
- Version number
- Installation method
- Config directory location

## Partial Configuration Files

cluster-cli tolerates partial or malformed configuration files:
- Valid fields are loaded
- Invalid fields trigger warnings (if logging enabled)
- Missing fields use defaults
- Parsing errors for the entire file are logged but don't prevent startup

This allows incremental configuration and prevents lockout due to typos.

## Configuration Priority

From highest to lowest priority:

1. **Command-line flags** (highest)
2. **Config file settings**
3. **kubectl context** (namespace only)
4. **Built-in defaults** (lowest)

For example, if you set `namespace = "staging"` in the config file but run `cluster -n production`, the production namespace is used.

## Security Considerations

### No Credentials Stored

cluster-cli does not store, transmit, or log:
- kubectl credentials
- API tokens
- Certificates
- Cloud provider credentials

It relies entirely on kubectl's existing configuration.

### Read-Only Operation

All kubectl commands are whitelisted to read-only operations:
- `get` (nodes, pods, events, namespaces, workloads)
- `top` (metrics)
- `logs` (container logs)
- `config` (context queries)

Mutation verbs are blocked at runtime.

### Config File Permissions

Recommended permissions for the config file:

```bash
chmod 600 ~/.config/cluster-cli/config.toml
```

Although the config file contains no secrets, restricting permissions follows the principle of least privilege.

## Examples

### Multi-Cluster Setup

For multiple clusters, create shell aliases:

```bash
# ~/.bashrc or ~/.zshrc
alias cluster-dev='kubectl config use-context dev-cluster && cluster -n development'
alias cluster-staging='kubectl config use-context staging-cluster && cluster -n staging'
alias cluster-prod='kubectl config use-context prod-cluster && cluster -n production -c prod-us-east-1'
```

### Team Shared Configuration

For team standardization, commit a sample config to your repository:

```toml
# .cluster-cli.toml (example config)
namespace = "default"
refresh_interval_secs = 120
cluster_name = "shared-dev-cluster"
```

Team members copy it:
```bash
cp .cluster-cli.toml ~/.config/cluster-cli/config.toml
```

### CI/CD Integration

Use command-line flags in CI pipelines:

```bash
# Check production cluster health
cluster status --json -n production > health.json

# Export pod list for analysis
cluster pods --json -n production --verbose > pods.json
```

See [Headless Mode](headless.md) for complete JSON API documentation.

## Troubleshooting

### Config File Not Found

If the config file doesn't exist, cluster-cli uses defaults. No error is shown unless logging is enabled:

```bash
RUST_LOG=debug cluster 2>cluster.log
# Check cluster.log for "config file not found" message
```

### Config Parse Errors

If the config file has syntax errors:

```bash
RUST_LOG=debug cluster 2>cluster.log
# Check cluster.log for "failed to parse config" message
```

The app still starts with defaults for invalid fields.

### Config Not Applied

Verify the config file location:

```bash
# Linux/macOS
cat ~/.config/cluster-cli/config.toml

# Check with --info
cluster --info
```

Command-line flags override config file settings. Ensure you're not passing conflicting flags.

## Next Steps

- [User Interface Guide](ui.md) - Navigate the TUI
- [Headless Mode](headless.md) - Automate with JSON API
- [Quick Start](quick-start.md) - Common workflows
