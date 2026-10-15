# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Experimental GPUI desktop app (`just desktop`) that watches every kubeconfig context at once, with a fleet overview, cluster detail, pod detail with live logs, namespace picker, and desktop notifications for new incidents and grade drops; supports an all-namespaces view and large fleets
- Stream pod, node, and event changes between refreshes with read-only `kubectl get --watch`, so crash loops and new pods appear within a second instead of at the next refresh; disable with `--no-watch` or `watch = false`
- Add cluster-wide health checks to the incident queue and score: unavailable APIServices, failed PersistentVolumes, namespaces stuck terminating, and admission webhooks that cannot be reached
- Add `crd_checks` to flag custom resources (e.g. cert-manager Certificates, Flux Kustomizations, Argo CD Applications) whose Ready/Available/Healthy/Synced condition has been `False` for over 5 minutes or whose Argo health is Degraded/Missing

### Changed
- Scale the health score to cluster size: penalties now depend on the share of pods and nodes affected, so large clusters are no longer stuck at F by a handful of failures
- Stop counting completed Job/CronJob pods and evicted pods as unhealthy, in the score and in pod status
- Split the kubectl collector, health scoring, incidents, config and fetcher into a shared `cluster-core` workspace crate; the TUI is unchanged

### Fixed
- Show each workload collection warning once in the error banner instead of repeating the same line from the poll and the snapshot
- Run `kubectl get --watch` through the shared kubectl setup so user kuberc preferences stay disabled and live streams count toward the process cap

## [0.4.0] - 2026-10-14

### Added
- Headless JSON API: `cluster status --json`, `cluster incidents --json`, `cluster pods --json`, `cluster nodes --json`, and `cluster logs <pod>` with `--verbose` flag for detailed output
- MCP (Model Context Protocol) server: `cluster mcp` exposes read-only tools for AI agents (Cursor, Claude Desktop) over stdio
- JSON schema versioning (v1.0.0) for stable programmatic access
- E2E testing infrastructure with kind cluster and sample workloads in `examples/e2e/`
- `--context` flag support for JSON commands (headless mode)
- MCP tools: `get_cluster_health`, `list_incidents`, `list_pods`, `describe_pod`, `explain_pod`, `get_logs`, `list_nodes`, `list_contexts`
- Incident timeline view that correlates Kubernetes events, container restarts, OOMKilled/CrashLoopBackOff terminations, node condition changes, and deployment/replicaset rollouts in a chronological view
- Deployment/ReplicaSet rollout tracking as timeline entries (new revisions, image changes, scaling events)
- Timeline filtering by event kind, namespace, and severity
- Timeline grouping by workload and time window to identify related incidents
- Precursor highlighting (⚡) that automatically identifies events (rollouts, node condition changes) that likely caused incidents within a configurable time window (default 10 minutes)
- Keyboard shortcuts: `t` to open timeline from dashboard, `T` to open timeline filtered to selected incident from incident queue, `s` to toggle sort order, `f`/`F` to cycle filters
- Accessible severity labels in timeline (not color-only)
- Diagnosis panel in pod detail that explains why a pod is unhealthy, with a likely cause and the next read-only `kubectl` command for each finding (crash loops with exit-code hints, OOM kills, image pull and config errors, scheduling and volume failures, evictions, failing probes, node problems, and resources near their limits)

- Native sidecar support: sidecars (init containers with `restartPolicy: Always`) are listed in pod detail, selectable in the log viewer, and counted in restarts, readiness, crash-loop and OOM detection, so a crash-looping sidecar marks its pod critical

- Documentation site at https://sharat.github.io/cluster-cli/ with a landing page and guides for the UI, health scoring, configuration, the headless JSON API, the MCP server, and the incident timeline

- Right-sizing view (`u`): compares each workload's CPU and memory requests with its peak usage this session, flags under- and over-provisioned workloads and those without requests, suggests per-pod requests with 30% headroom, and totals the capacity that could be freed

### Changed
- MCP `explain_pod` uses the same diagnosis as the pod detail view and now returns structured `findings` with severity, likely cause, and next command
- Upgrade Rust toolchain from 1.94.1 to 1.99.0
- Upgrade to Rust edition 2024
- Update all dependencies to latest compatible versions
- Update MSRV from 1.70 to 1.85 (required for edition 2024)
- Add rustfmt.toml, clippy.toml, and deny.toml configuration files
- Add MSRV check to CI pipeline
- Add cargo-deny security audit to CI pipeline
- Add cross-platform test job to CI (Ubuntu, macOS, Windows)

### Fixed
- `cluster logs -n <namespace>` selects the namespace; `-n` was claimed by both `--namespace` and `--tail` (use `--tail` for the line count)
- MCP `list_contexts` works; the read-only kubectl whitelist rejected the `kubectl config get-contexts` it runs
- `install.sh` honors `INSTALL_DIR` from the environment instead of always installing to `~/.local/bin`
- Keep pod detail container columns separated: long container names and states such as `Waiting (CrashLoopBackOff)` no longer run into the next column
- Keep the logs panel usable in pod detail on short terminals (under 33 rows) by compacting the overview and events panels
- Count native sidecars in pod CPU and memory requests and limits the way Kubernetes does (added to the app containers and to later init containers) instead of taking the largest init container
- Run kubectl with `KUBECTL_KUBERC=false` so a user's kubectl preferences (kuberc) cannot change the output cluster-cli parses
- Show events written through the newer events API (such as the scheduler's FailedScheduling) in the events panel, incidents, and diagnosis; they have no `lastTimestamp`, so sorting by it dropped them once a namespace had more than 50 events
- Keep Warning events ahead of Normal ones when trimming to the 50 most recent, so a busy namespace's routine events (Pulled, Created, Started) no longer push an older warning such as FailedScheduling out of the events panel, incidents, and diagnosis

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
