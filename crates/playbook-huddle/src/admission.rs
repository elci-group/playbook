//! Admission economics.
//!
//! A requested huddle is admitted when its expected value beats the policy
//! minimum, or when a mandatory safety trigger bypasses the economics. The
//! same state and policy always produce the same decision.

use playbook_core::{BarrierMode, DriveBudget, HuddleBudget, PlaybookError};

use playbook_policy::HuddlePolicy;

use crate::domain::HuddleTriggerKind;
use crate::domain::{HuddleClosure, HuddleLedger, HuddleScope, HuddleTrigger};

/// Expected-value components, each a deterministic 0..=1000 score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleValue {
    pub predicted_rework_avoided: u32,
    pub expected_failure_reduction: u32,
    pub expected_coordination_gain: u32,
    pub expected_budget_preservation: u32,
    pub inference_cost: u32,
    pub latency_cost: u32,
    pub context_cost: u32,
    pub opportunity_cost: u32,
}

impl HuddleValue {
    pub fn expected_value(&self) -> i64 {
        let gains = i64::from(self.predicted_rework_avoided.min(1_000))
            + i64::from(self.expected_failure_reduction.min(1_000))
            + i64::from(self.expected_coordination_gain.min(1_000))
            + i64::from(self.expected_budget_preservation.min(1_000));
        let costs = i64::from(self.inference_cost.min(1_000))
            + i64::from(self.latency_cost.min(1_000))
            + i64::from(self.context_cost.min(1_000))
            + i64::from(self.opportunity_cost.min(1_000));
        gains - costs
    }
}

#[derive(Clone, Debug)]
pub struct AdmissionInputs<'a> {
    pub scope: HuddleScope,
    pub trigger: &'a HuddleTrigger,
    pub debt: u32,
    pub field_fingerprint: &'a str,
    pub ledger: &'a HuddleLedger,
    pub value: HuddleValue,
    pub remaining_drive_budget: DriveBudget,
    pub manual: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmissionDecision {
    Admit {
        budget: HuddleBudget,
        barrier: BarrierMode,
    },
    Reject {
        reason_code: String,
    },
}

pub fn scope_key(scope: HuddleScope) -> &'static str {
    match scope {
        HuddleScope::Position => "position",
        HuddleScope::Unit => "unit",
        HuddleScope::Coordinator => "coordinator",
        HuddleScope::Team => "team",
        HuddleScope::Franchise => "franchise",
    }
}

/// The deterministic admission calculation. Every refusal carries a machine
/// reason code; the same inputs always produce the same result.
pub fn decide(
    inputs: &AdmissionInputs<'_>,
    policy: &HuddlePolicy,
) -> Result<AdmissionDecision, PlaybookError> {
    if inputs.ledger.active_by_scope(inputs.scope).is_some() {
        return Ok(AdmissionDecision::Reject {
            reason_code: "scope_busy".into(),
        });
    }
    if let Some(record) = inputs.ledger.last_closed(inputs.scope) {
        if record.field_fingerprint == inputs.field_fingerprint {
            return Ok(AdmissionDecision::Reject {
                reason_code: "unchanged_field".into(),
            });
        }
    }
    let envelope = policy.budget(scope_key(inputs.scope));
    if envelope.max_tokens > inputs.remaining_drive_budget.tokens
        || envelope.max_cost_microunits > inputs.remaining_drive_budget.monetary_cost_micros
        || envelope.max_participants == 0
    {
        return Ok(AdmissionDecision::Reject {
            reason_code: "no_budget".into(),
        });
    }
    let mandatory = inputs.debt >= policy.debt_mandatory;
    let strategic_scope = matches!(inputs.scope, HuddleScope::Team | HuddleScope::Franchise);
    if strategic_scope && !inputs.manual && !mandatory {
        return Ok(AdmissionDecision::Reject {
            reason_code: "scope_unjustified".into(),
        });
    }
    let failure_escape =
        inputs.trigger.kind == HuddleTriggerKind::Failure && inputs.debt >= policy.debt_eligible;
    let bypass = mandatory || inputs.manual || failure_escape;
    if !bypass && inputs.value.expected_value() <= policy.minimum_expected_value {
        return Ok(AdmissionDecision::Reject {
            reason_code: "expected_value".into(),
        });
    }
    let barrier = if mandatory || inputs.scope == HuddleScope::Franchise {
        BarrierMode::Hard
    } else if inputs.scope == HuddleScope::Position {
        BarrierMode::Soft
    } else {
        BarrierMode::Scope
    };
    Ok(AdmissionDecision::Admit {
        budget: envelope,
        barrier,
    })
}

/// The error recorded when the fold refuses a huddle action.
pub fn refusal(reason_code: &str) -> PlaybookError {
    PlaybookError::Inadmissible("huddle".into(), reason_code.into())
}

pub(crate) fn rejected_closure() -> HuddleClosure {
    HuddleClosure::Rejected
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::EventId;

    fn trigger() -> HuddleTrigger {
        HuddleTrigger {
            kind: HuddleTriggerKind::CoherenceDebt,
            reason_code: "coherence_debt".into(),
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-1").unwrap(),
        }
    }

    fn valuable() -> HuddleValue {
        HuddleValue {
            predicted_rework_avoided: 600,
            expected_failure_reduction: 300,
            expected_coordination_gain: 200,
            expected_budget_preservation: 100,
            inference_cost: 100,
            latency_cost: 50,
            context_cost: 50,
            opportunity_cost: 0,
        }
    }

    #[test]
    fn economics_admit_and_deterministically_reject() {
        let policy = playbook_policy::Policy::builtin().huddle;
        let book = HuddleLedger::empty();
        let trigger = trigger();
        let admission = AdmissionInputs {
            scope: HuddleScope::Position,
            trigger: &trigger,
            debt: 0,
            field_fingerprint: "field-a",
            ledger: &book,
            value: valuable(),
            remaining_drive_budget: DriveBudget {
                tokens: 100_000,
                monetary_cost_micros: 10_000_000,
                ..DriveBudget::zero()
            },
            manual: false,
        };
        match decide(&admission, &policy).unwrap() {
            AdmissionDecision::Admit { budget, barrier } => {
                assert_eq!(budget.max_tokens, 2_000);
                assert_eq!(barrier, BarrierMode::Soft);
            }
            other => panic!("expected admission, got {other:?}"),
        }
        let cheap = HuddleValue {
            predicted_rework_avoided: 50,
            expected_failure_reduction: 50,
            expected_coordination_gain: 0,
            expected_budget_preservation: 0,
            ..valuable()
        };
        let rejected = AdmissionInputs {
            value: cheap,
            ..admission.clone()
        };
        let decision = decide(&rejected, &policy).unwrap();
        assert_eq!(
            decision,
            AdmissionDecision::Reject {
                reason_code: "expected_value".into()
            }
        );
        // The same state and policy produce the same admission result.
        assert_eq!(decide(&rejected, &policy).unwrap(), decision);
    }

    #[test]
    fn unchanged_field_state_is_refused_as_a_huddle_storm() {
        let policy = playbook_policy::Policy::builtin().huddle;
        let mut book = HuddleLedger::empty();
        let trigger = trigger();
        let id = playbook_core::HuddleId::new("h-1").unwrap();
        book.push_active(crate::domain::ActiveHuddle {
            id,
            scope: HuddleScope::Position,
            barrier: BarrierMode::Soft,
            phase: crate::domain::HuddlePhase::Requested,
            trigger: trigger.clone(),
            budget: HuddleBudget::zero(),
            spent: crate::domain::HuddleCost::zero(),
            participants: Vec::new(),
            reports: Vec::new(),
            contradictions: Vec::new(),
            exhausted: false,
            escalation: None,
            partition: Vec::new(),
            field_fingerprint: "field-a".into(),
            at_ms: 1,
            debt_before: 0,
            ytg_milli_before: 0,
            rework_avoided: 0,
        });
        book.close_active(
            &playbook_core::HuddleId::new("h-1").unwrap(),
            HuddleClosure::Rejected,
            "expected_value".into(),
            None,
        )
        .unwrap();
        let admission = AdmissionInputs {
            scope: HuddleScope::Position,
            trigger: &trigger,
            debt: 0,
            field_fingerprint: "field-a",
            ledger: &book,
            value: valuable(),
            remaining_drive_budget: DriveBudget {
                tokens: 100_000,
                monetary_cost_micros: 10_000_000,
                ..DriveBudget::zero()
            },
            manual: false,
        };
        assert!(matches!(
            decide(&admission, &policy).unwrap(),
            AdmissionDecision::Reject { reason_code } if reason_code == "unchanged_field"
        ));
    }

    #[test]
    fn mandatory_debt_bypasses_the_economics() {
        let policy = playbook_policy::Policy::builtin().huddle;
        let book = HuddleLedger::empty();
        let trigger = trigger();
        let admission = AdmissionInputs {
            scope: HuddleScope::Team,
            trigger: &trigger,
            debt: policy.debt_mandatory,
            field_fingerprint: "field-a",
            ledger: &book,
            value: HuddleValue::default(),
            remaining_drive_budget: DriveBudget {
                tokens: 100_000,
                monetary_cost_micros: 10_000_000,
                ..DriveBudget::zero()
            },
            manual: false,
        };
        match decide(&admission, &policy).unwrap() {
            AdmissionDecision::Admit { barrier, .. } => assert_eq!(barrier, BarrierMode::Hard),
            other => panic!("mandatory debt must bypass economics: {other:?}"),
        }
    }

    #[test]
    fn empty_drive_budget_blocks_admission() {
        let policy = playbook_policy::Policy::builtin().huddle;
        let book = HuddleLedger::empty();
        let trigger = trigger();
        let admission = AdmissionInputs {
            scope: HuddleScope::Position,
            trigger: &trigger,
            debt: 0,
            field_fingerprint: "field-a",
            ledger: &book,
            value: valuable(),
            remaining_drive_budget: DriveBudget::zero(),
            manual: false,
        };
        assert_eq!(
            decide(&admission, &policy).unwrap(),
            AdmissionDecision::Reject {
                reason_code: "no_budget".into()
            }
        );
    }
}
