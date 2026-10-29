//! Huddles: budgeted, policy-admitted synchronization barriers.
//!
//! A huddle is invoked when the expected cost of team-state divergence
//! exceeds the expected cost of cross-agent reconciliation. It exists to
//! prevent concurrently operating personnel from developing incompatible
//! internal representations of the task, and to keep communication rare,
//! deliberate, hierarchical, evidence-bound and economically justified.
//!
//! The rules this crate owns:
//!
//! * synchronize only when necessary — condition-triggered, budget-aware,
//!   scope-limited, evidence-grounded;
//! * local before global — escalation only up the scope ladder, only with
//!   justification;
//! * evidence outranks consensus — no huddle outcome can satisfy a gate,
//!   erase a contradiction, or rewrite the measured field;
//! * bounded context dissemination — participants receive scoped packets,
//!   never the drive transcript;
//! * hard budgets — exhaustion forces synthesis; no huddle extends itself.
//!
//! Persistence, event sourcing, and the drive fold live in `playbook-state`,
//! which projects drive events onto [`apply::HuddleEvent`] and folds them
//! through [`apply::apply_huddle`].

#![forbid(unsafe_code)]

pub mod admission;
pub mod apply;
pub mod budget;
pub mod domain;
pub mod escalation;
pub mod participants;
pub mod reconcile;
pub mod trigger;

pub use admission::{AdmissionDecision, AdmissionInputs, HuddleValue};
pub use apply::{apply_huddle, fold_huddle, DriveView, HuddleEvent};
pub use domain::{
    ActiveHuddle, Assignment, AssignmentChange, AssumptionDelta, Blocker, ClaimRef, CoherenceState,
    Contradiction, ContradictionId, ContradictionResolution, ContradictionSeverity, DebtBand,
    DependencyDelta, DisclosureProfile, EvidenceSummary, GateSnapshot, HuddleClosure,
    HuddleCondition, HuddleContextPacket, HuddleCost, HuddleLedger, HuddleOutcome,
    HuddleParticipant, HuddlePhase, HuddleRecord, HuddleReport, HuddleRequestBody, HuddleRole,
    HuddleScope, HuddleTrigger, HuddleTriggerKind, ParticipantBudget, PersonnelAction,
    PersonnelChange, PlayProposal, ResolutionState, SharedStateDelta, StrategyChange,
};
pub use escalation::escalation_target;
pub use participants::{
    build_packet, dispatch_packet, participation_score, role_for, select, DefaultRegulator,
    HuddleRegulator, ParticipantCandidate,
};
pub use reconcile::{
    detect_contradictions, forced_synthesis, normalize_observation, normalize_report,
    try_tov_normalize_report, NormalizedOutcome, Observation,
};
pub use trigger::{coherence_debt, debt_band, evaluate_triggers, CoherenceInputs, TriggerInputs};

use playbook_policy::Policy;

/// Derived `H_t`: the machine-readable coherence state of a drive.
pub fn coherence_state(
    ledger: &HuddleLedger,
    inputs: &CoherenceInputs,
    policy: &Policy,
) -> CoherenceState {
    let debt = coherence_debt(inputs, &policy.huddle.debt_weights);
    CoherenceState {
        debt,
        band: debt_band(debt, &policy.huddle),
        active_huddles: u32::try_from(ledger.active().len()).unwrap_or(u32::MAX),
        closed_huddles: u32::try_from(ledger.closed().len()).unwrap_or(u32::MAX),
        unresolved_contradictions: ledger
            .active()
            .iter()
            .map(|huddle| u32::try_from(huddle.contradictions.len()).unwrap_or(u32::MAX))
            .sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_policy::Policy;

    #[test]
    fn coherence_state_is_derived_from_the_ledger() {
        let policy = Policy::builtin();
        let ledger = HuddleLedger::empty();
        let state = coherence_state(&ledger, &CoherenceInputs::default(), &policy);
        assert_eq!(state.debt, 0);
        assert_eq!(state.band, DebtBand::Healthy);
        assert_eq!(state.active_huddles, 0);
    }
}
