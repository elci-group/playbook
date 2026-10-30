//! Admission economics.
//!
//! Inference proposes urgency. Policy authorises expenditure. A formation is
//! admitted only when the ledger is idle, the drive can actually fund the
//! resource-mode envelope, and — for strategic calls — the expected value of
//! the race beats its costs. Instinctive and manual calls bypass the
//! economics, never the budget. The same state and policy always produce the
//! same decision.

use playbook_core::{BlitzBudget, DriveBudget, PlaybookError};

use playbook_policy::{BlitzCorroboration, BlitzPolicy};

use crate::domain::{BlitzClass, BlitzLedger, BlitzResourceMode, BlitzTrigger, DefensivePressure};

/// Expected-value components, each a deterministic 0..=1000 per-mille score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzValue {
    /// Verified time-to-recovery the race is expected to purchase.
    pub expected_ttr_gain_milli: u32,
    pub failure_cost_avoided: u32,
    pub uncertainty_reduction: u32,
    pub redundancy_cost: u32,
    pub verification_cost: u32,
    pub delay_cost: u32,
}

impl BlitzValue {
    /// Gains minus costs: ttr gain, avoided failure cost, and uncertainty
    /// reduction, against redundancy, verification, and delay. A blitz is
    /// bought, never assumed; a non-positive expected value buys nothing.
    pub fn expected_value(&self) -> i64 {
        let gains = i64::from(self.expected_ttr_gain_milli.min(1_000))
            + i64::from(self.failure_cost_avoided.min(1_000))
            + i64::from(self.uncertainty_reduction.min(1_000));
        let costs = i64::from(self.redundancy_cost.min(1_000))
            + i64::from(self.verification_cost.min(1_000))
            + i64::from(self.delay_cost.min(1_000));
        gains - costs
    }
}

/// Everything the deterministic authorisation needs. Every field is an
/// input, never a side channel; the same values always produce the same
/// decision.
#[derive(Clone, Debug)]
pub struct AuthorisationInputs<'a> {
    pub class: BlitzClass,
    pub trigger: &'a BlitzTrigger,
    pub pressure: &'a DefensivePressure,
    pub eligibility_score: u32,
    pub value: BlitzValue,
    pub ledger: &'a BlitzLedger,
    pub resource_mode: BlitzResourceMode,
    pub remaining_drive_budget: DriveBudget,
    pub manual: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Authorisation {
    Authorise {
        budget: BlitzBudget,
        corroboration: BlitzCorroboration,
    },
    Reject {
        reason_code: String,
    },
}

/// The deterministic authorisation. Every refusal carries a machine reason
/// code and is an `Ok` — only a genuinely exceptional failure of the call
/// itself is an `Err`. Formation exclusivity and budget solvency bind every
/// class; the expected-value economics bind only strategic, non-manual
/// calls.
pub fn authorise(
    inputs: &AuthorisationInputs<'_>,
    policy: &BlitzPolicy,
) -> Result<Authorisation, PlaybookError> {
    if inputs.ledger.active_blitz().is_some() {
        return Ok(Authorisation::Reject {
            reason_code: "formation_busy".into(),
        });
    }
    let envelope = policy.budget(inputs.resource_mode.key());
    if envelope.max_tokens > inputs.remaining_drive_budget.tokens
        || envelope.max_cost_microunits > inputs.remaining_drive_budget.monetary_cost_micros
        || envelope.max_workers == 0
    {
        return Ok(Authorisation::Reject {
            reason_code: "no_budget".into(),
        });
    }
    let instinctive = inputs.class == BlitzClass::Instinctive;
    if instinctive || inputs.manual {
        return Ok(Authorisation::Authorise {
            budget: envelope,
            corroboration: policy.corroboration,
        });
    }
    if inputs.eligibility_score < policy.min_pressure_score {
        return Ok(Authorisation::Reject {
            reason_code: "pressure_unjustified".into(),
        });
    }
    if inputs.value.expected_value() <= 0 {
        return Ok(Authorisation::Reject {
            reason_code: "expected_value".into(),
        });
    }
    Ok(Authorisation::Authorise {
        budget: envelope,
        corroboration: policy.corroboration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{BlitzId, EventId};
    use playbook_policy::Policy;

    use crate::domain::{BlitzCost, BlitzPhase, Severity, TargetPackage, VerificationKind};

    fn pressure() -> DefensivePressure {
        DefensivePressure {
            severity: Severity::Critical,
            deadline_ms: Some(60_000),
            deterioration_rate_ppm: 0,
            blast_radius_ppm: 0,
            solver_uncertainty_ppm: 0,
            containment_confidence_ppm: 0,
            recovery_confidence_ppm: 0,
            resource_decay_ppm: 0,
            expected_delay_loss: 0,
        }
    }

    fn strategic_trigger() -> BlitzTrigger {
        BlitzTrigger {
            class: BlitzClass::Strategic,
            reason_code: "strategic_defensive_pressure".into(),
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-1").unwrap(),
        }
    }

    fn valuable() -> BlitzValue {
        BlitzValue {
            expected_ttr_gain_milli: 600,
            failure_cost_avoided: 400,
            uncertainty_reduction: 100,
            redundancy_cost: 200,
            verification_cost: 100,
            delay_cost: 100,
        }
    }

    fn funded() -> DriveBudget {
        DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            ..DriveBudget::zero()
        }
    }

    fn authorisation_inputs<'a>(
        ledger: &'a BlitzLedger,
        trigger: &'a BlitzTrigger,
        pressure: &'a DefensivePressure,
    ) -> AuthorisationInputs<'a> {
        AuthorisationInputs {
            class: BlitzClass::Strategic,
            trigger,
            pressure,
            eligibility_score: 1_000,
            value: valuable(),
            ledger,
            resource_mode: BlitzResourceMode::Normal,
            remaining_drive_budget: funded(),
            manual: false,
        }
    }

    fn active_formation() -> crate::domain::ActiveBlitz {
        let target = TargetPackage {
            problem_statement: "restore the payment service".into(),
            evidence_refs: Vec::new(),
            artefact_refs: Vec::new(),
            constraints: Vec::new(),
            forbidden_actions: Vec::new(),
            expected_output: "verified patch".into(),
            win_condition: "health restored and independently verified".into(),
            verification_gates: vec![VerificationKind::Compilation],
            deadline_ms: 60_000,
            target_hash: String::new(),
        }
        .seal()
        .unwrap();
        crate::domain::ActiveBlitz {
            id: BlitzId::new("b-1").unwrap(),
            class: BlitzClass::Instinctive,
            phase: BlitzPhase::Racing,
            trigger: strategic_trigger(),
            target,
            pressure: pressure(),
            budget: BlitzBudget::zero(),
            spent: BlitzCost::zero(),
            corroboration: BlitzCorroboration::FirstPass,
            resource_mode: BlitzResourceMode::Normal,
            workers: Vec::new(),
            closed_waves: Vec::new(),
            exited: Vec::new(),
            candidates: Vec::new(),
            verifications: Vec::new(),
            human_confirmations: Vec::new(),
            winner: None,
            worker_spend: Vec::new(),
            cancellations: Vec::new(),
            cancel_started_at: None,
            cancel_complete: None,
            failure: None,
            correlated_failures: 0,
            effective_parallelism_milli: 0,
            at_ms: 1,
            deadline_at_ms: 60_001,
        }
    }

    #[test]
    fn strategic_admission_deterministically_authorises() {
        let policy = Policy::builtin().blitz;
        let book = BlitzLedger::empty();
        let trigger = strategic_trigger();
        let situation = pressure();
        let inputs = authorisation_inputs(&book, &trigger, &situation);
        let decision = authorise(&inputs, &policy).unwrap();
        match &decision {
            Authorisation::Authorise {
                budget,
                corroboration,
            } => {
                assert_eq!(budget.max_tokens, policy.budgets.normal.max_tokens);
                assert_eq!(*corroboration, policy.corroboration);
            }
            other => panic!("expected authorisation, got {other:?}"),
        }
        // The same state and policy produce the same authorisation.
        assert_eq!(authorise(&inputs, &policy).unwrap(), decision);
    }

    #[test]
    fn an_active_formation_refuses_a_second() {
        let policy = Policy::builtin().blitz;
        let mut book = BlitzLedger::empty();
        book.push_active(active_formation());
        let trigger = strategic_trigger();
        let situation = pressure();
        assert_eq!(
            authorise(&authorisation_inputs(&book, &trigger, &situation), &policy).unwrap(),
            Authorisation::Reject {
                reason_code: "formation_busy".into()
            }
        );
    }

    #[test]
    fn budget_exhaustion_refuses_the_envelope() {
        let policy = Policy::builtin().blitz;
        let book = BlitzLedger::empty();
        let trigger = strategic_trigger();
        let situation = pressure();
        let mut inputs = authorisation_inputs(&book, &trigger, &situation);
        inputs.remaining_drive_budget = DriveBudget::zero();
        assert_eq!(
            authorise(&inputs, &policy).unwrap(),
            Authorisation::Reject {
                reason_code: "no_budget".into()
            }
        );
        // Tokens alone do not fund a blitz: the monetary dimension binds too.
        let mut tokens_only = authorisation_inputs(&book, &trigger, &situation);
        tokens_only.remaining_drive_budget.monetary_cost_micros = 1;
        assert_eq!(
            authorise(&tokens_only, &policy).unwrap(),
            Authorisation::Reject {
                reason_code: "no_budget".into()
            }
        );
    }

    #[test]
    fn instinctive_and_manual_calls_bypass_the_economics() {
        let policy = Policy::builtin().blitz;
        let book = BlitzLedger::empty();
        let trigger = strategic_trigger();
        let situation = pressure();
        // A strategic call with no value and no pressure is refused on both
        // economics grounds.
        let mut unjustified = authorisation_inputs(&book, &trigger, &situation);
        unjustified.eligibility_score = 0;
        unjustified.value = BlitzValue::default();
        assert_eq!(
            authorise(&unjustified, &policy).unwrap(),
            Authorisation::Reject {
                reason_code: "pressure_unjustified".into()
            }
        );
        // Manual invocation overrides the economics, never the budget.
        let mut manual = unjustified.clone();
        manual.manual = true;
        assert!(matches!(
            authorise(&manual, &policy).unwrap(),
            Authorisation::Authorise { .. }
        ));
        // An instinctive trigger bypasses the economics outright.
        let mut instinctive_trigger = strategic_trigger();
        instinctive_trigger.class = BlitzClass::Instinctive;
        instinctive_trigger.reason_code = "power_critical".into();
        let instinctive = authorisation_inputs(&book, &instinctive_trigger, &situation);
        assert!(matches!(
            authorise(&instinctive, &policy).unwrap(),
            Authorisation::Authorise { .. }
        ));
    }

    #[test]
    fn strategic_calls_must_clear_pressure_and_value_floors() {
        let policy = Policy::builtin().blitz;
        let book = BlitzLedger::empty();
        let trigger = strategic_trigger();
        let situation = pressure();
        // Pressure justified but the race buys nothing: refused on value.
        let mut worthless = authorisation_inputs(&book, &trigger, &situation);
        worthless.value = BlitzValue::default();
        assert!(worthless.value.expected_value() <= 0);
        assert_eq!(
            authorise(&worthless, &policy).unwrap(),
            Authorisation::Reject {
                reason_code: "expected_value".into()
            }
        );
        // Restore the value: the strategic call is authorised.
        let justified = authorisation_inputs(&book, &trigger, &situation);
        assert!(justified.value.expected_value() > 0);
        assert!(matches!(
            authorise(&justified, &policy).unwrap(),
            Authorisation::Authorise { .. }
        ));
    }
}
