# Incident Timeline

Investigate cluster incidents with chronological correlation and precursor detection.

## Overview

The incident timeline provides a chronological view of cluster events, container restarts, OOM kills, node condition changes, and deployment rollouts. It helps answer:

- **What happened before this incident?**
- **Are these incidents related?**
- **Did a deployment cause these pod failures?**
- **Which node condition triggered pod evictions?**

## Opening the Timeline

### From Dashboard

- Press `t` to open the timeline (all entries)
- Press `T` with the Incident Queue focused to open the timeline for the selected incident. The header shows the incident's reason, and when the incident's first target is a pod, a namespace filter for that pod's namespace is applied. Press `c` to clear it.

### Timeline View

```
┌─ Incident Timeline ──────────────────────────────────────────────────────┐
│ Filters: None  |  Sort: Newest First                                     │
└──────────────────────────────────────────────────────────────────────────┘
┌─ Timeline (6 entries) ───────────────────────────────────────────────────┐
│ 0s ago     [CRITICAL] [CrashLoop]     Pod/api-6d4f9-x2k  Pod is in Cra…  │
│ 0s ago     [CRITICAL] [OOMKilled]     Pod/api-6d4f9-x2k/app  Container…  │
│ 0s ago     [ELEVATED] [Restart]       Pod/nginx-abc/nginx  Container r…  │
│ 0s ago     [CRITICAL] [NodeCondition] Node/node-1  Node is NotReady      │
│ 4m ago     [ELEVATED] [Rollout]       ⚡ Deployment/api  Scaled up rep…  │
│ 12m ago    [WARNING]  [SchedulingFail] Pod/worker-0  0/3 nodes are av…   │
└──────────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────┐
│ 1/6 | q Back | ↑/k/↓/j Scroll | s Sort | f Filter Kind | F Filter Sev…   │
└──────────────────────────────────────────────────────────────────────────┘
```

Each row shows the time (relative, such as `5m ago`, for the last 24 hours, otherwise `MM-DD HH:MM`), the severity, the kind, a `⚡` precursor marker, the resource, the namespace when known, and the message.

## Timeline Entry Types

Every Kubernetes event in the snapshot (Warning and Normal) becomes one entry, with its kind chosen from the event's reason and message. Pod and node status add further entries.

Event severity: Normal events are Elevated. Warning events are Critical for OOMKilled, CrashLoopBackOff, or NodeNotReady reasons (or "oomkilled" in the message); Warning for FailedScheduling, Evicted, FailedMount, FailedAttachVolume, Unhealthy, and BackOff; otherwise Elevated.

### Event
Kubernetes events not matching a more specific kind.

**Source**: Kubernetes events  
**Severity**: From the event (see above)

### ContainerRestart
Container with a non-zero restart count, one entry per container. Covers app containers and native sidecars (init containers with `restartPolicy: Always`).

**Source**: Container restart counts in pod status  
**Severity**:
- 1-2 restarts: Elevated
- 3-5 restarts: Warning
- 6+ restarts: Critical

### OOMKilled
Container terminated by the kernel OOM killer due to memory limits.

**Source**: Last termination reason of app containers and native sidecars (Critical), and events with an OOMKilled reason or message  
**Indicates**: Memory limit too low, memory leak, or resource pressure

### CrashLoop
Pod is in CrashLoopBackOff state.

**Source**: The pod's crash-looping flag, set when an app container, native sidecar, or init container is in CrashLoopBackOff (Critical), and events with a CrashLoopBackOff or BackOff reason  
**Indicates**: Application crash, configuration error, missing dependencies

### NodeConditionChange
Node condition is unhealthy (NotReady, MemoryPressure, DiskPressure, PIDPressure), or an event about a Node.

**Source**: Node conditions and Node events  
**Severity** (from node conditions):
- NotReady: Critical
- MemoryPressure: Warning
- DiskPressure: Warning
- PIDPressure: Warning

**Often a precursor**: Node issues frequently cause pod failures

### SchedulingFailure
Pod cannot be scheduled on any node.

**Source**: FailedScheduling events  
**Severity**: Warning

**Causes**: Insufficient resources, taints, node selectors, affinity rules

### ImagePullFailure
Container cannot pull its image.

**Source**: Events whose message mentions ImagePullBackOff or ErrImagePull, or `Failed` events about pulling an image (`BackOff` events are classified as CrashLoop)  
**Severity**: From the event (usually Elevated for `Failed` events)

**Causes**: Image not found, registry auth failure, network issues

### Rollout
Deployment, StatefulSet, DaemonSet, or ReplicaSet rollout event. A rollout event also appears once as a regular entry of its own kind.

**Source**: Events on Deployments, ReplicaSets, StatefulSets, and DaemonSets  
**Types**:
- **ScaledUp / ScaledDown**: `ScalingReplicaSet` events, or "scaled up"/"scaled down" in the message
- **ImageChange**: Message mentions an image being updated or changed
- **NewRevision**: `NewRevision` or `RollingUpdate` reason, or "revision" in the message
- **Failed**: `ProgressDeadlineExceeded` or `FailedCreate` reason, or "exceeded its progress deadline" in the message

**Severity**:
- NewRevision, ImageChange, ScaledUp, ScaledDown: Elevated
- Failed: Warning

**Often a precursor**: Rollouts frequently trigger cascading issues

## Precursor Detection

Entries marked with `⚡` are **precursors** - events that likely caused subsequent incidents.

### Precursor Logic

An entry is marked as a precursor if:

1. **It's a likely precursor kind**:
   - Rollout (deployments often cause issues)
   - NodeConditionChange (node issues affect pods)

2. **It happened before a critical incident**: a Critical CrashLoop, OOMKilled, ImagePullFailure, or SchedulingFailure entry

3. **It's within the precursor window** (10 minutes)

4. **For rollouts**: It affects the same workload. The workload is matched on the first dash-separated part of the resource name (`api` for `Deployment/api` and `Pod/api-6d4f9-x2k`)

Both timestamps must parse as RFC 3339.

### Example

```
⚡ 06:30:00  [ELEVATED]  Rollout  Deployment/api  (Scaled up replica set)
  06:32:15  [CRITICAL]  CrashLoop  Pod/api-6d4f9-x2k
  06:32:20  [CRITICAL]  CrashLoop  Pod/api-7h3k1-z9m
```

The rollout at 06:30 is marked as a precursor because:
- It's a rollout event (likely precursor kind)
- It happened 2 minutes before the crash loops
- It affects the same deployment (api)

## Timeline Navigation

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `s` | Toggle sort order (newest ↔ oldest) |
| `f` | Cycle kind filter |
| `F` | Cycle severity filter |
| `c` | Clear all filters |
| `Esc` / `q` | Return to dashboard |

### Sort Modes

#### Newest First (Default)
Most recent events at the top. Best for:
- Real-time monitoring
- Finding latest issues
- Recent incident investigation

#### Oldest First
Earliest events at the top. Best for:
- Understanding incident progression
- Root cause analysis
- Timeline reconstruction

Press `s` to toggle.

### Filtering

#### Kind Filter (Press `f`)

Cycle through (the header shows the active filter as `Kind: <label>`):
- **All**: Show all entry types
- **Event**: Only generic events
- **Restart**: Only container restarts
- **OOMKilled**: Only OOMKilled terminations
- **CrashLoop**: Only crash loops
- **NodeCondition**: Only node condition changes
- **SchedulingFail**: Only scheduling failures
- **ImagePullFail**: Only image pull failures
- **Rollout**: Only deployment rollouts

#### Severity Filter (Press `F`)

Cycle through:
- **All**: Show all severities
- **Critical**: Only critical incidents
- **Warning**: Warning and above
- **Elevated**: Elevated and above

#### Clear Filters (Press `c`)

Reset the kind, severity, and namespace filters to show everything. Leaving the timeline with `q`/`Esc` also clears them.

There is no separate details view; each row shows the entry's message inline.

## Timeline Building

### Data Sources

The timeline is built from:

1. **Kubernetes Events**: Warning and Normal events in the current namespace
2. **Pod Status**: Container restarts, crash loops, OOMKilled markers (including native sidecars)
3. **Node Status**: Unhealthy node conditions
4. **Workload Events**: Deployment, ReplicaSet, StatefulSet, and DaemonSet events, as Rollout entries

The timeline is rebuilt from the latest snapshot on every refresh.

### Timestamp Handling

- **Events**: `lastTimestamp`, or for events written through the newer `events.k8s.io` API (such as the scheduler's FailedScheduling), `series.lastObservedTime` or `eventTime`, then `firstTimestamp` or the creation time
- **Pod/Container Status**: Current time when the timeline is built (kubectl doesn't provide exact restart timestamps)
- **Node Conditions**: Current time when the timeline is built

Timestamps are sorted using RFC3339 parsing with fallback to string comparison.

### Entry Limits

The timeline shows:
- **Events** from the current snapshot: the 50 most recent per refresh, with Warning events kept ahead of Normal ones
- **All pod incidents** (restarts, crashes, OOM)
- **All unhealthy node conditions**
- **All workload rollouts** found in those events

Filter to focus.

## Correlation Analysis

### Workload Grouping

`group_timeline_entries()` in `src/data/timeline.rs` can group entries by:
- **Workload key**: Extracted from resource names (e.g., "api" from "api-6d4f9-x2k")
- **Namespace**: Shared namespace
- **Time window**: Entries within a caller-supplied number of seconds

The timeline view does not use it yet; entries are shown as a flat list.

### Time Window

The precursor window is **10 minutes**. An entry must occur within 10 minutes before an incident to be considered a precursor.

The window is set where the timeline is built, in `src/app.rs`, and is not user-configurable:
```rust
mark_precursors(&mut entries, 600); // 600 seconds = 10 minutes
```

## Use Cases

### Investigating a CrashLoopBackOff

1. Navigate to incident in dashboard
2. Press `T` to open the timeline for that incident
3. Look for precursor events (marked with `⚡`)
4. Check rollouts or node condition changes before the crash and read their messages

### Finding the Cause of OOMKills

1. Open timeline with `t`
2. Press `F` once to filter to Critical severity
3. Press `f` until the header shows `Kind: OOMKilled` (three presses)
4. Look at the timeline before each OOM
5. Check for:
   - Recent rollouts (image changes, scaling)
   - Node memory pressure
   - Related restarts

### Tracking Deployment Rollout Impact

1. Open timeline with `t`
2. Press `f` until the header shows `Kind: Rollout` (eight presses)
3. Note the rollout timestamp
4. Press `c` to clear the filter
5. Look for incidents shortly after the rollout

### Correlating Node Issues with Pod Failures

1. Open timeline with `t`
2. Press `f` until the header shows `Kind: NodeCondition` (five presses)
3. Note nodes with NotReady or MemoryPressure
4. Press `c` to show all entries
5. Look for pod evictions or scheduling failures after node issues

## Timeline Limitations

### Timestamp Accuracy

- **Events**: Accurate timestamps from Kubernetes
- **Pod restarts**: Estimated (not exact)
- **Node conditions**: Estimated (not exact)

For precise historical analysis, integrate with a log aggregation system.

### Historical Depth

The timeline shows:
- Events from the most recent `kubectl get events`
- Current pod/node state

It does NOT show:
- Events older than Kubernetes' event retention (usually 1 hour)
- Historical pod restarts (only current count)
- Past node condition changes

For long-term trends, export data periodically or use a monitoring system.

### Incomplete Correlation

Precursor detection is heuristic-based:
- Not all cause-effect relationships are detected
- Some precursors may be false positives
- Complex multi-service failures may not be fully linked

Use the timeline as a starting point, not definitive proof.

## Best Practices

### Regular Timeline Reviews

- Check timeline daily for pattern recognition
- Look for repeating incidents (same time, same cause)
- Track rollout success/failure rates

### Precursor Pattern Learning

- Note which types of rollouts cause issues
- Identify problematic node pools or zones
- Document common failure chains

### Filtering Strategies

For busy clusters:
1. Start with Critical severity only
2. Add specific kind filters (CrashLoop, OOM)
3. Look at precursors first (⚡ markers)
4. Expand filters if needed

### Combining with Other Views

- Use timeline to identify affected pods
- Switch to pod detail view for logs
- Use dashboard for current cluster state
- Export data for long-term analysis

## Technical Details

### Implementation

Timeline building: `src/data/timeline.rs`  
Timeline UI: `src/ui/views/timeline.rs`

Key functions:
- `build_timeline()`: Constructs timeline from snapshot
- `mark_precursors()`: Identifies precursor events
- `filter_timeline_entries()`: Applies kind/severity filters
- `group_timeline_entries()`: Groups correlated entries (not yet used by the UI)

### Performance

Timeline building is fast:
- O(n log n) sorting
- O(n) filtering
- O(incidents × entries) precursor detection

No performance issues even with hundreds of events.

## Future Enhancements

Planned features:
- Export timeline to JSON/CSV
- Custom time window configuration
- Multi-namespace timeline view
- Timeline diff (compare two snapshots)
- Timeline scrubbing (navigate time)
- Automatic incident grouping in UI
- Custom precursor rules

## Next Steps

- [Health & Incidents](health.md) - Understanding incident severity and ranking
- [Pod Detail View](ui.md#pod-detail-view) - Investigate specific pods
- [User Interface Guide](ui.md) - Complete keyboard reference
