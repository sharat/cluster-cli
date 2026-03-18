# cluster-rs Feature Inventory

This document is the single feature reference for `cluster-rs` as implemented in the current codebase.

## Product Summary

`cluster-rs` is a read-only terminal UI for monitoring Kubernetes cluster health using `kubectl`. It renders a live dashboard, supports drill-down views for pods and nodes, and surfaces health, resource, rollout, and incident signals without mutating cluster state.

## Core Dashboard

### Main layout

The dashboard includes:

- A header with app name, active cluster/context label, namespace, local time, and refresh status
- A health and namespace resource summary strip
- A node panel
- An incident queue panel
- A pod table
- A status/help bar

### Background refresh

- Data refresh runs continuously in the background
- Refreshes are aligned to clock boundaries instead of drifting over time
- Manual refresh is supported with `r`
- Refresh interval can be changed at runtime with `R`

### Read-only operation

The app only allows read-only `kubectl` commands:

- `kubectl get ...`
- `kubectl top ...`
- `kubectl logs ...`
- `kubectl config current-context`
- `kubectl config view`

The app does not apply, patch, delete, or persist cluster changes.

## Cluster and Context

### Namespace handling

- Uses the configured namespace when provided
- Falls back to the current kubectl context namespace when unset
- Falls back to `default` if no namespace is available from context
- Supports interactive namespace selection from a fetched namespace list
- Supports direct manual namespace entry
- Preserves the current snapshot on namespace switch until refreshed data arrives

### Cluster identification

- Shows the configured cluster name when supplied
- Otherwise shows the active kubectl context name from the current kubeconfig context

### Error surfacing

- Fetch errors are promoted to short-lived status messages in the footer
- If the app cannot establish a usable `kubectl` connection on startup, it shows a blocking connection-required empty state instead of a blank dashboard
- The blocking state is used for missing `kubectl`, missing kubeconfig context, or an unavailable configured namespace
- Previous events are retained when an event refresh fails, avoiding an empty incident view during transient failures

## Health and Resource Summary

### Cluster health score

The dashboard renders:

- A 0-100 health score
- A letter grade
- Critical node count
- Critical pod count
- Total restart count
- A colorized health bar

### Namespace resource summary

The summary strip aggregates pod-level resource data for the active namespace:

- CPU usage, request total, and limit total
- Memory usage, request total, and limit total
- Percentage of usage versus requests and limits
- Pod count

## Node Features

### Node list

The dashboard node panel shows:

- Node name
- CPU percentage
- Memory percentage
- A compact memory usage bar
- A health icon derived from node severity

### Node detail popup

Pressing `Enter` on a selected node opens a popup with:

- CPU usage and allocatable CPU
- Memory usage and allocatable memory
- Overall node health status
- Explicit Kubernetes condition states
- Scheduling state
- Kernel version
- OS image
- Container runtime version
- Kubelet version
- Architecture
- Operating system

### Node conditions and scheduling

Node detail explicitly shows:

- `Ready`
- `MemoryPressure`
- `DiskPressure`
- `PIDPressure`
- `NetworkUnavailable`

Each condition is rendered with a derived state label and the raw Kubernetes boolean or unknown value.

Scheduling is shown as:

- `Schedulable` when the node is available for scheduling
- `Cordoned` when the node is unschedulable

The UI labels the unschedulable case as `cordoned/draining` because this code path does not distinguish a plain cordon from an active drain operation.

## Incident and Event Features

### Incident queue

The dashboard shows a ranked incident queue built from nodes, pods, and warning events rather than rendering raw events directly.

Each incident bucket includes:

- Severity
- Occurrence count
- Number of affected resources
- A normalized reason
- A compact target hint or sample message

### Incident drill-down

Pressing `Enter` on an incident opens the best matching resource when one can be resolved:

- A pod opens pod detail and starts pod log streaming
- A node opens node detail
- A workload opens the workload popup focused on that workload
- Multi-resource incidents fall back to pod-focused navigation so the operator can continue investigation from the most relevant table

### Incident severity levels

Incident buckets are classified into:

- `Critical`
- `Warning`
- `Elevated`

### Event ingestion

The app fetches Kubernetes events for the active namespace and uses them to:

- Populate the incident queue
- Enrich health scoring
- Power pod event timelines
- Infer probe failures in pod detail
- Attach recent events to workload summaries

## Pod Features

### Pod table

The dashboard pod table shows:

- Pod phase/status
- Pod name
- CPU `usage / request / limit`
- Memory `usage / request / limit`
- A pressure bar derived from current request-or-limit saturation
- Restart count
- Age

Pod resource math is based on summed container requests and limits across the pod, not just the first container.

### Pod prioritization

The pod list can be re-ordered interactively with `s`.

Available sort modes:

- Default snapshot order
- Restart count
- CPU pressure
- Memory pressure
- Not-ready pods first
- Latest matching incident activity first

Sorting happens in the UI layer after filtering, and selection resets to the top when the sort mode changes.

### Pod filtering

- Filter pods by name with `/`
- Filter is applied live as the user types
- Cursor resets when the filter changes
- Filter can be dismissed with `Esc` or `Enter`

### CSV export

- Press `E` from the dashboard to export the current pod list to CSV
- The app prompts for a filename and suggests a timestamped name by default
- The export uses the active namespace and current cluster/context label when available

### Pod detail view

Pressing `Enter` on a selected pod opens a dedicated detail screen with three sections:

- Overview
- Events
- Logs

### Pod overview

The overview section includes:

- Pod phase
- Node placement
- Age
- Restart count
- CPU usage versus requests and limits
- CPU history sparkline
- Memory usage versus requests and limits
- Memory history sparkline
- Per-container status rows

### Per-container diagnostics

Container rows include:

- Container name
- Ready or not-ready status
- Current state
- Restart count
- Last termination reason
- Last exit code
- Inferred readiness probe failure count
- Inferred liveness probe failure count

### Pod event timeline

The events section shows pod-related events with:

- Timestamp
- Reason
- Message
- Warning highlighting

### Live log streaming

When a pod detail view is opened, the app starts a live `kubectl logs -f` stream for that pod.

The log viewer supports:

- Follow mode
- Manual scroll
- Log-level-aware coloring
- A rolling in-memory buffer capped at 1000 lines

## Workload Features

### Workload summaries

The data layer fetches workload summaries for:

- Deployments
- StatefulSets
- DaemonSets

These summaries include rollout and availability data and are attached to the in-memory snapshot.

### Workload popup

Workloads are accessed from the dashboard through a popup opened with `w`, rather than a dedicated home-page panel.

The popup includes:

- A navigable workload list
- Desired versus ready replica counts
- Available replica and unavailable pod tracking
- Rollout status summaries for steady state, in-progress rollouts, scale-to-zero, and rollout deadline failures
- Recent rollout-related events attached to each workload summary

Deployment event correlation includes ReplicaSet-owned rollout events when they are the most recent signal.

## Visualization

### Usage bars

The UI includes compact gradient bars for:

- Node CPU and memory usage
- Pod CPU and memory usage
- Health score

### Sparklines

Historical pod CPU and memory usage are rendered as compact sparklines in the pod detail overview using recent samples retained in memory.

### Color semantics

The interface uses consistent color cues for:

- Health severity
- Resource pressure
- Incident severity
- Warning events
- Focused and selected panels

## Keyboard and Interaction

### Dashboard navigation

- `Tab` cycles focused panel
- `1` focuses Nodes
- `2` focuses Incident Queue
- `3` focuses Pods
- `j` / `Down` moves selection down
- `k` / `Up` moves selection up
- `Enter` drills into the selected pod or node, or opens the best matching incident target
- `w` toggles the workload popup
- `s` cycles pod sort modes
- `E` opens CSV export
- `q` or `Ctrl+C` quits

### Namespace controls

- `n` opens namespace selector
- `N` opens manual namespace input
- `Enter` applies the selected or entered namespace
- `Esc` cancels namespace overlays

### Refresh controls

- `r` triggers a manual refresh
- `R` opens refresh interval input

### Connection recovery

- If the app starts in the blocking connection-required state, fix the `kubectl` setup and press `r`
- For missing kubeconfig context, select a context with `kubectl config use-context ...`
- For a missing or wrong namespace, use `n` or `N` inside the app and then retry

### Pod detail controls

- `Tab` cycles Overview, Events, and Logs
- `j` / `k` scroll within the detail screen
- `f` toggles log follow mode
- `Esc` or `q` returns to the dashboard

### Node detail controls

- `j` / `k` scroll the detail popup
- `Esc` or `q` returns to the dashboard

### Workload popup controls

- `j` / `k` and arrow keys move through workloads
- `w`, `Esc`, or `q` closes the popup

## Data Collection

### Kubernetes objects collected

The current implementation collects:

- Nodes
- Pods
- Events
- Namespaces
- Current context name
- Workload summaries

It also derives additional signals from pod and node payloads, such as:

- Node condition summaries
- Incident buckets
- Namespace resource totals
- Probe failure counters inferred from events

## Runtime Behavior

### Terminal behavior

- Runs in the alternate screen
- Restores terminal state on panic
- Uses a visible wait cursor while initially connecting to the cluster

### Logging

- Application logs go to stderr when enabled with `RUST_LOG`
- No runtime log files are created by the app

## Current Scope Notes

The items below exist in configuration or state but should not be treated as fully surfaced user features yet:

- `node_pool_filter` is accepted in config and CLI, but is not currently reflected in the visible dashboard behavior in this code path
