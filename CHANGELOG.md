# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Headless JSON API: `cluster status --json`, `cluster incidents --json`, `cluster pods --json`, `cluster nodes --json`, and `cluster logs <pod>` with `--verbose` flag for detailed output
- MCP (Model Context Protocol) server: `cluster mcp` exposes read-only tools for AI agents (Cursor, Claude Desktop) over stdio
- JSON schema versioning (v1.0.0) for stable programmatic access
- E2E testing infrastructure with kind cluster and sample workloads in `examples/e2e/`
- `--context` flag support for JSON commands (headless mode)
- MCP tools: `get_cluster_health`, `list_incidents`, `list_pods`, `describe_pod`, `explain_pod`, `get_logs`, `list_nodes`, `list_contexts`
<<<<<<< HEAD
- Incident timeline view that correlates Kubernetes events, container restarts, OOMKilled/CrashLoopBackOff terminations, node condition changes, and deployment/replicaset rollouts in a chronological view
- Deployment/ReplicaSet rollout tracking as timeline entries (new revisions, image changes, scaling events)
- Timeline filtering by event kind, namespace, and severity
- Timeline grouping by workload and time window to identify related incidents
- Precursor highlighting (⚡) that automatically identifies events (rollouts, node condition changes) that likely caused incidents within a configurable time window (default 10 minutes)
- Keyboard shortcuts: `t` to open timeline from dashboard, `T` to open timeline filtered to selected incident from incident queue, `s` to toggle sort order, `f`/`F` to cycle filters
- Accessible severity labels in timeline (not color-only)
=======
>>>>>>> 0f9c018 (style: apply rustfmt with edition 2024 rules)

### Changed
- Upgrade Rust toolchain from 1.94.1 to 1.99.0
- Upgrade to Rust edition 2024
- Update all dependencies to latest compatible versions
- Update MSRV from 1.70 to 1.85 (required for edition 2024)
- Add rustfmt.toml, clippy.toml, and deny.toml configuration files
- Add MSRV check to CI pipeline
- Add cargo-deny security audit to CI pipeline
- Add cross-platform test job to CI (Ubuntu, macOS, Windows)
<<<<<<< HEAD

## [0.3.5] - 2026-10-13
- Add rustfmt.toml, clippy.toml, and deny.toml configuration files
- Add MSRV check to CI pipeline
- Add cargo-deny security audit to CI pipeline
- Add cross-platform test job to CI (Ubuntu, macOS, Windows)
=======
### Added
- Headless JSON API: `cluster status --json`, `cluster incidents --json`, `cluster pods --json`, `cluster nodes --json`, and `cluster logs <pod>` with `--verbose` flag for detailed output
- MCP (Model Context Protocol) server: `cluster mcp` exposes read-only tools for AI agents (Cursor, Claude Desktop) over stdio
- JSON schema versioning (v1.0.0) for stable programmatic access
- E2E testing infrastructure with kind cluster and sample workloads in `examples/e2e/`
- `--context` flag support for JSON commands (headless mode)
- MCP tools: `get_cluster_health`, `list_incidents`, `list_pods`, `describe_pod`, `explain_pod`, `get_logs`, `list_nodes`, `list_contexts`
>>>>>>> b0fd2d9 (feat: add JSON API and MCP server for AI agents)
=======
>>>>>>> 0f9c018 (style: apply rustfmt with edition 2024 rules)

## [0.3.4] - 2026-10-07

### Fixed
- Stop error logs from drawing over the TUI when kubectl fails; logging is now only enabled when `RUST_LOG` is set
- Show a clear "no context configured" message instead of raw kubectl diagnostics when no kubectl context is set, and keep the connection popup's actions visible for long errors
- Record curl-based installations explicitly so upgrades keep using the correct method from custom install directories
- Prevent stale log lines from a previous container or source appearing after a stream switch
- Surface workload kinds that could not be collected instead of silently showing incomplete data
- Recognize the standard Cluster API deployment label when it is propagated to Nodes
- Prevent a crash when the refresh interval is 0 or extremely large; intervals are now clamped to 1s–24h
- Treat completed Job/CronJob pods as healthy instead of critical, and show unready Pending pods as Warning
- Terminate timed-out `kubectl` processes instead of leaving them running
- Parse all Kubernetes memory quantity formats (plain bytes, decimal `M`/`G`, `Ti`, fractions, exponents)
- Ignore key release events so each keypress registers once on Windows
- Keep valid settings from a partial `config.toml` and warn when the file cannot be parsed
- Restore the terminal when startup fails after entering the alternate screen
- Show a warning when `kubectl top` metrics are unavailable instead of reporting 0% usage
- Stop reporting RBAC "forbidden" errors as connection failures, and report kubectl timeouts as connection issues
- Load the namespace picker even without cluster-wide pod list access
- Avoid back-to-back refreshes after a slow fetch
- Sanitize the default export filename for context names containing `:` or `/` (e.g. EKS ARNs)
- Match probe failures and incident focus to the correct pods instead of pods sharing a name prefix

### Changed
- Detect `kubectl` command timeouts more quickly by setting an explicit 30s timeout instead of relying on OS defaults
- Surface an event count from all namespaces instead of the 0–100 shown in the panel
- Shift to a simpler, faster filtering pass for namespaced workload kinds without pagination
- Handle container statuses even when `state` or nested `reason` are missing
- Schedule a follow-up fetch 5s after every 5xx response from an unreachable cluster
- Cancel long-running `kubectl` commands immediately on app exit
- Allow reading multiple node columns from `kubectl top` outputs that show pods or container breakdown
- Use plural form "restarts" for `> 1` restart counts

## [0.3.3] - 2026-09-27

### Fixed
- Fixed OOM threshold validation to allow memory_percent = 0 during tests
- Show "Low Memory" badge for OOMKilled containers

## [0.3.2] - 2026-09-26

### Fixed
- Fix test build by allowing dead code on pod_history tests
- Add dedicated pod_history test fixture
- Log invalid env variable values instead of panicking during startup

## [0.3.1] - 2026-09-25

### Fixed
- Highlight correct incident count in dashboard incident panel

## [0.3.0] - 2026-09-25

### Added
- Cluster health overview with A–F grading based on weighted penalties for incidents
- Node health panel showing memory/CPU/disk pressure, Ready state, pods-per-node, and aggregate grade
- Incident panel with ranked buckets: OOMKilled, CrashLoopBackOff, FailedScheduling, ImagePullBackOff, and grouped warnings
- Pod-level status panel with detailed running/pending/failed/succeeded breakdown
- Workload popup: drill into pods filtered by owner (Deployment, StatefulSet, DaemonSet, Job, CronJob, ReplicaSet)
- Pod detail view: Lifecycle history, container breakdown, probe history, resource usage, event correlation, log streaming
- Node detail view: full node breakdown by condition, taints, allocatable resources, and running pods
- Keyboard-driven navigation with configurable shortcuts
- Namespace filtering (cluster-wide or single namespace)
- Pod filtering by status (all, running, pending, failed, succeeded)
- Pod sorting by age, restarts, CPU, memory, name
- Namespace picker (n/N)
- Refresh controls (r for immediate, R for interval)
- Log viewer with: streaming, log follow, container switching, log-level parsing, RFC3339 parsing, export, search
- Background fetching: tokio-based concurrency for all kubectl I/O
- Configuration file support: `~/.config/cluster/config.toml` for namespace, refresh interval, node pool filter, cluster name, resource group
- Connection diagnostics: helpful error messages for kubectl missing, no context, or unavailable namespaces
- Installation tracking and upgrade check (GitHub releases)
- Fully read-only: enforces read-only kubectl whitelist at runtime

### Changed
- 30s timeout on all kubectl commands with read-only whitelist enforcement
- Minimum Rust version: 1.70

### Security
- Uses read-only kubectl commands only (get, top, logs, config)

## [0.2.0] - 2026-08-15

### Added
- Initial release with basic cluster monitoring

## [0.1.0] - 2026-07-20

### Added
- Initial prototype
