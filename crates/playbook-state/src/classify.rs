//! Pure down classification.
//!
//! Identical measurements and the same policy version produce the same down.
//! Model inference does not run here.

use playbook_core::{Classification, Down, Measurement, PPM_SCALE, YTG_REFERENCE_MILLI};
use playbook_policy::Policy;

pub fn estimate_completion_ppm(measurement: &Measurement, policy: &Policy) -> u32 {
    if measurement.ytg_milli == 0 {
        return PPM_SCALE;
    }
    if !measurement.preflight_complete {
        return 800_000;
    }
    if measurement.budget_remaining_ppm == 0 {
        return 0;
    }
    let work = (u64::from(measurement.ytg_milli) * u64::from(PPM_SCALE)
        / u64::from(YTG_REFERENCE_MILLI))
    .max(1);
    let mut probability = if u64::from(measurement.budget_remaining_ppm) >= work {
        900_000
    } else {
        u32::try_from(u64::from(measurement.budget_remaining_ppm) * 900_000 / work).unwrap_or(0)
    };
    if measurement.blocking_unresolvable {
        probability = probability.min(150_000);
    } else if measurement.blocking_capability {
        probability = probability.min(400_000);
    }
    probability.saturating_sub(
        measurement
            .stall_cycles
            .saturating_mul(policy.stall_penalty_ppm),
    )
}

pub fn classify_measurement(measurement: &Measurement, policy: &Policy) -> Classification {
    let mut current = measurement.clone();
    current.completion_ppm = estimate_completion_ppm(measurement, policy);
    let down = decide(&current, policy);
    Classification {
        reasons: reasons(&current, down, policy),
        completion_ppm: current.completion_ppm,
        down,
    }
}

fn decide(measurement: &Measurement, policy: &Policy) -> Down {
    let gained = measurement.ytg_milli < measurement.previous_ytg_milli;
    let resolvable_gap = measurement.blocking_capability && !measurement.blocking_unresolvable;
    if measurement.possession_restarted && measurement.ytg_milli > 0 && !measurement.cycle_failure {
        if resolvable_gap {
            return Down::ThirdDownPlayAction;
        }
        if measurement.progress_cycles > 0
            && measurement.budget_remaining_ppm >= policy.rush_budget_fraction_ppm
        {
            return Down::ThirdDownRush;
        }
        if measurement.progress_cycles > 0 {
            return Down::ThirdDownPass;
        }
    }
    if measurement.validated_progress && resolvable_gap && measurement.ytg_milli > 0 {
        return Down::ThirdDownPlayAction;
    }
    if measurement.cycle_failure
        || (measurement.count_regression && measurement.ytg_milli > measurement.previous_ytg_milli)
    {
        return Down::SecondDownLoss;
    }
    let threshold = policy.conversion_threshold(&measurement.workload_class);
    let active =
        measurement.progress_cycles > 0 || measurement.attempts > 0 || measurement.stall_cycles > 0;
    if measurement.ytg_milli > 0
        && measurement.preflight_complete
        && active
        && measurement.completion_ppm < threshold
    {
        return Down::FourthDown;
    }
    if gained && measurement.progress_cycles >= 2 {
        if measurement.budget_remaining_ppm < policy.rush_budget_fraction_ppm {
            return Down::ThirdDownPass;
        }
        return Down::ThirdDownRush;
    }
    if gained {
        return Down::SecondDownGain;
    }
    if !measurement.validated_progress
        && !measurement.material_failure
        && !measurement.cycle_failure
        && measurement.attempts <= policy.first_down_attempt_threshold
    {
        return Down::FirstDown;
    }
    measurement.previous_down
}

fn reasons(measurement: &Measurement, down: Down, policy: &Policy) -> Vec<String> {
    let mut lines = Vec::new();
    if measurement.previous_ytg_milli != measurement.ytg_milli {
        lines.push(format!(
            "YTG moved {} → {}",
            measurement.previous_ytg_milli, measurement.ytg_milli
        ));
    }
    lines.push(format!(
        "remaining budget is {}",
        playbook_core::ppm_percent(measurement.budget_remaining_ppm)
    ));
    if let Some(gate) = &measurement.open_gate {
        lines.push(format!("mandatory gate {gate} remains open"));
    }
    if let Some(class) = measurement.failure_class {
        lines.push(format!("failure class is {class:?}"));
    }
    lines.push(format!(
        "policy pass_threshold = {}",
        playbook_core::ppm_percent(policy.rush_budget_fraction_ppm)
    ));
    lines.push(format!(
        "policy conversion_threshold = {}",
        playbook_core::ppm_percent(policy.conversion_threshold(&measurement.workload_class))
    ));
    lines.push(format!("classification {}", down.machine_id()));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{FailureClass, Measurement};

    fn base() -> Measurement {
        Measurement {
            validated_progress: false,
            material_failure: false,
            cycle_failure: false,
            failure_class: None,
            attempts: 0,
            progress_cycles: 0,
            ytg_milli: 10_000,
            previous_ytg_milli: 10_000,
            budget_remaining_ppm: PPM_SCALE,
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

    #[test]
    fn the_same_measurement_classifies_twice_as_the_same_down() {
        let policy = Policy::builtin();
        let measurement = base();
        assert_eq!(
            classify_measurement(&measurement, &policy),
            classify_measurement(&measurement, &policy)
        );
        assert_eq!(
            classify_measurement(&measurement, &policy).down,
            Down::FirstDown
        );
    }

    #[test]
    fn progress_then_budget_splits_gain_rush_and_pass() {
        let policy = Policy::builtin();
        let mut gain = base();
        gain.validated_progress = true;
        gain.progress_cycles = 1;
        gain.previous_ytg_milli = 10_000;
        gain.ytg_milli = 6_000;
        assert_eq!(
            classify_measurement(&gain, &policy).down,
            Down::SecondDownGain
        );

        let mut rush = gain.clone();
        rush.progress_cycles = 2;
        rush.budget_remaining_ppm = 640_000;
        assert_eq!(
            classify_measurement(&rush, &policy).down,
            Down::ThirdDownRush
        );

        let mut pass = rush.clone();
        pass.budget_remaining_ppm = 340_000;
        let class = classify_measurement(&pass, &policy);
        assert_eq!(class.down, Down::ThirdDownPass);
        assert!(class
            .reasons
            .iter()
            .any(|line| line.contains("pass_threshold = 50%")));
    }

    #[test]
    fn tool_failure_is_loss_and_does_not_require_negative_yards() {
        let policy = Policy::builtin();
        let mut measurement = base();
        measurement.cycle_failure = true;
        measurement.failure_class = Some(FailureClass::ToolFailure);
        measurement.attempts = 2;
        measurement.validated_progress = true;
        measurement.progress_cycles = 2;
        assert_eq!(
            classify_measurement(&measurement, &policy).down,
            Down::SecondDownLoss
        );
    }

    #[test]
    fn low_completion_probability_is_fourth_down() {
        let policy = Policy::builtin();
        let mut measurement = base();
        measurement.validated_progress = true;
        measurement.progress_cycles = 2;
        measurement.attempts = 3;
        measurement.preflight_complete = true;
        measurement.ytg_milli = 3_000;
        measurement.previous_ytg_milli = 3_000;
        measurement.budget_remaining_ppm = 100_000;
        measurement.stall_cycles = 2;
        assert_eq!(
            classify_measurement(&measurement, &policy).down,
            Down::FourthDown
        );
    }
}
