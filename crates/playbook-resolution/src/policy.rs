//! The remediation policy engine (§8): classification and prescription
//! are separated. The classifier answers what happened; this engine
//! answers what actions are authorised — under budgets, loop guards and
//! the constitutional ceiling that no action may touch the truth
//! conditions of completion.

use serde::{Deserialize, Serialize};

use crate::classify::{Confidence, Diagnosis, RefusalCause};

/// The authorised repair actions (§3.3: execution conditions only).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Normalise the brief contract: strip wrappers, express requirements
    /// machine-checkably (§9.1 — never by weakening validators).
    NormaliseBrief,
    /// Expand the token envelope (§10: never blindly — see the controller).
    ExpandEnvelope,
    /// Split an oversized work unit into independently verifiable children
    /// (§11), acceptance standard unchanged.
    DecomposeWork,
    /// Impose first/last/cardinality anchors on the work unit (§12).
    ImposeAnchors,
    /// Assign exclusive artefact ownership (§13).
    AssignOwnership,
    /// Substitute the player from the bench (§20) — same goal line.
    SubstitutePlayer,
    /// Change provider for the same model family (§20).
    ChangeProvider,
    /// Plain bounded retry (§41: not remediation — consumes retry budget).
    BoundedRetry,
    /// Escalate: no authorised autonomous repair exists (§55).
    Escalate,
}

impl Action {
    pub fn key(self) -> &'static str {
        match self {
            Self::NormaliseBrief => "normalise_brief",
            Self::ExpandEnvelope => "expand_envelope",
            Self::DecomposeWork => "decompose_work",
            Self::ImposeAnchors => "impose_anchors",
            Self::AssignOwnership => "assign_ownership",
            Self::SubstitutePlayer => "substitute_player",
            Self::ChangeProvider => "change_provider",
            Self::BoundedRetry => "bounded_retry",
            Self::Escalate => "escalate",
        }
    }

    /// Escalation is the only action that leaves the autonomous loop (§55:
    /// autonomy includes knowing when not to continue).
    pub fn is_escalation(self) -> bool {
        matches!(self, Self::Escalate)
    }
}

/// The remediation budget (§43): remediation never quietly consumes
/// unbounded inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionBudget {
    pub max_remediation_attempts: u32,
    pub max_tokens: u64,
    pub max_wall_time_ms: u64,
}

impl Default for ResolutionBudget {
    fn default() -> Self {
        Self {
            max_remediation_attempts: 4,
            max_tokens: 40_000,
            max_wall_time_ms: 600_000,
        }
    }
}

/// The policy regime: the minimum confidence autonomous repair requires,
/// and the knobs the engine may turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionPolicy {
    /// Automatic remediation requires at least this confidence (§7).
    pub min_confidence: Confidence,
    pub budget: ResolutionBudget,
    /// The current token envelope, for the controller's expand decision.
    pub token_envelope: u64,
}

impl Default for ResolutionPolicy {
    fn default() -> Self {
        Self {
            min_confidence: Confidence::High,
            budget: ResolutionBudget::default(),
            token_envelope: 2_000,
        }
    }
}

/// The prescription: authorised actions with the reason, and whether the
/// budget/loop guards permit them at all. The prescription never executes
/// itself and never mutates state (§8).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prescription {
    pub actions: Vec<Action>,
    /// The recommended envelope if ExpandEnvelope is authorised.
    pub suggested_envelope: Option<u64>,
    /// The recommended child count if DecomposeWork is authorised.
    pub suggested_chunks: Option<u32>,
    pub rationale: String,
    pub escalated: bool,
}

/// Prescribe the remediation for a diagnosis. Mechanical: same diagnosis
/// and state always yield the same prescription. Loop prevention (§42)
/// and budget exhaustion (§43) escalate rather than loop.
pub fn prescribe(
    diagnosis: &Diagnosis,
    policy: &ResolutionPolicy,
    attempts_so_far: u32,
    tokens_spent: u64,
    identical_state_seen: bool,
) -> Prescription {
    let budget_exhausted = attempts_so_far >= policy.budget.max_remediation_attempts
        || tokens_spent >= policy.budget.max_tokens;
    // §42: materially equivalent state twice means the loop is closed.
    if identical_state_seen && attempts_so_far >= 2 {
        return Prescription {
            actions: vec![Action::Escalate],
            suggested_envelope: None,
            suggested_chunks: None,
            rationale: "materially equivalent execution state recurred without progress (§42)"
                .into(),
            escalated: true,
        };
    }
    if budget_exhausted {
        return Prescription {
            actions: vec![Action::Escalate],
            suggested_envelope: None,
            suggested_chunks: None,
            rationale: "remediation budget exhausted (§43)".into(),
            escalated: true,
        };
    }
    let autonomous = diagnosis.confidence >= policy.min_confidence;
    let require_confidence = |action: Action| -> Vec<Action> {
        if autonomous {
            vec![action]
        } else {
            vec![Action::BoundedRetry, Action::Escalate]
        }
    };
    let (actions, envelope, chunks, rationale) = match diagnosis.primary {
        RefusalCause::BriefRigidity => (
            require_confidence(Action::NormaliseBrief),
            None,
            None,
            "the brief's wrapper rules own the mismatch: normalise the contract, keep the validator",
        ),
        RefusalCause::OutputTruncation => {
            // §10: choose between more room and smaller work. Reasoning
            // models burn hidden budgets, so expansion alone is not
            // automatically Pareto-efficient — decomposition is offered
            // when the work unit is large.
            let expand = Action::ExpandEnvelope;
            let decompose = Action::DecomposeWork;
            (
                {
                    let mut both = require_confidence(decompose);
                    for action in require_confidence(expand) {
                        if !both.contains(&action) {
                            both.push(action);
                        }
                    }
                    both
                },
                Some(policy.token_envelope.saturating_mul(4)),
                Some(8),
                "truncation: expand the envelope or shrink the unit — both re-run the unchanged gate",
            )
        }
        RefusalCause::WorkUnitTooLarge => (
            require_confidence(Action::DecomposeWork),
            None,
            Some(8),
            "identical failures across attempts mark the unit, not the attempt (§41)",
        ),
        RefusalCause::AnchorViolation => (
            require_confidence(Action::ImposeAnchors),
            None,
            None,
            "sequence integrity failed: impose the anchor contract",
        ),
        RefusalCause::OutputOwnershipCollision => (
            require_confidence(Action::AssignOwnership),
            None,
            None,
            "exclusive ownership resolves the collision; silent overwrite is forbidden",
        ),
        RefusalCause::ArtefactIncomplete => (
            require_confidence(Action::NormaliseBrief),
            Some(policy.token_envelope.saturating_mul(2)),
            None,
            "an incomplete attempt usually means envelope or brief mismatch",
        ),
        RefusalCause::RateLimited | RefusalCause::ProviderFailure => (
            if autonomous {
                vec![Action::ChangeProvider, Action::BoundedRetry]
            } else {
                vec![Action::BoundedRetry, Action::Escalate]
            },
            None,
            None,
            "provider-side failure: route around it; never recorded as model incapability (§17)",
        ),
        RefusalCause::VersionControlFailure => (
            vec![Action::Escalate],
            None,
            None,
            "the authority chain itself failed (§34): reconcile externally, never re-assert",
        ),
        RefusalCause::Unknown => (
            vec![Action::Escalate],
            None,
            None,
            "no deterministic cause: human or specialist diagnosis (§7)",
        ),
        _ => (
            require_confidence(Action::BoundedRetry),
            None,
            None,
            "bounded retry within the same conditions",
        ),
    };
    // Escalation is not a flag bolted onto a repair: it means NO
    // autonomous repair remains (bounded retry plus escalation is still
    // an escalation, §41/§55).
    let escalated = actions
        .iter()
        .all(|a| matches!(a, Action::Escalate | Action::BoundedRetry));
    Prescription {
        actions,
        suggested_envelope: envelope,
        suggested_chunks: chunks,
        rationale: rationale.into(),
        escalated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::{classify, RefusalObservation};

    fn observation() -> RefusalObservation {
        RefusalObservation {
            finish_reason: None,
            provider_status: None,
            exit_code: None,
            gate_failures: Vec::new(),
            output_structurally_complete: true,
            artefact_expected_but_missing: false,
            output_path_contended: false,
            version_control_failed: false,
            prior_attempts_with_same_failures: 0,
            semantic_requirements_met_after_normalisation: None,
            first_required_item_missing: None,
        }
    }

    #[test]
    fn truncation_prescribes_expand_or_decompose_never_gate_weakening() {
        let mut obs = observation();
        obs.finish_reason = Some("length".into());
        obs.output_structurally_complete = false;
        let diagnosis = classify(&obs);
        assert_eq!(diagnosis.primary, crate::RefusalCause::OutputTruncation);
        assert_eq!(diagnosis.confidence, Confidence::Certain);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 0, 0, false);
        assert!(prescription.actions.contains(&Action::ExpandEnvelope));
        assert!(prescription.actions.contains(&Action::DecomposeWork));
        assert!(prescription.suggested_envelope.unwrap() > 2_000);
    }

    #[test]
    fn the_loop_guard_escalates_instead_of_looping() {
        let mut obs = observation();
        obs.gate_failures = vec!["missing: x".into()];
        obs.prior_attempts_with_same_failures = 3;
        let diagnosis = classify(&obs);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 3, 0, true);
        assert!(prescription.escalated);
        assert_eq!(prescription.actions, vec![Action::Escalate]);
    }

    #[test]
    fn budget_exhaustion_escalates() {
        let obs = observation();
        let diagnosis = classify(&obs);
        assert!(!diagnosis.deterministic);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 9, 0, false);
        assert!(prescription.escalated, "budget exhaustion is an escalation");
    }

    #[test]
    fn provider_failure_is_never_recorded_as_model_incapacity() {
        let mut obs = observation();
        obs.provider_status = Some("http 503".into());
        obs.output_structurally_complete = false;
        let diagnosis = classify(&obs);
        assert_eq!(diagnosis.primary, RefusalCause::ProviderFailure);
        assert_eq!(diagnosis.primary.family(), crate::RefusalFamily::Execution);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 0, 0, false);
        assert!(prescription.actions.contains(&Action::ChangeProvider));
        assert!(!prescription.actions.contains(&Action::NormaliseBrief));
    }

    #[test]
    fn version_control_failure_always_escalates() {
        let mut obs = observation();
        obs.version_control_failed = true;
        let diagnosis = classify(&obs);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 0, 0, false);
        assert_eq!(prescription.actions, vec![Action::Escalate]);
        assert_eq!(diagnosis.primary.family(), crate::RefusalFamily::Governance);
    }

    #[test]
    fn low_confidence_diagnoses_do_not_autonomously_repair() {
        let obs = observation();
        let diagnosis = classify(&obs);
        assert_eq!(diagnosis.confidence, Confidence::Unknown);
        let prescription = prescribe(&diagnosis, &ResolutionPolicy::default(), 0, 0, false);
        assert!(prescription.actions.contains(&Action::Escalate));
        assert!(!prescription.actions.contains(&Action::NormaliseBrief));
    }
}
