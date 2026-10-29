//! Coherence debt and the deterministic trigger taxonomy.
//!
//! Debt is a deterministic integer in 0..=1000. Bands and weights live in
//! policy, not in compiled business logic.

use playbook_core::{Down, EventId, EvidenceId};

use playbook_policy::{HuddleDebtWeights, HuddlePolicy};

use crate::domain::{DebtBand, HuddleScope, HuddleTrigger, HuddleTriggerKind};

/// Per-mille divergence inputs. Each component is 0..=1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct CoherenceInputs {
    pub assumption_divergence: u32,
    pub dependency_staleness: u32,
    pub contradiction_load: u32,
    pub personnel_turnover: u32,
    pub cross_unit_change_impact: u32,
    pub unresolved_blockers: u32,
    pub context_age: u32,
}

/// Deterministic coherence debt: a weighted sum of clamped per-mille
/// components, clamped to 1000.
pub fn coherence_debt(inputs: &CoherenceInputs, weights: &HuddleDebtWeights) -> u32 {
    let clamp = |value: u32| value.min(1_000);
    let raw = u64::from(clamp(inputs.assumption_divergence)) * u64::from(weights.assumption)
        + u64::from(clamp(inputs.dependency_staleness)) * u64::from(weights.dependency)
        + u64::from(clamp(inputs.contradiction_load)) * u64::from(weights.contradiction)
        + u64::from(clamp(inputs.personnel_turnover)) * u64::from(weights.turnover)
        + u64::from(clamp(inputs.cross_unit_change_impact)) * u64::from(weights.cross_impact)
        + u64::from(clamp(inputs.unresolved_blockers)) * u64::from(weights.blocker)
        + u64::from(clamp(inputs.context_age)) * u64::from(weights.context_age);
    (raw / 1_000).min(1_000) as u32
}

pub fn debt_band(debt: u32, policy: &HuddlePolicy) -> DebtBand {
    if debt >= policy.debt_mandatory {
        DebtBand::MandatoryReconciliation
    } else if debt >= policy.debt_coordinator {
        DebtBand::CoordinatorRecommended
    } else if debt >= policy.debt_eligible {
        DebtBand::HuddleEligible
    } else if debt >= policy.debt_monitor {
        DebtBand::Monitor
    } else {
        DebtBand::Healthy
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerInputs {
    pub failed_attempts: u32,
    pub field_changed_since_failure: bool,
    pub mandatory_gate_completed: bool,
    pub dependent_active_personnel: u32,
    pub down: Down,
    pub debt: u32,
    pub evidence_refs: Vec<EvidenceId>,
    pub originating_event: EventId,
}

/// Deterministic trigger evaluation, most severe first:
/// failure, then success, then strategic, then coherence debt.
pub fn evaluate_triggers(
    inputs: &TriggerInputs,
    policy: &HuddlePolicy,
) -> Option<(HuddleScope, HuddleTrigger)> {
    let trigger = |kind, reason_code: &str| HuddleTrigger {
        kind,
        reason_code: reason_code.to_string(),
        evidence_refs: inputs.evidence_refs.clone(),
        originating_event: inputs.originating_event.clone(),
    };
    if inputs.failed_attempts >= policy.failure_threshold && !inputs.field_changed_since_failure {
        return Some((
            HuddleScope::Position,
            trigger(HuddleTriggerKind::Failure, "repeated_failed_play"),
        ));
    }
    if inputs.mandatory_gate_completed && inputs.dependent_active_personnel >= 2 {
        return Some((
            HuddleScope::Unit,
            trigger(HuddleTriggerKind::Success, "gate_unlocked_dependents"),
        ));
    }
    if inputs.down == Down::FourthDown {
        return Some((
            HuddleScope::Coordinator,
            trigger(HuddleTriggerKind::Strategic, "fourth_down_disposition"),
        ));
    }
    if inputs.debt >= policy.debt_eligible {
        let scope = if inputs.debt >= policy.debt_mandatory {
            HuddleScope::Team
        } else if inputs.debt >= policy.debt_coordinator {
            HuddleScope::Coordinator
        } else {
            HuddleScope::Position
        };
        return Some((
            scope,
            trigger(HuddleTriggerKind::CoherenceDebt, "coherence_debt"),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_policy::Policy;

    fn weights() -> HuddleDebtWeights {
        Policy::builtin().huddle.debt_weights
    }

    fn policy() -> HuddlePolicy {
        Policy::builtin().huddle
    }

    #[test]
    fn debt_is_deterministic_and_clamped() {
        let inputs = CoherenceInputs {
            contradiction_load: 500,
            ..CoherenceInputs::default()
        };
        assert_eq!(coherence_debt(&inputs, &weights()), 200);
        let full = CoherenceInputs {
            assumption_divergence: 1_000,
            dependency_staleness: 1_000,
            contradiction_load: 1_000,
            personnel_turnover: 1_000,
            cross_unit_change_impact: 1_000,
            unresolved_blockers: 1_000,
            context_age: 1_000,
        };
        assert_eq!(coherence_debt(&full, &weights()), 1_000);
        assert_eq!(
            coherence_debt(&full, &weights()),
            coherence_debt(&full, &weights())
        );
    }

    #[test]
    fn bands_follow_policy_thresholds() {
        let policy = policy();
        assert_eq!(debt_band(0, &policy), DebtBand::Healthy);
        assert_eq!(debt_band(policy.debt_monitor, &policy), DebtBand::Monitor);
        assert_eq!(
            debt_band(policy.debt_eligible, &policy),
            DebtBand::HuddleEligible
        );
        assert_eq!(
            debt_band(policy.debt_coordinator, &policy),
            DebtBand::CoordinatorRecommended
        );
        assert_eq!(
            debt_band(policy.debt_mandatory, &policy),
            DebtBand::MandatoryReconciliation
        );
    }

    #[test]
    fn triggers_fire_in_severity_order() {
        let policy = policy();
        let event = EventId::new("evt-1").unwrap();
        let base = TriggerInputs {
            failed_attempts: 0,
            field_changed_since_failure: true,
            mandatory_gate_completed: false,
            dependent_active_personnel: 0,
            down: Down::FirstDown,
            debt: 0,
            evidence_refs: Vec::new(),
            originating_event: event,
        };
        assert_eq!(evaluate_triggers(&base, &policy), None);

        let failed = TriggerInputs {
            failed_attempts: policy.failure_threshold,
            field_changed_since_failure: false,
            ..base.clone()
        };
        let (scope, trigger) = evaluate_triggers(&failed, &policy).unwrap();
        assert_eq!(scope, HuddleScope::Position);
        assert_eq!(trigger.kind, HuddleTriggerKind::Failure);
        assert_eq!(trigger.reason_code, "repeated_failed_play");

        // A field change cancels the failure trigger; success takes over.
        let success = TriggerInputs {
            field_changed_since_failure: true,
            mandatory_gate_completed: true,
            dependent_active_personnel: 2,
            ..failed
        };
        let (scope, trigger) = evaluate_triggers(&success, &policy).unwrap();
        assert_eq!(scope, HuddleScope::Unit);
        assert_eq!(trigger.kind, HuddleTriggerKind::Success);

        let strategic = TriggerInputs {
            down: Down::FourthDown,
            mandatory_gate_completed: false,
            dependent_active_personnel: 0,
            ..success
        };
        let (scope, trigger) = evaluate_triggers(&strategic, &policy).unwrap();
        assert_eq!(scope, HuddleScope::Coordinator);
        assert_eq!(trigger.kind, HuddleTriggerKind::Strategic);

        // Strategic outranks debt, so the debt band is evaluated on a
        // non-fourth-down field.
        let indebted = TriggerInputs {
            debt: policy.debt_mandatory,
            ..base.clone()
        };
        let (scope, trigger) = evaluate_triggers(&indebted, &policy).unwrap();
        assert_eq!(scope, HuddleScope::Team);
        assert_eq!(trigger.kind, HuddleTriggerKind::CoherenceDebt);
        assert_eq!(trigger.reason_code, "coherence_debt");
    }
}
