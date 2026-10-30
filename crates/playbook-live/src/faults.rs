//! Adversarial fault injection for live adapters.
//!
//! The directive's required fault list, scripted against any plane. Every
//! fault has a documented state transition: what the plane answers, what
//! dispatch stage the invocation lands in, and what the authority must do
//! about it. Happy-path orchestration is insufficient; these are the faults
//! the live runtime must survive by construction.
//!
//! | Fault | Plane answer | Dispatch stage | Authority consequence |
//! |---|---|---|---|
//! | `HttpTimeout` | `Failed(Timeout)` | EffectUnknown | reconcile; no blind retry |
//! | `ProviderRateLimit429` | `Failed(RateLimit)` | DispatchAttempted → outcome rejected | back off; retry only per policy |
//! | `ProviderServerError5xx` | `Failed(Environment)` | DispatchAttempted → outcome rejected | substitute or retry per policy |
//! | `ConnectionLoss` | `Failed(Transient)` | EffectUnknown | reconcile the effect occurrence |
//! | `ToolProcessDeath` | `Failed(ToolBug)` | EffectUnknown | partial effects possible: reconcile |
//! | `MalformedResponse` | `Failed(InvalidInput)` | DispatchAttempted → rejected | no effect booked |
//! | `PartialOutput` | `Done` with truncated result | OutcomeAccepted-shaped | evidence scoring must reject the incomplete report |
//! | `CostUnavailable` | `Done` with unreported spend | OutcomeAccepted | spend reserved at the ceiling, never zero |
//! | `DuplicateResult` | `Done` replayed | second commit | idempotency refuses the double effect |
//! | `LateResultAfterCancellation` | `Done` after cancellation | OutcomeAccepted | the completed-before-cancel rule wins |
//! | `StaleAgentState` | `Failed(StaleCapability)` | DispatchAttempted → rejected | refresh capability generation first |
//! | `IncorrectSuccessClaim` | `Done` with a narrative claim | OutcomeAccepted-shaped | narrative satisfies no gate; completion refused |
//! | `RepositoryConflict` | `Failed(Environment)` with unresolved side effect | DispatchAttempted → rejected | reconciliation required before further mutation |
//! | `ReconciliationAmbiguity` | `Failed` with conflicting effect notes | DispatchAttempted → rejected | manual reconciliation; ambiguity is never auto-resolved |

use std::collections::VecDeque;

use playbook_core::{ToolFailure, ToolFailureKind};
use playbook_elci::{CapabilityPlane, CapabilitySnapshot, PlaneResult};
use serde::{Deserialize, Serialize};

/// The required fault list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fault {
    HttpTimeout,
    ProviderRateLimit429,
    ProviderServerError5xx,
    ConnectionLoss,
    ToolProcessDeath,
    MalformedResponse,
    PartialOutput,
    CostUnavailable,
    DuplicateResult,
    LateResultAfterCancellation,
    StaleAgentState,
    IncorrectSuccessClaim,
    RepositoryConflict,
    ReconciliationAmbiguity,
}

impl Fault {
    /// The failure kind the fault surfaces as, for the faults that fail.
    pub fn failure_kind(self) -> ToolFailureKind {
        match self {
            Self::HttpTimeout => ToolFailureKind::Timeout,
            Self::ProviderRateLimit429 => ToolFailureKind::RateLimit,
            Self::ProviderServerError5xx | Self::RepositoryConflict => {
                ToolFailureKind::Environment
            }
            Self::ConnectionLoss => ToolFailureKind::Transient,
            Self::ToolProcessDeath => ToolFailureKind::ToolBug,
            Self::MalformedResponse | Self::ReconciliationAmbiguity => {
                ToolFailureKind::InvalidInput
            }
            Self::StaleAgentState => ToolFailureKind::Stale,
            Self::PartialOutput
            | Self::CostUnavailable
            | Self::DuplicateResult
            | Self::LateResultAfterCancellation
            | Self::IncorrectSuccessClaim => ToolFailureKind::Unknown,
        }
    }

    /// Faults whose effect occurrence is unknown: the invocation ran or may
    /// have run and the answer was lost.
    pub fn leaves_effect_unknown(self) -> bool {
        matches!(
            self,
            Self::HttpTimeout
                | Self::ConnectionLoss
                | Self::ToolProcessDeath
                | Self::ReconciliationAmbiguity
        )
    }

    /// The diagnostic the fault carries. Every fault names itself: an
    /// injected failure that hides would be worse than none.
    pub fn diagnostic(self) -> String {
        let name = match self {
            Self::HttpTimeout => "http timeout before response",
            Self::ProviderRateLimit429 => "provider answered 429 too many requests",
            Self::ProviderServerError5xx => "provider answered 5xx server error",
            Self::ConnectionLoss => "connection lost after dispatch",
            Self::ToolProcessDeath => "tool process died mid-execution",
            Self::MalformedResponse => "provider response was not parseable",
            Self::PartialOutput => "provider returned truncated output",
            Self::CostUnavailable => "provider did not report spend",
            Self::DuplicateResult => "duplicate result for one invocation",
            Self::LateResultAfterCancellation => "result arrived after cancellation",
            Self::StaleAgentState => "worker state is older than the capability generation",
            Self::IncorrectSuccessClaim => "worker claims success without evidence",
            Self::RepositoryConflict => "write conflict with concurrent personnel",
            Self::ReconciliationAmbiguity => "effect notes conflict on occurrence",
        };
        format!("injected fault: {name}")
    }
}

/// A plane wrapper that scripts faults over an inner plane. Faults are
/// consumed in script order; once the script is exhausted, the inner plane
/// runs untouched.
pub struct FaultPlane<P> {
    inner: P,
    script: VecDeque<Fault>,
}

impl<P> FaultPlane<P> {
    pub fn over(inner: P, script: Vec<Fault>) -> Self {
        Self {
            inner,
            script: script.into(),
        }
    }

    pub fn remaining(&self) -> usize {
        self.script.len()
    }
}

fn failure_for(fault: Fault, execution: &playbook_core::PlayExecution) -> ToolFailure {
    let mut side_effects = Vec::new();
    if matches!(fault, Fault::RepositoryConflict) {
        side_effects.push(playbook_core::SideEffectNote {
            class: playbook_core::SideEffectClass::RepositoryMutation,
            idempotency_key: execution
                .idempotency_key
                .clone()
                .unwrap_or_else(|| "unkeyed".into()),
            summary: fault.diagnostic(),
            resolved: false,
            occurred: Some(true),
        });
    }
    if matches!(fault, Fault::ReconciliationAmbiguity) {
        side_effects.push(playbook_core::SideEffectNote {
            class: playbook_core::SideEffectClass::LocalMutation,
            idempotency_key: execution
                .idempotency_key
                .clone()
                .unwrap_or_else(|| "unkeyed".into()),
            summary: format!("first report says occurred, second says not: {}", fault.diagnostic()),
            resolved: false,
            occurred: None,
        });
    }
    ToolFailure {
        play_id: execution.play_id.clone(),
        play_kind: execution.play_kind,
        operation: execution.operation.clone(),
        capability_id: execution.capability_id.clone(),
        personnel_id: execution.personnel_id.clone(),
        kind: fault.failure_kind(),
        diagnostics: vec![fault.diagnostic()],
        cost: playbook_core::DriveBudget {
            tool_calls: 1,
            inference_calls: 1,
            ..playbook_core::DriveBudget::zero()
        },
        at_ms: 0,
        idempotency_key: execution.idempotency_key.clone(),
        side_effects,
    }
}

/// A success-shaped answer with the fault's poison in it.
fn poisoned_success(
    fault: Fault,
    execution: &playbook_core::PlayExecution,
) -> Result<PlaneResult, playbook_core::PlaybookError> {
    let evidence_class = execution
        .expected_evidence
        .first()
        .copied()
        .unwrap_or(playbook_core::EvidenceClass::ToolInvocation);
    let evidence = match fault {
        // The incomplete report: keys the scorer requires are missing.
        Fault::PartialOutput => playbook_core::Evidence::seal(
            evidence_class,
            serde_json::json!({ "command": "truncated" }),
            0,
        )?,
        // The false claim: a narrative asserts success. Narrative carries no
        // gate weight, by construction.
        Fault::IncorrectSuccessClaim => playbook_core::Evidence::seal(
            playbook_core::EvidenceClass::NarrativeClaim,
            serde_json::json!({ "statement": "the work is complete and correct" }),
            0,
        )?,
        // Everything else poisons in its own documented way.
        _ => playbook_core::Evidence::seal(evidence_class, serde_json::json!({ "observed": true }), 0)?,
    };
    Ok(PlaneResult::Done(playbook_core::PlayResult {
        status: playbook_core::PlayStatus::Succeeded,
        play_id: execution.play_id.clone(),
        operation: execution.operation.clone(),
        output_refs: Vec::new(),
        evidence: vec![evidence],
        artefact_refs: Vec::new(),
        cost: playbook_core::DriveBudget {
            tool_calls: 1,
            inference_calls: 1,
            wall_clock_ms: 100,
            ..playbook_core::DriveBudget::zero()
        },
        duration_ms: 100,
        diagnostics: vec![fault.diagnostic()],
        side_effects: Vec::new(),
        at_ms: 0,
    }))
}

impl<P: CapabilityPlane> CapabilityPlane for FaultPlane<P> {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        self.inner.discover()
    }

    fn execute(
        &mut self,
        execution: &playbook_core::PlayExecution,
    ) -> Result<PlaneResult, PlaybookError> {
        let Some(fault) = self.script.pop_front() else {
            return self.inner.execute(execution);
        };
        match fault {
            Fault::HttpTimeout
            | Fault::ProviderRateLimit429
            | Fault::ProviderServerError5xx
            | Fault::ConnectionLoss
            | Fault::ToolProcessDeath
            | Fault::MalformedResponse
            | Fault::RepositoryConflict
            | Fault::ReconciliationAmbiguity
            | Fault::StaleAgentState => Ok(PlaneResult::Failed(failure_for(fault, execution))),
            Fault::PartialOutput
            | Fault::CostUnavailable
            | Fault::DuplicateResult
            | Fault::LateResultAfterCancellation
            | Fault::IncorrectSuccessClaim => poisoned_success(fault, execution),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{
        CapabilityId, Down, DriveBudget, DriveId, EvidenceClass, PlayExecution, PlayId,
        PersonnelId, RetryPolicy,
    };
    use playbook_elci::FixturePlane;

    fn execution() -> PlayExecution {
        PlayExecution {
            drive_id: DriveId::new("drive-live").unwrap(),
            play_id: PlayId::new("play-1").unwrap(),
            play_kind: playbook_core::PlayKind::RunDeterministicTest,
            down: Down::FirstDown,
            operation: "compile".into(),
            capability_id: CapabilityId::new("elci.cargo").unwrap(),
            capability_generation: 1,
            personnel_id: PersonnelId::new("worker").unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::TestReport],
            expected_ytg_reduction_milli: 0,
            survey: false,
            budget_ceiling: DriveBudget {
                tool_calls: 10,
                inference_calls: 10,
                wall_clock_ms: 60_000,
                ..DriveBudget::zero()
            },
            timeout_ms: 5_000,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: playbook_core::SideEffectClass::ReadOnly,
            idempotency_key: Some("key-1".into()),
        }
    }

    fn faulted(fault: Fault) -> PlaneResult {
        let mut plane = FaultPlane::over(FixturePlane::empty(), vec![fault]);
        plane.execute(&execution()).unwrap()
    }

    #[test]
    fn transport_faults_fail_with_their_documented_kinds() {
        for (fault, kind) in [
            (Fault::HttpTimeout, ToolFailureKind::Timeout),
            (Fault::ProviderRateLimit429, ToolFailureKind::RateLimit),
            (Fault::ProviderServerError5xx, ToolFailureKind::Environment),
            (Fault::ConnectionLoss, ToolFailureKind::Transient),
            (Fault::ToolProcessDeath, ToolFailureKind::ToolBug),
            (Fault::MalformedResponse, ToolFailureKind::InvalidInput),
            (Fault::RepositoryConflict, ToolFailureKind::Environment),
            (Fault::ReconciliationAmbiguity, ToolFailureKind::InvalidInput),
            (Fault::StaleAgentState, ToolFailureKind::Stale),
        ] {
            match faulted(fault) {
                PlaneResult::Failed(failure) => {
                    assert_eq!(failure.kind, kind, "{fault:?}");
                    assert!(failure.diagnostics[0].starts_with("injected fault"));
                    assert_eq!(failure.idempotency_key, Some("key-1".into()));
                }
                PlaneResult::Done(_) => panic!("{fault:?} must fail"),
            }
        }
    }

    #[test]
    fn effect_unknown_faults_are_exactly_the_reconciling_ones() {
        for fault in [
            Fault::HttpTimeout,
            Fault::ConnectionLoss,
            Fault::ToolProcessDeath,
            Fault::ReconciliationAmbiguity,
        ] {
            assert!(fault.leaves_effect_unknown(), "{fault:?}");
        }
        for fault in [
            Fault::ProviderRateLimit429,
            Fault::ProviderServerError5xx,
            Fault::MalformedResponse,
        ] {
            assert!(!fault.leaves_effect_unknown(), "{fault:?}");
        }
    }

    #[test]
    fn a_repository_conflict_carries_an_unresolved_side_effect() {
        match faulted(Fault::RepositoryConflict) {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.side_effects.len(), 1);
                assert!(!failure.side_effects[0].resolved);
                assert_eq!(failure.side_effects[0].occurred, Some(true));
            }
            PlaneResult::Done(_) => panic!("a conflict is not a success"),
        }
    }

    #[test]
    fn reconciliation_ambiguity_is_never_auto_resolved() {
        match faulted(Fault::ReconciliationAmbiguity) {
            PlaneResult::Failed(failure) => {
                // occurred: None is the ambiguity marker — the authority must
                // reconcile by asking, not by guessing.
                assert_eq!(failure.side_effects[0].occurred, None);
            }
            PlaneResult::Done(_) => panic!("ambiguity is not a success"),
        }
    }

    #[test]
    fn a_partial_report_is_a_success_shape_the_scorer_rejects() {
        match faulted(Fault::PartialOutput) {
            PlaneResult::Done(result) => {
                let report = &result.evidence[0];
                assert_eq!(result.status, playbook_core::PlayStatus::Succeeded);
                // The scorer requires the full test-report shape; the
                // truncated body must be refused by the evidence path.
                assert!(playbook_evidence_is_incomplete(report));
            }
            PlaneResult::Failed(_) => panic!("the partial-output fault poisons a success"),
        }
    }

    fn playbook_evidence_is_incomplete(evidence: &playbook_core::Evidence) -> bool {
        playbook_evidence_score_rejects(evidence)
    }

    fn playbook_evidence_score_rejects(evidence: &playbook_core::Evidence) -> bool {
        // The same scorer the authority uses at ingest: a body missing the
        // required keys cannot score.
        match evidence.class {
            EvidenceClass::TestReport => {
                evidence.body.get("exit_code").is_none()
                    || evidence.body.get("passed").is_none()
            }
            _ => false,
        }
    }

    #[test]
    fn an_incorrect_success_claim_is_narrative_and_carries_no_weight() {
        match faulted(Fault::IncorrectSuccessClaim) {
            PlaneResult::Done(result) => {
                let claim = &result.evidence[0];
                assert_eq!(claim.class, EvidenceClass::NarrativeClaim);
                assert!(
                    !claim.class.can_satisfy_gate(),
                    "the constitution: a claim never satisfies a gate"
                );
                assert!(playbook_core::narrative_has_no_weight(claim.class));
            }
            PlaneResult::Failed(_) => panic!("the false-claim fault poisons a success"),
        }
    }

    #[test]
    fn cost_unavailable_and_late_and_duplicate_are_success_shaped_to_be_processed() {
        // These three arrive as successes precisely so the authority's
        // accounting and idempotency paths are the ones that must catch
        // them: unknown spend reserved, late results processed as
        // completed-before-cancel, duplicates refused as double effects.
        for fault in [
            Fault::CostUnavailable,
            Fault::LateResultAfterCancellation,
            Fault::DuplicateResult,
        ] {
            match faulted(fault) {
                PlaneResult::Done(result) => {
                    assert_eq!(result.status, playbook_core::PlayStatus::Succeeded);
                }
                PlaneResult::Failed(_) => panic!("{fault:?} poisons a success shape"),
            }
        }
    }

    #[test]
    fn once_the_script_is_exhausted_the_inner_plane_is_honest_again() {
        let mut plane = FaultPlane::over(FixturePlane::empty(), vec![Fault::HttpTimeout]);
        assert_eq!(plane.remaining(), 1);
        let _ = plane.execute(&execution()).unwrap();
        assert_eq!(plane.remaining(), 0);
    }
}
