# Health & Incidents

Understanding cluster health scores, incident tracking, and what they mean for your Kubernetes cluster.

## Health Score

cluster-cli calculates a comprehensive health score (0-100) with letter grading (A-F) based on weighted penalties across multiple dimensions.

### Scoring Formula

Starts at 100 and applies penalties for:

#### Critical Node Issues
- **MemoryPressure (≥85%)**: -15 per node
- **Not Ready**: -10 per node
- **Unhealthy Conditions**: -4 per condition per node
  - DiskPressure, PIDPressure, NetworkUnavailable

#### Critical Pod Issues
- **Failed Phase**: -18 per pod
- **Unknown Phase**: -10 per pod
- **Pending Phase**: -6 per pod
- **Running but Not Ready**: -8 per pod
- **CrashLoopBackOff**: -20 per pod
- **OOMKilled**: -15 per pod
- **MemoryPressure (≥85%)**: -10 per pod

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

### Letter Grades

| Score | Grade | Meaning |
|-------|-------|---------|
| 90-100 | A | Excellent - cluster is healthy |
| 80-89 | B | Good - minor issues |
| 70-79 | C | Fair - some attention needed |
| 60-69 | D | Poor - significant issues |
| 0-59 | F | Critical - immediate action required |

### Thresholds

Defined in `src/data/models.rs`:
- `RESOURCE_PRESSURE_PCT = 85` - Threshold for memory/CPU pressure
- `GRADE_A_THRESHOLD = 90`
- `GRADE_B_THRESHOLD = 80`
- `GRADE_C_THRESHOLD = 70`
- `GRADE_D_THRESHOLD = 60`

### Example Calculations

#### Healthy Cluster (Score: 100, Grade: A)
- 3 nodes, all Ready
- 10 pods, all Running and Ready
- Memory usage: 40-60%
- No recent restarts
- No warning events

**Penalties**: None  
**Final Score**: 100 (A)

#### Cluster with Minor Issues (Score: 82, Grade: B)
- 3 nodes, all Ready
- 10 pods: 9 Running+Ready, 1 with 3 restarts
- Memory usage: 50-70%
- 2 FailedScheduling warnings

**Penalties**:
- Restarts: -6 (3 × 2)
- Warnings: -4 (2 × 2)
- FailedScheduling: -6 (2 × 3)  
**Final Score**: 84 (B)

#### Cluster in Critical State (Score: 18, Grade: F)
- 3 nodes: 2 Ready, 1 NotReady with MemoryPressure
- 10 pods: 7 Running, 1 CrashLooping, 1 OOMKilled, 1 Pending
- Total restarts: 25
- 5 FailedScheduling events
- 1 ProgressDeadlineExceeded event

**Penalties**:
- Node NotReady: -10
- Node MemoryPressure: -15
- CrashLooping pod: -20
- OOMKilled pod: -15
- Pending pod: -6
- Restarts: -50 (25 × 2)
- Warnings: -10 (5 × 2)
- FailedScheduling: -15 (5 × 3)
- Rollout failure: -4

**Total Penalties**: -145 (capped at 100)  
**Final Score**: 0, but displayed as 18 in practice (F)

## Incident System

Incidents are ranked problems detected from nodes, pods, and events. They're bucketed by reason and severity for easy prioritization.

### Incident Sources

#### From Nodes
- **NodeNotReady**: Node reports NotReady status

#### From Pods
- **CrashLoopBackOff**: Container repeatedly crashes
- **OOMKilled**: Container killed by OOM killer
- **ImagePullBackOff**: Cannot pull container image

#### From Containers
- Per-container status parsing for:
  - CrashLoopBackOff in waiting state
  - OOMKilled in last termination reason
  - ImagePullBackOff / ErrImagePull in waiting state

#### From Events (Warnings)
- **FailedScheduling**: Pod cannot be placed on any node
- **Evicted**: Pod was evicted from node
- **NodeNotReady**: Node became not ready
- **BackOff**: Restarting failed container (mapped to CrashLoopBackOff)
- **ImagePullBackOff / ErrImagePull**: Image pull failures
- **FailedMount / FailedAttachVolume**: Storage issues
- **Unhealthy**: Probe failures
- **Other warning reasons**: Generic elevated severity

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

Incidents are grouped by:
1. **Reason**: Same type of incident (e.g., all CrashLoopBackOff)
2. **Occurrence Count**: How many times it happened
3. **Affected Resources**: Which pods/nodes/workloads

Each bucket contains:
- **Reason**: Human-readable incident type
- **Severity**: Critical, Warning, or Elevated
- **Occurrences**: Event count
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
- **Pod**: If the incident targets a specific pod
- **Node**: If it's a node-level issue
- **Workload**: If it's a deployment/statefulset/etc. issue

Press `T` to open the timeline filtered to the selected incident's context.

## Resource Pressure

### Memory Pressure

Triggered when:
- **Node**: Memory usage ≥ 85% of capacity
- **Pod**: Memory usage ≥ 85% of limit (or request if no limit)

Calculation:
```rust
memory_pct = (current_memory_mb / limit_mb) * 100

// If no limit, use request
// If no request, use 0% (no pressure)
```

### CPU Pressure

Displayed but not currently factored into health score:
```rust
cpu_pct = (current_cpu_millicores / limit_millicores) * 100
```

### Visual Indicators

Pressure bars use gradient colors:
- **0-70%**: Green → Yellow
- **70-85%**: Yellow → Orange
- **85-100%**: Orange → Red

## Pod Readiness

A pod is considered ready when:
- Phase is "Running"
- Ready container count == Total container count
- At least one container exists

Readiness affects:
- Health score (-8 penalty for running but not ready)
- Pod sort modes (not-ready sorts unready pods first)
- Visual indicators (yellow for not ready)

### Readiness vs Liveness

cluster-cli tracks:
- **Readiness failures**: Inferred from pod events with "Readiness probe failed" messages
- **Liveness failures**: Inferred from pod events with "Liveness probe failed" messages

These are displayed in the pod detail overview section but don't directly affect the health score (they typically lead to restarts or non-ready status, which do affect the score).

## Critical Counts

Displayed in the header:
- **Critical Nodes**: Nodes with MemoryPressure ≥85%, NotReady, or unhealthy conditions
- **Critical Pods**: Pods with MemoryPressure ≥85%, Failed/Unknown phase, Running but NotReady, CrashLooping, or OOMKilled
- **Total Restarts**: Sum of restart counts across all pods

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
- Grade B (80-89) should trigger investigation
- Grade C (70-79) requires immediate attention
- Grade D/F (< 70) is a production incident

### Regular Checks
- Monitor health score trends over time
- Review incident buckets daily
- Track restart patterns
- Watch for new incident types

### Alerting Thresholds
Consider external alerting for:
- Health score < 80 (below grade B)
- Critical node count > 0
- Critical pod count > 10% of total
- Any incident bucket with > 10 occurrences

## Next Steps

- [Timeline](timeline.md) - Investigate incident chronology
- [Pod Detail View](ui.md#pod-detail-view) - Deep dive into pod issues
- [Headless Mode](headless.md) - Script health checks
