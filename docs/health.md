# Health & Incidents

Understanding cluster health scores, incident tracking, and what they mean for your Kubernetes cluster.

## Health Score

cluster-cli calculates a comprehensive health score (0-100) with letter grading (A-F) based on weighted penalties across multiple dimensions.

### Scoring Formula

Starts at 100 and applies penalties for:

#### Critical Node Issues
- **Memory usage ≥85% of allocatable**: -15 per node
- **Not Ready or any unhealthy condition**: -10 per node (once)
- **Unhealthy Conditions**: an additional -4 per condition per node
  - Ready not `True`, or MemoryPressure, DiskPressure, PIDPressure, NetworkUnavailable `True`
  - A NotReady node with no other condition therefore costs -14

#### Critical Pod Issues
- **Failed Phase**: -18 per pod
- **Unknown Phase**: -10 per pod
- **Pending Phase**: -6 per pod
- **Running but Not Ready**: -8 per pod
- **CrashLoopBackOff**: -20 per pod
- **OOMKilled**: -15 per pod
- **Memory usage ≥85% of limit**: -10 per pod

A pod can collect several of these penalties at once (for example, a crash-looping pod is usually also Running but not ready).

#### Restart Penalties
- **2 points per restart** (capped at 100 total)

#### Warning Events
- **2 points per warning** (up to 20 warnings counted)
- **FailedScheduling**: +3 additional points per event (up to 10 events)
- **Rollout Failures**: +4 additional points per event (up to 10 events)
  - ProgressDeadlineExceeded
  - ReplicaFailure
  - FailedCreate
  - FailedDaemonPod
  - RolloutAborted
  - Or a message containing "progress deadline exceeded", "replica failure", or both "rollout" and "failed"

Event counts use each event's `count` field, so a repeated event counts once per occurrence.

The score never drops below 0.

### Native Sidecars

Native sidecars (init containers with `restartPolicy: Always`) run alongside the app containers, so cluster-cli treats them like app containers:
- Their restarts add to the pod's restart count and the restart penalty
- They count toward readiness (as in kubectl's READY column)
- A crash-looping or OOMKilled sidecar marks the pod crash-looping or OOMKilled, so the pod is critical
- Pod CPU and memory requests and limits follow the Kubernetes rules: the larger of all app containers plus all sidecars, and the peak of the init phase (each regular init container plus the sidecars started before it)

Regular init containers do not count toward readiness or restarts, but one stuck in CrashLoopBackOff still marks the pod crash-looping.

### Letter Grades

| Score | Grade | Meaning |
|-------|-------|---------|
| 90-100 | A | Excellent - cluster is healthy |
| 75-89 | B | Good - minor issues |
| 60-74 | C | Fair - some attention needed |
| 45-59 | D | Poor - significant issues |
| 0-44 | F | Critical - immediate action required |

### Thresholds

Defined in `src/data/models.rs`:
- `RESOURCE_PRESSURE_PCT = 85` - Threshold for memory/CPU pressure
- `GRADE_A_THRESHOLD = 90`
- `GRADE_B_THRESHOLD = 75`
- `GRADE_C_THRESHOLD = 60`
- `GRADE_D_THRESHOLD = 45`

### Example Calculations

#### Healthy Cluster (Score: 100, Grade: A)
- 3 nodes, all Ready
- 10 pods, all Running and Ready
- Memory usage: 40-60%
- No recent restarts
- No warning events

**Penalties**: None  
**Final Score**: 100 (A)

#### Cluster with Minor Issues (Score: 84, Grade: B)
- 3 nodes, all Ready
- 10 pods, all Running and Ready; 1 has 3 restarts
- Memory usage: 50-70%
- 2 FailedScheduling warnings

**Penalties**:
- Restarts: -6 (3 × 2)
- Warnings: -4 (2 × 2)
- FailedScheduling: -6 (2 × 3)  
**Final Score**: 84 (B)

#### Cluster in Critical State (Score: 0, Grade: F)
- 3 nodes: 2 Ready, 1 NotReady using ≥85% of its memory
- 10 pods: 7 Running and Ready, 1 CrashLooping (Running, not ready), 1 OOMKilled, 1 Pending
- Total restarts: 25
- 5 FailedScheduling events
- 1 ProgressDeadlineExceeded event

**Penalties**:
- Node NotReady: -14 (-10, plus -4 for the Ready condition)
- Node memory ≥85%: -15
- CrashLooping pod: -28 (-20, plus -8 for Running but not ready)
- OOMKilled pod: -15
- Pending pod: -6
- Restarts: -50 (25 × 2)
- Warnings: -12 (6 × 2)
- FailedScheduling: -15 (5 × 3)
- Rollout failure: -4

**Total Penalties**: -159  
**Final Score**: 0 (F), since the score never drops below 0

## Incident System

Incidents are ranked problems detected from nodes, pods, and events. They're bucketed by reason and severity for easy prioritization.

### Incident Sources

#### From Nodes
- **NodeNotReady**: Node reports NotReady status

#### From Pods
- **CrashLoopBackOff**: An app container, native sidecar, or init container is in CrashLoopBackOff
- **OOMKilled**: An app container or native sidecar was killed by the OOM killer

#### From Containers
- Per-container status parsing (app containers and native sidecars) for:
  - ImagePullBackOff / ErrImagePull in waiting state
  - CrashLoopBackOff in waiting state (when the pod-level incident is not already raised)
  - OOMKilled in last termination reason (when the pod-level incident is not already raised)

#### From Events (Warnings only)
- **FailedScheduling**: Pod cannot be placed on any node
- **Evicted**: Pod was evicted from node (reason, or "evicted" in the message)
- **NodeNotReady**: Node became not ready (reason, or "node is not ready" in the message)
- **BackOff** with "restarting failed container": mapped to CrashLoopBackOff
- **Image pull failures**: messages mentioning ImagePullBackOff, ErrImagePull, "pull image", or "pulling image" are mapped to ImagePullBackOff
- **OOMKilled**: reason, or "oomkilled" in the message
- **FailedMount / FailedAttachVolume**: Storage issues
- **Unhealthy**: Probe failures
- **Other warning reasons**: bucketed under their own reason with Elevated severity

Normal events never create incidents.

#### Event Collection

Each refresh reads the namespace's events with `kubectl get events -o json` and keeps the 50 most recent. Warning events fill that cap before Normal events, so a burst of routine Normal events (Pulled, Created, Started, ScalingReplicaSet) cannot push out an older Warning such as FailedScheduling.

Events written through the newer `events.k8s.io` API, such as the scheduler's FailedScheduling, often have no `lastTimestamp`. cluster-cli takes their time from `series.lastObservedTime` or `eventTime` (then `firstTimestamp` or the creation time), so they sort correctly and are included.

### Incident Severity

#### Critical
- CrashLoopBackOff
- OOMKilled
- ImagePullBackOff
- NodeNotReady

#### Warning
- FailedScheduling
- Evicted
- FailedMount
- FailedAttachVolume
- Unhealthy (probe failures)

#### Elevated
- All other warning events

### Incident Bucketing

Incidents are grouped into one bucket per reason (e.g., all CrashLoopBackOff). Each bucket accumulates an occurrence count and the set of affected resources, and takes the highest severity of its members.

Each bucket contains:
- **Reason**: Human-readable incident type
- **Severity**: Critical, Warning, or Elevated
- **Occurrences**: Sum of event counts (1 per pod, node, or container status finding)
- **Targets**: Specific resources affected (pods, nodes, containers, workloads)
- **Sample Message**: Representative error message
- **Latest Timestamp**: When it last occurred

### Incident Ranking

Incidents are sorted by:
1. **Severity rank**: Critical > Warning > Elevated
2. **Occurrence count**: More frequent first
3. **Affected resources**: More resources first
4. **Timestamp**: Most recent first
5. **Reason**: Alphabetically (tie-breaker)

### Incident Drill-Down

Press `Enter` on an incident in the dashboard to jump to:
- **Pod**: If the incident targets a single pod
- **Node**: If it targets a single node
- **Workload**: If it targets a single deployment/statefulset/etc. (opens the workload popup)
- **Pod list**: If it targets several pods or workloads, the pod list is focused on the affected pods

Press `T` to open the timeline for the selected incident. See [Timeline](timeline.md#opening-the-timeline).

## Resource Pressure

### Memory Pressure

Triggered when:
- **Node**: Memory usage ≥ 85% of allocatable memory
- **Pod**: Memory usage ≥ 85% of the pod's effective memory limit

Calculation:
```rust
memory_pct = (current_memory_mb / limit_mb) * 100

// If the pod has no memory limit, memory_pct is 0 (no pressure)
```

The pod's effective limit follows the Kubernetes rules for init containers and native sidecars (see [Native Sidecars](#native-sidecars)). Usage comes from `kubectl top`, so it needs metrics-server; without it, usage reads as 0.

### CPU Pressure

Displayed but not currently factored into health score:
```rust
cpu_pct = (current_cpu_millicores / limit_millicores) * 100
```

### Visual Indicators

Percentage cells are colored by band:
- **0-30%**: Teal
- **31-55%**: Green
- **56-70%**: Yellow
- **71-84%**: Orange
- **85-100%**: Red

## Pod Readiness

A pod is considered ready when:
- At least one app container or native sidecar exists
- Every app container and native sidecar is ready

Readiness affects:
- Health score (-8 penalty when the phase is Running but the pod is not ready)
- Pod status (a non-Pending, non-Succeeded pod that is not ready is critical)
- Pod sort modes (not-ready sorts unready pods first)

### Readiness vs Liveness

cluster-cli tracks:
- **Readiness failures**: Inferred from `Unhealthy` pod events with "Readiness probe failed" messages
- **Liveness failures**: Inferred from `Unhealthy` pod events with "Liveness probe failed" messages

These appear as findings in the pod detail diagnosis but don't directly affect the health score (they typically lead to restarts or non-ready status, which do affect the score).

## Critical Counts

Displayed in the header:
- **Critical Nodes**: Nodes with memory usage ≥85%, NotReady, or unhealthy conditions
- **Critical Pods**: Pods with memory usage ≥85% of limit, Failed/Unknown phase, Running but NotReady, CrashLooping, or OOMKilled
- **Total Restarts**: Sum of restart counts across all pods (app containers and native sidecars)

These provide quick at-a-glance indicators of cluster health.

## Recommendations

### For High Restart Counts
1. Check pod logs for crash reasons
2. Review resource limits (OOMKilled pods)
3. Inspect liveness probe configurations
4. Check for image pull issues

### For Node Pressure
1. Scale up node pool
2. Identify resource-hungry pods
3. Evict or reschedule pods
4. Add resource limits to prevent runaway consumption

### For Scheduling Failures
1. Check node selectors and taints
2. Review resource requests
3. Verify cluster capacity
4. Check pod disruption budgets

### For Crash Loops
1. Review application logs
2. Check configuration and secrets
3. Verify dependencies (databases, services)
4. Inspect startup scripts and commands

### For OOMKills
1. Increase memory limits
2. Profile application memory usage
3. Check for memory leaks
4. Review memory requests vs actual usage

## Monitoring Best Practices

### Proactive Monitoring
- Aim for Grade A (90+) in production
- Grade B (75-89) should trigger investigation
- Grade C (60-74) requires immediate attention
- Grade D/F (< 60) is a production incident

### Regular Checks
- Monitor health score trends over time
- Review incident buckets daily
- Track restart patterns
- Watch for new incident types

### Alerting Thresholds
Consider external alerting for:
- Health score < 75 (below grade B)
- Critical node count > 0
- Critical pod count > 10% of total
- Any incident bucket with > 10 occurrences

## Next Steps

- [Timeline](timeline.md) - Investigate incident chronology
- [Pod Detail View](ui.md#pod-detail-view) - Deep dive into pod issues
- [Headless Mode](headless.md) - Script health checks
