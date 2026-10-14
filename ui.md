# User Interface Guide

Complete reference for cluster-cli's terminal user interface, views, and keyboard shortcuts.

## Dashboard View

The main dashboard shows three panels:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Cluster: prod-cluster    Namespace: production    Health: 85 (B)   [●●●○○] │
├────────────────────┬──────────────────────┬────────────────────────────────┤
│ Nodes              │ Incidents            │ Pods                            │
│                    │                      │                                 │
│ node-1  Ready 85%  │ CrashLoopBackOff (3) │ api-6d4f9   Running  ↑         │
│ node-2  Ready 72%  │ FailedScheduling (2) │ worker-0    Running            │
│ node-3  Ready 91%  │ ImagePullBackOff (1) │ db-0        Running            │
│                    │                      │ nginx-abc   Running            │
└────────────────────┴──────────────────────┴────────────────────────────────┘
│ 1: Nodes  2: Incidents  3: Pods  Tab: Cycle  Enter: Details  q: Quit       │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Header Bar

The top bar displays:
- **Cluster name**: Current kubectl context
- **Namespace**: Active namespace (color-coded)
- **Health score**: 0-100 with letter grade (A-F)
- **Health bar**: Visual indicator with gradient coloring

### Panels

#### Nodes Panel (Focus: `1`)

Shows node health and resource usage:
- Node name
- Ready status
- CPU percentage (of capacity)
- Memory percentage (of capacity)
- Visual pressure indicators

#### Incidents Panel (Focus: `2`)

Ranked incidents by severity and occurrence count:
- Incident reason (e.g., CrashLoopBackOff, FailedScheduling)
- Occurrence count
- Severity (Critical, Warning, Elevated)
- Affected resources

#### Pods Panel (Focus: `3`)

Pod list with real-time status:
- Pod name
- Phase (Running, Pending, Failed, Succeeded, Unknown)
- Restart count
- Ready indicator
- CPU/memory sparklines (in detail view)
- Age

### Status Bar

Bottom bar shows:
- Current focus indicator
- Available actions
- Key hints

## Dashboard Keyboard Shortcuts

### Navigation

| Key | Action |
|-----|--------|
| `q` or `Ctrl+C` | Quit application |
| `1` | Focus Nodes panel |
| `2` | Focus Incidents panel |
| `3` | Focus Pods panel |
| `Tab` | Cycle focus (Nodes → Incidents → Pods) |
| `↑` or `k` | Move cursor up |
| `↓` or `j` | Move down |
| `Enter` | View selected item details or drill into incident |

### Views

| Key | Action |
|-----|--------|
| `t` | Open incident timeline |
| `T` | Open timeline filtered to selected incident (Incidents panel focused) |
| `u` | Open right-sizing view |
| `w` | Toggle workload popup |
| `Esc` | Clear the incident focus set by `Enter` on an incident |
| `?` | Help: every key and marker (works in every view) |

### Namespace Management

| Key | Action |
|-----|--------|
| `n` | Open namespace selector (shows pod counts) |
| `N` | Open namespace input (manual entry) |

Inside namespace selector/input:
- `↑`/`k`, `↓`/`j` - Navigate (selector only)
- `Enter` - Apply selection
- `Esc` - Cancel

### Pod Operations

| Key | Action |
|-----|--------|
| `s` | Cycle sort mode (default, restarts, CPU, memory, not-ready, incident) |
| `/` | Open pod name filter input |
| `E` | Export current pod list to CSV |

Inside filter input:
- Type to filter
- `Enter` or `Esc` - Close filter

### Refresh

| Key | Action |
|-----|--------|
| `r` | Manual refresh |
| `R` | Open refresh interval input |

Inside refresh input:
- Type interval in seconds (1-86400)
- `Enter` - Apply
- `Esc` - Cancel

### Workload Popup

Press `w` to open. Shows:
- Deployments, StatefulSets, DaemonSets
- Jobs, CronJobs
- HorizontalPodAutoscalers
- PodDisruptionBudgets
- Services, Ingresses
- PersistentVolumeClaims

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Navigate list |
| `w`, `q`, or `Esc` | Close popup |

## Pod Detail View

Press `Enter` on a pod in the dashboard to open detailed view.

### Sections

Navigate sections with `Tab`:

1. **Overview**
   - Pod metadata (namespace, node, age, phase)
   - Resource usage and limits (CPU, memory)
   - Container breakdown: app containers, native sidecars (init containers with `restartPolicy: Always`), and regular init containers, each with its state
   - Restart counts and last termination reasons

2. **Events**
   - Pod-related Kubernetes events, including those written through the newer events API (such as the scheduler's `FailedScheduling`)
   - Timestamps and event types
   - Warning and error messages

3. **Logs**
   - Live log streaming
   - Multi-container support, including sidecars and init containers
   - Search and filtering

Between the Overview and Events panels, the **Diagnosis** panel explains why the pod is unhealthy. Each finding has a severity label, the issue, its likely cause, and the next read-only `kubectl` command to run. It covers crash loops (with exit-code hints), OOM kills, image pull and configuration errors, scheduling and volume failures, evictions, failing probes, node problems, and resources near their limits, and names the container responsible, including a sidecar. A healthy pod shows "No problems detected".

On short terminals (under 33 rows), the Overview and Events panels shrink so the Logs panel keeps room for log lines.

### Pod Detail Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Tab` | Cycle sections (Overview → Events → Logs) |
| `↑`/`k`, `↓`/`j` | Scroll the focused section |
| `Esc` or `q` | Return to dashboard |

#### Logs Section

| Key | Action |
|-----|--------|
| `f` | Toggle follow mode (auto-scroll) |
| `c` | Cycle containers, including sidecars and init containers |
| `p` | Toggle previous logs (terminated container) |
| `/` | Search buffered logs |
| `w` | Toggle line wrapping |
| `t` | Toggle Kubernetes timestamps |
| `E` | Export logs to file |
| `↑`/`k`, `↓`/`j` | Scroll (when not following) |

## Node Detail View

Press `Enter` on a node in the dashboard to open detailed view.

Shows:
- **Hardware Specifications**
  - CPU capacity (cores)
  - Memory capacity (GB)
  - Allocatable resources

- **System Information**
  - Kernel version
  - OS image
  - Container runtime version
  - Kubelet version
  - Architecture

- **Resource Usage**
  - Current CPU usage (millicores and %)
  - Current memory usage (MB and %)
  - Visual pressure bars

- **Conditions**
  - Ready status
  - MemoryPressure
  - DiskPressure
  - PIDPressure
  - NetworkUnavailable

- **Pod List**
  - Pods running on this node

### Node Detail Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Scroll view |
| `Esc` or `q` | Return to dashboard |

## Right-Sizing View

Press `u` from the dashboard to compare what each workload requests with what it uses.

```
┌ Summary ─────────────────────────────────────────────────────────────────────┐
│  Requested  CPU 500m  Mem 480Mi    Using  CPU 225m (45%)  Mem 35Mi (7%)      │
│  Reclaimable by following the suggestions  CPU 380m  Mem 224Mi               │
└──────────────────────────────────────────────────────────────────────────────┘
┌ Workloads (4) ───────────────────────────────────────────────────────────────┐
│Verdict       Workload            Pods  CPU use / req       Suggested / pod   │
│[under]       deploy/busy-worker  1     201m / 20m (1005%)  cpu 200m*         │
│[over]        deploy/idle-api     2     0 / 400m (0%)       cpu 10m, mem 16Mi │
│[no metrics]  pod/crashloop-pod   0/1   0 / -               -                 │
│[ok]          deploy/healthy-app  3     3m / 30m (10%)      -                 │
└──────────────── * capped at the limit: usage is pinned there, so raise the limit too ┘
```

- **Grouping**: Running pods are grouped by their controlling workload (Deployment, StatefulSet, DaemonSet, Job). A ReplicaSet is resolved to its Deployment. A pod without an owner is listed on its own.
- **Usage** is each pod's peak seen this session (from `kubectl top`), so one quiet sample doesn't make a busy workload look idle.
- **Measured pods only**: Pods missing from `kubectl top` (for example, crash-looping ones) are left out of the comparison instead of counted as idle. The Pods column shows `measured/total` when some pods weren't measured.
- **Verdicts**, most urgent first:
  - **under**: using more than it requests
  - **over**: using under 30% of a request of at least 50m CPU or 64Mi memory
  - **no requests**: no CPU or memory requests set
  - **no metrics**: no pod in the workload was measured
  - **ok**
- **Suggestions** are per-pod requests: usage plus 30% headroom, rounded up to 10m CPU or 16Mi memory. A suggestion never exceeds the limit of the containers that keep running (app containers and sidecars). One capped there is marked `*`: usage pinned at a limit means the pod is throttled, so raise the limit too.
- **Reclaimable** totals the CPU and memory that following the over-provisioned suggestions would free.

Needs [metrics-server](https://github.com/kubernetes-sigs/metrics-server) in the cluster. Without it, the view explains that usage is unavailable.

### Right-Sizing Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Scroll |
| `r` | Refresh |
| `Esc` or `q` | Return to dashboard |

## Timeline View

Press `t` from the dashboard to open the incident timeline, or `T` to open it filtered to the selected incident.

### Timeline Display

Chronological view of:
- Kubernetes warning events
- Container restarts
- OOMKilled terminations
- Crash loops
- Node condition changes
- Scheduling failures
- Image pull failures
- Deployment/ReplicaSet rollouts

Entries marked with `⚡` are **precursors** - events (like rollouts or node condition changes) that likely caused subsequent incidents.

### Timeline Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑`/`k`, `↓`/`j` | Navigate timeline |
| `s` | Toggle sort order (newest first ↔ oldest first) |
| `f` | Cycle kind filter (all, events, restarts, OOM, crashes, node changes, scheduling, image pulls, rollouts) |
| `F` | Cycle severity filter (all, critical, warning, elevated) |
| `c` | Clear all filters |
| `Esc` or `q` | Return to dashboard |

### Timeline Features

- **Precursor Detection**: Automatically identifies events that occurred within 10 minutes before incidents
- **Correlation**: Groups related incidents by workload and time window
- **Filtering**: Focus on specific event types, namespaces, or severity levels
- **Time Navigation**: Sort chronologically in either direction

## Visual Indicators

### Health Score Colors

The letter grade is always shown next to the score:

- **90-100 (A)**: Green, bold
- **75-89 (B)**: Green
- **60-74 (C)**: Yellow
- **45-59 (D)**: Red
- **0-44 (F)**: Red, bold

### Pod and Node Status Colors

Pods and nodes show a status marker, colored by their health status:

- **Healthy** (`✓`): Green
- **Elevated** (`~`): Cyan
- **Warning** (`!`): Yellow
- **Critical** (`!!`): Red, bold (for example, a crash-looping or OOM-killed container, including a sidecar)

### Incident Severity

- **Critical** (`!!`): Red (CrashLoopBackOff, OOMKilled, ImagePullBackOff, NodeNotReady)
- **Warning** (`!`): Yellow (FailedScheduling, Evicted, FailedMount, FailedAttachVolume, Unhealthy)
- **Elevated** (`~`): Cyan (other warnings)

### Resource Pressure

Memory/CPU bars use a gradient:
- **0-70%**: Green to yellow
- **70-85%**: Yellow to orange
- **85-100%**: Orange to red

Pressure threshold: **85%**

## Accessibility

### Keyboard Only

Everything works from the keyboard; there is no mouse support to depend on. Press `?` in any view for the help screen, which lists every key and explains each marker.

### Color Is Never the Only Signal

- Status is written as a marker as well as a color: `!!` critical, `!` warning, `~` elevated, `✓` healthy. The same marks are used for pods, nodes, incidents, and diagnosis findings.
- The focused panel has `▶` before its title, not only a highlighted border.
- The selected row has a `>` marker and is drawn in reverse video.
- Right-sizing verdicts are words (`[under]`, `[over]`); a suggestion capped at a limit is marked `*`.
- Timeline precursors are marked `⚡`, a symbol used for nothing else.

### Readable on Light and Dark Themes

Text uses your terminal's default foreground, and highlights use the terminal's named colors (red, yellow, green, cyan), so your color scheme decides the exact shades. Only decorative elements, such as usage bars and unfocused borders, use fixed or dim colors.

### No-Color Mode

Run `cluster --no-color`, or set the [`NO_COLOR`](https://no-color.org) environment variable to any non-empty value, to draw the interface without any color. Highlights are drawn in reverse video and all status stays readable from the markers above.

### Screen Readers

The TUI is a full-screen interface that redraws in place, which most screen readers cannot follow. For screen readers, braille displays, or scripting, use the [headless commands](headless.md) instead: `cluster status`, `cluster incidents`, `cluster pods`, and `cluster nodes` print the same health data as plain text (or JSON with `--json`).

## Overlays & Modals

### Namespace Selector

Triggered by `n`:
- Shows all namespaces alphabetically
- Current namespace highlighted in **magenta** with `●`
- Selected item in **yellow**
- Pod count displayed for each namespace

### Input Modals

Used for:
- Namespace entry (`N`)
- Refresh interval (`R`)
- CSV export filename (`E`)
- Pod filter (`/` in dashboard)
- Log search (`/` in log viewer)

All support:
- Typing input
- `Backspace` to delete
- `Enter` to confirm
- `Esc` to cancel

## Tips & Tricks

### Quick Navigation

- Press number keys `1`, `2`, `3` to jump directly to panels
- Use `Tab` for sequential cycling
- `Enter` on incidents automatically jumps to the affected resource

### Efficient Log Viewing

- Start with `f` (follow mode) for live logs
- Use `/` to search for errors
- Press `t` to toggle timestamps when debugging
- Export with `E` to save logs for later analysis

### Monitoring Strategies

- Use `s` to sort pods by restarts to find crash loops quickly
- Press `T` on an incident to see the timeline leading up to it
- Use node pool filtering (`--node-pool-filter`) to focus on specific node groups
- Set a longer refresh interval (`R`) for production clusters to reduce API load

### Multi-Container Pods

- Pod detail view shows per-container status
- Use `c` in the logs section to switch containers
- Use `p` to view previous (terminated) container logs

### Performance

- Increase refresh interval for large clusters
- Use namespace filtering to reduce data volume
- The app prefetches data in the background, so navigation is instant

## Next Steps

- [Health & Incidents](health.md) - Understanding cluster health scoring
- [Configuration](configuration.md) - Customize refresh rates and defaults
- [Timeline](timeline.md) - Deep dive into timeline correlation
