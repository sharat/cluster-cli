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

- Press `t` to open the timeline (all events)
- Press `T` on a selected incident to open timeline filtered to that incident

### Timeline View

```
┌─ Incident Timeline ──────────────────────────────────────────────────────┐
│ Filters: None  |  Sort: Newest First                                      │
├──────────────────────────────────────────────────────────────────────────┤
│ 2026-10-14 06:35:00  [CRITICAL]  CrashLoop       Pod/api-6d4f9-x2k       │
│ ⚡ 2026-10-14 06:30:00  [ELEVATED]  Rollout         Deployment/api         │
│ 2026-10-14 06:25:00  [WARNING]   SchedulingFail Pod/worker-0             │
│ 2026-10-14 06:20:00  [CRITICAL]  OOMKilled       Pod/api-6d4f9/app       │
│ ⚡ 2026-10-14 06:15:00  [WARNING]   NodeCondition   Node/node-1            │
│ 2026-10-14 06:10:00  [ELEVATED]  Restart         Pod/nginx-abc/nginx     │
└──────────────────────────────────────────────────────────────────────────┘
│ ↑/k ↓/j: Navigate  s: Sort  f: Filter Kind  F: Filter Severity  q: Back │
└──────────────────────────────────────────────────────────────────────────┘
```

## Timeline Entry Types

### Event
Generic Kubernetes warning events not matching specific patterns.

**Source**: Kubernetes events API  
**Severity**: Typically Elevated, sometimes Warning

### ContainerRestart
Container restart detected in pod status.

**Source**: Pod container restart counts  
**Severity**:
- 0-2 restarts: Elevated
- 3-5 restarts: Warning
- 6+ restarts: Critical

### OOMKilled
Container terminated by the kernel OOM killer due to memory limits.

**Source**: Container termination reason  
**Severity**: Always Critical

**Indicates**: Memory limit too low, memory leak, or resource pressure

### CrashLoop
Pod is in CrashLoopBackOff state.

**Source**: Pod phase and crash_looping flag  
**Severity**: Always Critical

**Indicates**: Application crash, configuration error, missing dependencies

### NodeConditionChange
Node condition became unhealthy (NotReady, MemoryPressure, DiskPressure, PIDPressure).

**Source**: Node conditions  
**Severity**:
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

**Source**: ImagePullBackOff and ErrImagePull events  
**Severity**: Critical

**Causes**: Image not found, registry auth failure, network issues

### Rollout
Deployment, StatefulSet, DaemonSet, or ReplicaSet rollout event.

**Source**: Deployment/ReplicaSet events  
**Types**:
- **NewRevision**: New deployment revision created
- **ImageChange**: Container image updated
- **ScaledUp**: Replicas increased
- **ScaledDown**: Replicas decreased
- **Failed**: Rollout failure (ProgressDeadlineExceeded, etc.)

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

2. **It happened before a critical incident**

3. **It's within the precursor window** (default: 10 minutes)

4. **For rollouts**: It affects the same workload

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
| `Enter` | Show entry details |
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

Cycle through:
- **All**: Show all entry types
- **Event**: Only generic events
- **Restart**: Only container restarts
- **OOM**: Only OOMKilled terminations
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

Reset all filters to show everything.

## Entry Details

Press `Enter` on a timeline entry to see full details:

```
┌─ Entry Details ──────────────────────────────────────────────────────────┐
│                                                                           │
│ Type: CrashLoop                                                           │
│ Severity: Critical                                                        │
│ Timestamp: 2026-10-14T06:35:00Z                                           │
│ Resource: Pod/api-6d4f9-x2k                                               │
│ Namespace: production                                                     │
│ Message: Pod is in CrashLoopBackOff                                       │
│                                                                           │
│ Details:                                                                  │
│   Pod: api-6d4f9-x2k                                                      │
│   Restarts: 5                                                             │
│                                                                           │
│ Press Esc or q to close                                                   │
└──────────────────────────────────────────────────────────────────────────┘
```

## Timeline Building

### Data Sources

The timeline is built from:

1. **Kubernetes Events**: Warning and normal events from the cluster
2. **Pod Status**: Container restarts, crash loops, OOMKilled markers
3. **Node Status**: Node condition changes
4. **Workload Events**: Deployment, StatefulSet, DaemonSet rollouts

### Timestamp Handling

- **Events**: Use event timestamp from Kubernetes
- **Pod/Container Status**: Estimated as current time (kubectl doesn't provide exact restart timestamps)
- **Node Conditions**: Estimated as current time

Timestamps are sorted using RFC3339 parsing with fallback to string comparison.

### Entry Limits

The timeline shows:
- **All events** from the current snapshot
- **All pod incidents** (restarts, crashes, OOM)
- **All node conditions**
- **All workload rollouts**

No arbitrary limits. Filter to focus.

## Correlation Analysis

### Workload Grouping

Entries are internally grouped by:
- **Workload key**: Extracted from resource names (e.g., "api" from "api-6d4f9-x2k")
- **Namespace**: Shared namespace
- **Time window**: Events within N seconds (default: 5 minutes)

This powers future features like:
- "Show me all incidents related to the API deployment"
- "Group timeline entries by service"
- "Find correlated failures"

### Time Window

The precursor window is **10 minutes** by default. An event must occur within 10 minutes before an incident to be considered a precursor.

This is configurable in the code (`src/data/timeline.rs`):
```rust
mark_precursors(&mut entries, 600); // 600 seconds = 10 minutes
```

## Use Cases

### Investigating a CrashLoopBackOff

1. Navigate to incident in dashboard
2. Press `T` to open timeline filtered to that incident
3. Look for precursor events (marked with `⚡`)
4. Check rollouts or node condition changes before the crash
5. Press `Enter` on the rollout to see details

### Finding the Cause of OOMKills

1. Open timeline with `t`
2. Press `F` to filter to Critical severity
3. Press `f` to filter to OOM entries
4. Look at the timeline before each OOM
5. Check for:
   - Recent rollouts (image changes, scaling)
   - Node memory pressure
   - Related restarts

### Tracking Deployment Rollout Impact

1. Open timeline with `t`
2. Press `f` to filter to Rollout entries
3. Note the rollout timestamp
4. Switch filter to All (`f` several times)
5. Look for incidents shortly after the rollout

### Correlating Node Issues with Pod Failures

1. Open timeline with `t`
2. Press `f` to filter to NodeCondition
3. Note nodes with NotReady or MemoryPressure
4. Switch to All entries
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
- `group_timeline_entries()`: Groups correlated entries (future feature)

### Performance

Timeline building is fast:
- O(n log n) sorting
- O(n) filtering
- O(n²) precursor detection (bounded by time window)

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
