//! Right-sizing: compare what each workload requests with what it uses.
//!
//! Running pods are grouped by their owning workload (a bare pod is its own
//! workload). Usage is the peak seen this session, supplied by the caller, so
//! one quiet sample does not make a busy workload look over-provisioned. Only
//! pods with measured usage are compared, so a pod missing from `kubectl top`
//! is not mistaken for an idle one.
//! Everything works on data already in the snapshot.

use std::collections::BTreeMap;

use crate::data::models::{PodInfo, PodOwner};

/// Usage below this percentage of the request counts as over-provisioned.
pub const OVERPROVISIONED_PCT: u64 = 30;
/// Headroom added on top of observed usage when suggesting a request.
pub const HEADROOM_PCT: u64 = 30;

/// Requests below these per-pod amounts are too small to be worth flagging as
/// over-provisioned.
const MIN_FLAG_CPU_MILLICORES: u64 = 50;
const MIN_FLAG_MEMORY_MB: u64 = 64;
/// Suggestions are rounded up to these steps and never go below one step.
const CPU_STEP_MILLICORES: u64 = 10;
const MEMORY_STEP_MB: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizingVerdict {
    /// Using more than requested: at risk of throttling or eviction.
    UnderProvisioned,
    /// Using well under the request: capacity is reserved but idle.
    OverProvisioned,
    /// No CPU or memory requests: the scheduler cannot plan for it.
    NoRequests,
    /// `kubectl top` returned nothing for these pods.
    NoMetrics,
    Ok,
}

impl SizingVerdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::UnderProvisioned => "under",
            Self::OverProvisioned => "over",
            Self::NoRequests => "no requests",
            Self::NoMetrics => "no metrics",
            Self::Ok => "ok",
        }
    }
}

/// Totals across a workload's running pods.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceTotals {
    pub usage: u64,
    pub request: u64,
}

impl ResourceTotals {
    /// Usage as a percentage of the request, or None when nothing is requested.
    pub fn usage_pct(self) -> Option<u64> {
        (self.request > 0).then(|| self.usage * 100 / self.request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadSizing {
    pub owner: PodOwner,
    pub namespace: String,
    /// Running pods in the workload.
    pub pods: u64,
    /// Pods with measured usage; the totals and suggestions cover only these.
    pub measured_pods: u64,
    /// CPU in millicores.
    pub cpu: ResourceTotals,
    /// Memory in MB.
    pub memory: ResourceTotals,
    pub verdict: SizingVerdict,
    /// Suggested per-pod CPU request (millicores) when the current one is off.
    pub suggested_cpu: Option<u64>,
    /// Suggested per-pod memory request (MB) when the current one is off.
    pub suggested_memory: Option<u64>,
    /// The CPU suggestion was capped at the pods' CPU limit: usage is pinned
    /// at the limit (throttled), so the limit needs raising too.
    pub cpu_at_limit: bool,
    /// The memory suggestion was capped at the pods' memory limit.
    pub memory_at_limit: bool,
}

impl WorkloadSizing {
    /// CPU (millicores) and memory (MB) that following the suggestions would
    /// free across all pods. Zero unless the workload is over-provisioned.
    pub fn reclaimable(&self) -> (u64, u64) {
        if self.verdict != SizingVerdict::OverProvisioned {
            return (0, 0);
        }
        let free = |totals: ResourceTotals, suggested: Option<u64>| {
            suggested
                .map_or(0, |per_pod| totals.request.saturating_sub(per_pod * self.measured_pods))
        };
        (free(self.cpu, self.suggested_cpu), free(self.memory, self.suggested_memory))
    }
}

/// Namespace-wide totals for the summary line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SizingSummary {
    pub cpu: ResourceTotals,
    pub memory: ResourceTotals,
    pub reclaimable_cpu: u64,
    pub reclaimable_memory: u64,
    pub workloads_flagged: usize,
}

/// Analyze running pods. `usage` returns a pod's (cpu millicores, memory MB),
/// typically the peak seen this session, or None if it was never measured.
/// Results are ordered by urgency:
/// under-provisioned, over-provisioned (most reclaimable first), no requests,
/// no metrics, then fine.
pub fn analyze(
    pods: &[PodInfo],
    usage: impl Fn(&PodInfo) -> Option<(u64, u64)>,
) -> Vec<WorkloadSizing> {
    let mut groups: BTreeMap<(String, PodOwner), Vec<&PodInfo>> = BTreeMap::new();
    for pod in pods.iter().filter(|p| p.phase == "Running") {
        let owner = pod
            .owner
            .clone()
            .unwrap_or_else(|| PodOwner { kind: "Pod".to_string(), name: pod.name.clone() });
        groups.entry((pod.namespace.clone(), owner)).or_default().push(pod);
    }

    let mut workloads: Vec<WorkloadSizing> = groups
        .into_iter()
        .map(|((namespace, owner), pods)| size_workload(owner, namespace, &pods, &usage))
        .collect();
    workloads.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then_with(|| {
                let (a_cpu, a_mem) = a.reclaimable();
                let (b_cpu, b_mem) = b.reclaimable();
                (b_cpu, b_mem).cmp(&(a_cpu, a_mem))
            })
            .then_with(|| a.owner.cmp(&b.owner))
    });
    workloads
}

pub fn summarize(workloads: &[WorkloadSizing]) -> SizingSummary {
    let mut summary = SizingSummary::default();
    for workload in workloads {
        summary.cpu.usage += workload.cpu.usage;
        summary.cpu.request += workload.cpu.request;
        summary.memory.usage += workload.memory.usage;
        summary.memory.request += workload.memory.request;
        let (cpu, memory) = workload.reclaimable();
        summary.reclaimable_cpu += cpu;
        summary.reclaimable_memory += memory;
        if workload.verdict != SizingVerdict::Ok {
            summary.workloads_flagged += 1;
        }
    }
    summary
}

fn size_workload(
    owner: PodOwner,
    namespace: String,
    pods: &[&PodInfo],
    usage: &impl Fn(&PodInfo) -> Option<(u64, u64)>,
) -> WorkloadSizing {
    let mut cpu = ResourceTotals::default();
    let mut memory = ResourceTotals::default();
    let mut measured = 0u64;
    // Lowest running limit across measured pods; None if any pod is unbounded.
    let mut cpu_limit: Option<u64> = Some(u64::MAX);
    let mut memory_limit: Option<u64> = Some(u64::MAX);
    let tighter = |current: Option<u64>, limit: Option<u64>| {
        current.zip(limit).map(|(current, limit)| current.min(limit))
    };
    for pod in pods {
        // Compare requests with usage only where usage was measured.
        let Some((cpu_used, memory_used)) = usage(pod) else {
            continue;
        };
        measured += 1;
        cpu_limit = tighter(cpu_limit, pod.running_limits.cpu_millicores);
        memory_limit = tighter(memory_limit, pod.running_limits.memory_mb);
        cpu.usage += cpu_used;
        cpu.request += pod.cpu_request_millicores;
        memory.usage += memory_used;
        memory.request += pod.memory_request_mb;
    }
    let count = measured;
    let has_metrics = measured > 0;

    let over = |totals: ResourceTotals, floor: u64| {
        totals.request >= floor * count && totals.usage * 100 < totals.request * OVERPROVISIONED_PCT
    };
    let under = |totals: ResourceTotals| totals.request > 0 && totals.usage > totals.request;

    let verdict = if !has_metrics {
        SizingVerdict::NoMetrics
    } else if cpu.request == 0 && memory.request == 0 {
        SizingVerdict::NoRequests
    } else if under(cpu) || under(memory) {
        SizingVerdict::UnderProvisioned
    } else if over(cpu, MIN_FLAG_CPU_MILLICORES) || over(memory, MIN_FLAG_MEMORY_MB) {
        SizingVerdict::OverProvisioned
    } else {
        SizingVerdict::Ok
    };

    // A request may not exceed the limit; cap there and say so, since usage
    // pinned at a limit means the true need is higher than observed.
    let suggest = |totals: ResourceTotals, step: u64, off: bool, limit: Option<u64>| {
        off.then(|| {
            let wanted = suggested_request(totals.usage.div_ceil(count.max(1)), step);
            match limit.filter(|&limit| limit != u64::MAX && wanted > limit) {
                Some(limit) => (limit, true),
                None => (wanted, false),
            }
        })
    };
    let (suggest_cpu, suggest_memory) = match verdict {
        SizingVerdict::NoRequests => (true, true),
        SizingVerdict::UnderProvisioned => (under(cpu), under(memory)),
        SizingVerdict::OverProvisioned => {
            (over(cpu, MIN_FLAG_CPU_MILLICORES), over(memory, MIN_FLAG_MEMORY_MB))
        }
        SizingVerdict::NoMetrics | SizingVerdict::Ok => (false, false),
    };
    let cpu_suggestion = suggest(cpu, CPU_STEP_MILLICORES, suggest_cpu, cpu_limit);
    let memory_suggestion = suggest(memory, MEMORY_STEP_MB, suggest_memory, memory_limit);

    WorkloadSizing {
        owner,
        namespace,
        pods: pods.len() as u64,
        measured_pods: measured,
        cpu,
        memory,
        verdict,
        suggested_cpu: cpu_suggestion.map(|(value, _)| value),
        suggested_memory: memory_suggestion.map(|(value, _)| value),
        cpu_at_limit: cpu_suggestion.is_some_and(|(_, capped)| capped),
        memory_at_limit: memory_suggestion.is_some_and(|(_, capped)| capped),
    }
}

/// Per-pod usage plus headroom, rounded up to `step` and at least one step.
fn suggested_request(per_pod_usage: u64, step: u64) -> u64 {
    let with_headroom = (per_pod_usage * (100 + HEADROOM_PCT)).div_ceil(100);
    with_headroom.div_ceil(step).max(1) * step
}
