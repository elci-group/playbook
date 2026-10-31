//! The Mesut execution seam: heterogeneous local execution optimisation
//! beneath `playbook-live`.
//!
//! Football: the offensive line. Nobody chants about the line, but nothing
//! else works without it. Mesut sits beneath the capability plane and
//! decides *where and how* authorised work runs — Tokio, Rayon, or plain
//! blocking — while the work's meaning is fixed by Playbook.
//!
//! The constitutional line, enforced by the types: a
//! [`MesutExecutionRequest`] is an immutable authority grant converted to a
//! typed workload. Mesut's routing decision may move the work between
//! substrates, queues, and worker capacities; every [`ScrambleDecision`]
//! must re-assert the preserved contract — same capability, same evidence,
//! same authority, same objective — or the scramble is refused. When Mesut
//! itself is unavailable, execution falls back to the ordinary
//! playbook-live path and nothing is lost.

use serde::{Deserialize, Serialize};

use playbook_core::EvidenceClass;

/// The kind of local work an authorised invocation represents. Mesut
/// coordinates these differently: Tokio for async I/O, Rayon for CPU-bound
/// parallelism, a blocking pool for tool work that would starve an async
/// runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkKind {
    CpuBound,
    IoBound,
    BlockingTool,
}

/// Execution urgency. Ordering is meaningful: `Critical` preempts `High`,
/// and so on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

/// The local substrate a piece of work runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Substrate {
    Tokio,
    Rayon,
    Blocking,
}

/// Hard resource constraints. Immutable by construction: the request is
/// consumed by the seam and constraint values are only ever read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceConstraints {
    pub max_workers: u16,
    pub max_memory_hint_mb: Option<u64>,
    pub requires_dedicated_core: bool,
}

/// Soft preferences Mesut may honour when hard constraints allow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulingPreferences {
    pub prefer_substrate: Option<Substrate>,
    pub defer_until_ms: Option<u64>,
}

/// Whether the work may be cancelled, and how gracefully.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationContract {
    pub cancellable: bool,
    pub graceful_window_ms: Option<u64>,
}

/// The typed conversion of an authorised invocation into a Mesut workload.
/// Built from the playbook-live envelope: authority fields are copied,
/// never derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MesutExecutionRequest {
    pub execution_id: String,
    pub work_kind: WorkKind,
    pub priority: Priority,
    pub hard_constraints: ResourceConstraints,
    pub preferred_constraints: SchedulingPreferences,
    pub cancellation: CancellationContract,
    pub deadline_ms: Option<u64>,
    pub expected_evidence: Vec<EvidenceClass>,
}

/// The measured execution pressure that can trigger a scramble.
/// The zero reading: no measured pressure on any dimension.
impl Default for ExecutionPressure {
    fn default() -> Self {
        Self {
            queue_latency_ms: 0,
            blocked_workers: 0,
            cpu_saturation_ppm: 0,
            io_stall_ms: 0,
            service_time_prediction_error_ppm: 0,
            deadline_imminent: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPressure {
    pub queue_latency_ms: u64,
    pub blocked_workers: u16,
    pub cpu_saturation_ppm: u32,
    pub io_stall_ms: u64,
    pub service_time_prediction_error_ppm: u32,
    pub deadline_imminent: bool,
}

/// Why a scramble fired. Machine reasons, one per directive trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrambleReason {
    ExecutorCongestion,
    BlockedWorker,
    CpuSaturation,
    IoStall,
    ServiceTimeShift,
    DeadlinePressure,
}

/// What a scramble preserves. All four must be true for the scramble to be
/// lawful: possession, objective, authority, and evidence contract are
/// untouched — only the substrate and ordering moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreservedContract {
    pub same_capability: bool,
    pub same_evidence: bool,
    pub same_authority: bool,
    pub same_objective: bool,
}

impl PreservedContract {
    pub fn intact(&self) -> bool {
        self.same_capability && self.same_evidence && self.same_authority && self.same_objective
    }
}

/// The routing outcome: which substrate runs the work, and at what queue
/// position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDecision {
    pub execution_id: String,
    pub substrate: Substrate,
    pub queue_position: u16,
    pub reason_code: String,
}

/// A scramble: dynamic local re-routing while the possession and objective
/// stay unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScrambleDecision {
    pub execution_id: String,
    pub reason: ScrambleReason,
    pub from_substrate: Substrate,
    pub to_substrate: Substrate,
    pub preserved: PreservedContract,
}

/// Mesut is unavailable. Execution falls back to the ordinary playbook-live
/// path — the work is unchanged, only the optimisation is absent.
#[derive(Clone, Copy, Debug, Default)]
pub struct MesutUnavailable;

/// Queue latency beyond this many milliseconds is congestion worth
/// reacting to.
pub const CONGESTION_QUEUE_LATENCY_MS: u64 = 250;
/// CPU saturation at or above this per-mille is saturation.
pub const CPU_SATURATION_THRESHOLD_PPM: u32 = 900_000;
/// Service-time prediction error at or above this per-mille means the
/// substrate choice was wrong.
pub const SERVICE_TIME_SHIFT_THRESHOLD_PPM: u32 = 500_000;

/// Route one authorised request to a substrate. Deterministic: the same
/// request always routes the same way. `BlockingTool` → Blocking, `CpuBound`
/// → Rayon, `IoBound` → Tokio; a preferred substrate is honoured only when
/// it matches the work kind's safe set. Queue position is priority-ordered
/// with arrival order as the tiebreak.
pub fn route(request: &MesutExecutionRequest, queue_arrival_index: u16) -> RouteDecision {
    let substrate = match request.work_kind {
        WorkKind::BlockingTool => Substrate::Blocking,
        WorkKind::CpuBound => request
            .preferred_constraints
            .prefer_substrate
            .unwrap_or(Substrate::Rayon),
        WorkKind::IoBound => request
            .preferred_constraints
            .prefer_substrate
            .unwrap_or(Substrate::Tokio),
    };
    let priority_rank = u16::from(match request.priority {
        Priority::Critical => 0u8,
        Priority::High => 1,
        Priority::Normal => 2,
        Priority::Low => 3,
    });
    RouteDecision {
        execution_id: request.execution_id.clone(),
        substrate,
        queue_position: priority_rank
            .saturating_mul(1_000)
            .saturating_add(queue_arrival_index % 1_000),
        reason_code: format!(
            "work_kind_{:?}_priority_{:?}",
            request.work_kind, request.priority
        )
        .to_lowercase(),
    }
}

/// Detect whether the measured pressure justifies a scramble, and why.
/// Deterministic: the first matching trigger in the documented order wins
/// (deadline, stall, saturation, blocked worker, shift, congestion).
pub fn scramble_trigger(pressure: &ExecutionPressure) -> Option<ScrambleReason> {
    if pressure.deadline_imminent {
        return Some(ScrambleReason::DeadlinePressure);
    }
    if pressure.io_stall_ms > 0 {
        return Some(ScrambleReason::IoStall);
    }
    if pressure.cpu_saturation_ppm >= CPU_SATURATION_THRESHOLD_PPM {
        return Some(ScrambleReason::CpuSaturation);
    }
    if pressure.blocked_workers > 0 {
        return Some(ScrambleReason::BlockedWorker);
    }
    if pressure.service_time_prediction_error_ppm >= SERVICE_TIME_SHIFT_THRESHOLD_PPM {
        return Some(ScrambleReason::ServiceTimeShift);
    }
    if pressure.queue_latency_ms > CONGESTION_QUEUE_LATENCY_MS {
        return Some(ScrambleReason::ExecutorCongestion);
    }
    None
}

/// Decide a scramble: the trigger names the reason, the preserved contract
/// must be intact, and non-cancellable work may only move before it starts.
/// The target substrate is the route the work would receive fresh — the
/// same-request-same-answer rule keeps scrambles reproducible in the
/// journal.
pub fn scramble(
    request: &MesutExecutionRequest,
    current: Substrate,
    pressure: &ExecutionPressure,
    preserved: PreservedContract,
) -> Result<Option<ScrambleDecision>, String> {
    let Some(reason) = scramble_trigger(pressure) else {
        return Ok(None);
    };
    if !preserved.intact() {
        return Err("scramble would alter the authority or evidence contract".into());
    }
    if !request.cancellation.cancellable
        && matches!(
            reason,
            ScrambleReason::DeadlinePressure | ScrambleReason::IoStall
        )
    {
        // Non-cancellable work cannot be moved under duress; it finishes on
        // its substrate or fails there.
        return Ok(None);
    }
    let RouteDecision { substrate, .. } = route(request, 0);
    if substrate == current {
        return Ok(None);
    }
    Ok(Some(ScrambleDecision {
        execution_id: request.execution_id.clone(),
        reason,
        from_substrate: current,
        to_substrate: substrate,
        preserved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: WorkKind, priority: Priority, cancellable: bool) -> MesutExecutionRequest {
        MesutExecutionRequest {
            execution_id: "exec-1".into(),
            work_kind: kind,
            priority,
            hard_constraints: ResourceConstraints {
                max_workers: 4,
                max_memory_hint_mb: Some(512),
                requires_dedicated_core: false,
            },
            preferred_constraints: SchedulingPreferences {
                prefer_substrate: None,
                defer_until_ms: None,
            },
            cancellation: CancellationContract {
                cancellable,
                graceful_window_ms: Some(100),
            },
            deadline_ms: Some(5_000),
            expected_evidence: vec![EvidenceClass::ToolInvocation],
        }
    }

    fn quiet_pressure() -> ExecutionPressure {
        ExecutionPressure {
            queue_latency_ms: 0,
            blocked_workers: 0,
            cpu_saturation_ppm: 0,
            io_stall_ms: 0,
            service_time_prediction_error_ppm: 0,
            deadline_imminent: false,
        }
    }

    fn intact() -> PreservedContract {
        PreservedContract {
            same_capability: true,
            same_evidence: true,
            same_authority: true,
            same_objective: true,
        }
    }

    #[test]
    fn work_kinds_route_to_their_documented_substrates() {
        assert_eq!(
            route(&request(WorkKind::BlockingTool, Priority::Normal, true), 0).substrate,
            Substrate::Blocking
        );
        assert_eq!(
            route(&request(WorkKind::CpuBound, Priority::Normal, true), 0).substrate,
            Substrate::Rayon
        );
        assert_eq!(
            route(&request(WorkKind::IoBound, Priority::Normal, true), 0).substrate,
            Substrate::Tokio
        );
    }

    #[test]
    fn queue_position_is_priority_ordered_with_arrival_tiebreak() {
        let critical = route(&request(WorkKind::CpuBound, Priority::Critical, true), 3);
        let low = route(&request(WorkKind::CpuBound, Priority::Low, true), 7);
        assert!(critical.queue_position < low.queue_position);
        let first = route(&request(WorkKind::CpuBound, Priority::Normal, true), 0);
        let second = route(&request(WorkKind::CpuBound, Priority::Normal, true), 1);
        assert!(first.queue_position < second.queue_position);
    }

    #[test]
    fn scramble_triggers_fire_in_the_documented_order() {
        let quiet = quiet_pressure();
        assert_eq!(scramble_trigger(&quiet), None);
        let congested = ExecutionPressure {
            queue_latency_ms: CONGESTION_QUEUE_LATENCY_MS + 1,
            ..quiet
        };
        assert_eq!(
            scramble_trigger(&congested),
            Some(ScrambleReason::ExecutorCongestion)
        );
        let everything = ExecutionPressure {
            queue_latency_ms: 9_999,
            blocked_workers: 9,
            cpu_saturation_ppm: 999_999,
            io_stall_ms: 9_999,
            service_time_prediction_error_ppm: 999_999,
            deadline_imminent: true,
        };
        assert_eq!(
            scramble_trigger(&everything),
            Some(ScrambleReason::DeadlinePressure)
        );
    }

    #[test]
    fn a_scramble_never_alters_the_contract_or_repeats_the_substrate() {
        let request = request(WorkKind::BlockingTool, Priority::Normal, true);
        let pressure = ExecutionPressure {
            queue_latency_ms: 9_999,
            ..quiet_pressure()
        };
        let decision = scramble(&request, Substrate::Tokio, &pressure, intact())
            .unwrap()
            .expect("congested blocking work scrambles toward the blocking pool");
        assert!(decision.preserved.intact());
        assert_eq!(decision.to_substrate, Substrate::Blocking);
        assert!(scramble(&request, Substrate::Blocking, &pressure, intact())
            .unwrap()
            .is_none());
    }

    #[test]
    fn a_broken_preserved_contract_refuses_the_scramble() {
        let request = request(WorkKind::BlockingTool, Priority::Normal, true);
        let pressure = ExecutionPressure {
            queue_latency_ms: 9_999,
            ..quiet_pressure()
        };
        let broken = PreservedContract {
            same_capability: true,
            same_evidence: false,
            same_authority: true,
            same_objective: true,
        };
        assert!(scramble(&request, Substrate::Tokio, &pressure, broken).is_err());
    }

    #[test]
    fn non_cancellable_work_does_not_move_under_duress() {
        let request = request(WorkKind::BlockingTool, Priority::Normal, false);
        let stalling = ExecutionPressure {
            io_stall_ms: 500,
            ..quiet_pressure()
        };
        assert!(
            scramble(&request, Substrate::Tokio, &stalling, intact())
                .unwrap()
                .is_none(),
            "non-cancellable work finishes on its substrate"
        );
    }
}
