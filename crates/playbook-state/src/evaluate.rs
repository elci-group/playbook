//! The EVALUATING branch of the game state machine.
//!
//! Every evaluation names exactly one branch — Advance, Huddle, Substitute,
//! Lastdown, Blitz, or CERS recovery — with reasons, deterministically. This
//! is the single point where the existing subsystems are chosen between; it
//! owns no orchestration of its own and consults no model. Priority runs
//! defence before coherence before personnel before possession before
//! progress: emergency defence (Blitz), then failed-cognition recovery
//! (CERS) so possession never continues on unharvested failure, then
//! coherence (Huddle), then personnel (Substitute), then possession
//! preservation (Lastdown), and only then Advance.

use playbook_core::{EvaluationBranch, Measurement};

/// How many failed attempts on the same field constitute a coherence
/// problem: the huddle subsystem's failure doctrine, mirrored here so the
/// branch ladder and the trigger agree.
const REPEATED_FAILURE_FOR_HUDDLE: u32 = 2;

/// Everything the branch ladder reads. All inputs are measured state —
/// never model claims.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationInputs<'a> {
    pub measurement: &'a Measurement,
    /// A defensive trigger has already cleared blitz eligibility.
    pub defensive_pressure: bool,
    /// The coherence-debt reading, 0..=1000.
    pub coherence_debt: u32,
    /// The debt band at which a huddle becomes eligible (policy).
    pub huddle_debt_eligible: u32,
    /// The Bench has a decided, not-yet-applied substitution.
    pub substitution_pending: bool,
}

/// Name the branch. Deterministic: the same inputs always name the same
/// branch, and every branch carries its machine reasons.
pub fn evaluate_branch(inputs: &EvaluationInputs<'_>) -> (EvaluationBranch, Vec<String>) {
    let measurement = inputs.measurement;
    if inputs.defensive_pressure {
        return (
            EvaluationBranch::Blitz,
            vec!["defensive_pressure_active".into()],
        );
    }
    if measurement.material_failure && measurement.failure_class.is_some() {
        return (
            EvaluationBranch::CersRecovery,
            vec![format!(
                "material_failure_{:?}",
                measurement
                    .failure_class
                    .unwrap_or(playbook_core::FailureClass::TaskRegression)
            )
            .to_lowercase()],
        );
    }
    if inputs.coherence_debt >= inputs.huddle_debt_eligible
        || (measurement.cycle_failure && measurement.attempts >= REPEATED_FAILURE_FOR_HUDDLE)
    {
        let mut reasons = Vec::new();
        if inputs.coherence_debt >= inputs.huddle_debt_eligible {
            reasons.push("coherence_debt_eligible".into());
        }
        if measurement.cycle_failure && measurement.attempts >= REPEATED_FAILURE_FOR_HUDDLE {
            reasons.push("repeated_failure_without_field_change".into());
        }
        return (EvaluationBranch::Huddle, reasons);
    }
    if inputs.substitution_pending {
        return (
            EvaluationBranch::Substitute,
            vec!["bench_decision_pending".into()],
        );
    }
    if measurement.cycle_failure || measurement.stall_cycles > 0 || measurement.possession_restarted
    {
        let mut reasons = Vec::new();
        if measurement.cycle_failure {
            reasons.push("cycle_failure".into());
        }
        if measurement.stall_cycles > 0 {
            reasons.push(format!("stall_cycles_{}", measurement.stall_cycles));
        }
        if measurement.possession_restarted {
            reasons.push("possession_restarted".into());
        }
        return (EvaluationBranch::Lastdown, reasons);
    }
    (EvaluationBranch::Advance, vec!["measured_progress".into()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{Down, FailureClass};

    fn measurement() -> Measurement {
        Measurement {
            validated_progress: true,
            material_failure: false,
            cycle_failure: false,
            failure_class: None,
            attempts: 0,
            progress_cycles: 1,
            ytg_milli: 5_000,
            previous_ytg_milli: 6_000,
            budget_remaining_ppm: 800_000,
            completion_ppm: 0,
            blocking_capability: false,
            blocking_unresolvable: false,
            mandatory_open: true,
            mandatory_failed: false,
            previous_down: Down::FirstDown,
            preflight_complete: true,
            stall_cycles: 0,
            possession_restarted: false,
            open_gate: Some("implementation".into()),
            count_regression: false,
            workload_class: "engineering".into(),
        }
    }

    fn inputs<'a>(measurement: &'a Measurement) -> EvaluationInputs<'a> {
        EvaluationInputs {
            measurement,
            defensive_pressure: false,
            coherence_debt: 0,
            huddle_debt_eligible: 400,
            substitution_pending: false,
        }
    }

    #[test]
    fn defence_outranks_everything() {
        let measurement = measurement();
        let mut pressured = inputs(&measurement);
        pressured.defensive_pressure = true;
        pressured.coherence_debt = 1_000;
        assert_eq!(
            evaluate_branch(&pressured),
            (
                EvaluationBranch::Blitz,
                vec!["defensive_pressure_active".to_string()]
            )
        );
    }

    #[test]
    fn failed_cognition_is_harvested_before_possession_continues() {
        let mut failed = measurement();
        failed.material_failure = true;
        failed.failure_class = Some(FailureClass::ToolFailure);
        failed.validated_progress = false;
        assert_eq!(
            evaluate_branch(&inputs(&failed)).0,
            EvaluationBranch::CersRecovery
        );
    }

    #[test]
    fn coherence_and_repeated_failures_convene_a_huddle() {
        let mut indebted = measurement();
        indebted.validated_progress = false;
        let mut debt = inputs(&indebted);
        debt.coherence_debt = 400;
        assert_eq!(evaluate_branch(&debt).0, EvaluationBranch::Huddle);
        let mut repeated = measurement();
        repeated.cycle_failure = true;
        repeated.attempts = 2;
        repeated.validated_progress = false;
        let (branch, reasons) = evaluate_branch(&inputs(&repeated));
        assert_eq!(branch, EvaluationBranch::Huddle);
        assert!(reasons.contains(&"repeated_failure_without_field_change".into()));
    }

    #[test]
    fn substitution_outranks_lastdown_but_not_a_huddle() {
        let mut candidate = measurement();
        candidate.validated_progress = false;
        let mut pending = inputs(&candidate);
        pending.substitution_pending = true;
        assert_eq!(evaluate_branch(&pending).0, EvaluationBranch::Substitute);
        // Lastdown owns the stalled possession once the personnel question
        // is settled.
        let mut stalled = measurement();
        stalled.validated_progress = false;
        stalled.stall_cycles = 2;
        assert_eq!(
            evaluate_branch(&inputs(&stalled)).0,
            EvaluationBranch::Lastdown
        );
    }

    #[test]
    fn measured_progress_advances() {
        assert_eq!(
            evaluate_branch(&inputs(&measurement())),
            (
                EvaluationBranch::Advance,
                vec!["measured_progress".to_string()]
            )
        );
    }
}
