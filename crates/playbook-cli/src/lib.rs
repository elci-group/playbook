//! Playbook command line. `run` is the testable entry point.

#![forbid(unsafe_code)]

use playbook_core::PlaybookError;
use playbook_observability::{audit_text, stage_line};
use playbook_policy::Policy;
use playbook_sim::{compare_policies, run_mvp, MvpReport};

pub fn run(args: &[impl AsRef<str>]) -> i32 {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    match args.first().copied() {
        Some("mvp") => command(print_mvp()),
        Some("sim") => command(print_sim(&args[1..])),
        Some("sprint") => command(print_sprint()),
        Some("priors") => command(print_priors()),
        Some("explain") => command(print_explain()),
        Some("status") => command(print_status()),
        Some("policy") => command(print_policy(args.get(1).copied())),
        _ => {
            eprintln!("usage: playbook <mvp|sim|sprint|priors|explain|status|policy>");
            2
        }
    }
}

fn command(result: Result<(), PlaybookError>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn print_mvp() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    for stage in &report.stages {
        println!("{stage}");
    }
    println!("replay_match {}", report.replay_match);
    println!(
        "disposition {}",
        report
            .disposition
            .map(|item| item.machine())
            .unwrap_or("none")
    );
    println!("false_completion_claims {}", report.false_completion_claims);
    println!("duplicate_failed_plays {}", report.duplicate_failed_plays);
    println!(
        "control_duplicate_failed_plays {}",
        report.control_duplicate_failed_plays
    );
    Ok(())
}

fn print_sim(args: &[&str]) -> Result<(), PlaybookError> {
    let mut policy_name = "playbook-policy-pass-heavy";
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--policy" {
            policy_name = args.get(index + 1).copied().unwrap_or(policy_name);
            index += 2;
            continue;
        }
        if args[index] == "--trace" {
            index += 2;
            continue;
        }
        index += 1;
    }
    let report = accepted_report()?;
    let comparison = compare_policies(&report.events)?;
    println!("policy {policy_name}");
    println!("ytg_match {}", comparison.ytg_match);
    println!("budget_match {}", comparison.budget_match);
    println!("completion_match {}", comparison.completion_match);
    println!("down_labels_differ {}", comparison.down_labels_differ);
    println!("completion_status {}", comparison.completion_status);
    println!("ytg_trajectory {:?}", comparison.ytg_trajectory);
    println!("budget_remaining_ppm {}", comparison.budget_remaining_ppm);
    println!("tool_failures {}", comparison.tool_failures);
    println!("model_calls {}", comparison.model_calls);
    println!("personnel_changes {}", comparison.personnel_changes);
    println!("fourth_down_index {:?}", comparison.fourth_down_index);
    println!("wall_clock_ms {}", comparison.wall_clock_ms);
    println!("monetary_cost_micros {}", comparison.monetary_cost_micros);
    println!("gates_satisfied {}", comparison.gates_satisfied);
    println!("yards_gained_ppm {}", comparison.yards_gained_ppm);
    println!("budget_spent_ppm {}", comparison.budget_spent_ppm);
    println!(
        "efficiency_ratio_milli {}",
        comparison.efficiency_ratio_milli
    );
    if !comparison.ytg_match || !comparison.budget_match || !comparison.down_labels_differ {
        return Err(PlaybookError::ReplayDivergence(
            "policy comparison diverged".into(),
        ));
    }
    Ok(())
}

fn print_sprint() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report.events).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_priors() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    let priors = report.final_state.priors();
    println!("samples {}", priors.samples());
    for (personnel, row) in priors.rows() {
        println!(
            "{personnel} samples={} realized_reduction_sum_milli={}",
            row.samples, row.realized_reduction_sum_milli
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(priors).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_explain() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    println!(
        "{}",
        audit_text(&report.final_state.snapshot(&Policy::builtin()))
    );
    Ok(())
}

fn print_status() -> Result<(), PlaybookError> {
    let report = accepted_report()?;
    let snapshot = report.final_state.snapshot(&Policy::builtin());
    println!("{}", stage_line(&snapshot));
    println!(
        "{}",
        serde_json::to_string(&snapshot).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn print_policy(variant: Option<&str>) -> Result<(), PlaybookError> {
    let policy = match variant {
        Some("pass-heavy") => Policy::pass_heavy_contrast(),
        _ => Policy::builtin(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&policy).map_err(PlaybookError::json)?
    );
    Ok(())
}

fn accepted_report() -> Result<MvpReport, PlaybookError> {
    let report = run_mvp()?;
    let required = [
        "FIRST_DOWN",
        "SECOND_DOWN_GAIN",
        "THIRD_DOWN_PASS",
        "SECOND_DOWN_LOSS",
        "FOURTH_DOWN",
        "THIRD_DOWN_RUSH",
    ];
    for name in required {
        if !report.stages.iter().any(|stage| stage == name) {
            return Err(PlaybookError::ReplayDivergence(format!(
                "missing stage {name}"
            )));
        }
    }
    if !report.replay_match || report.package_leaks || report.duplicate_failed_plays != 0 {
        return Err(PlaybookError::ReplayDivergence(
            "mvp acceptance failed".into(),
        ));
    }
    if report.false_completion_claims == 0 || report.control_duplicate_failed_plays == 0 {
        return Err(PlaybookError::ReplayDivergence(
            "mvp counters failed".into(),
        ));
    }
    Ok(report)
}

trait DispositionName {
    fn machine(self) -> &'static str;
}

impl DispositionName for playbook_core::Disposition {
    fn machine(self) -> &'static str {
        match self {
            Self::Convert => "convert",
            Self::FieldGoal => "field_goal",
            Self::Punt => "punt",
            Self::Reset => "reset",
            Self::Abandon => "abandon",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_and_mvp_commands_exit_clean() {
        assert_eq!(run(&["policy"]), 0);
        assert_eq!(run(&["policy", "pass-heavy"]), 0);
        assert_eq!(run(&["mvp"]), 0);
        assert_eq!(run(&["sprint"]), 0);
        assert_eq!(run(&["priors"]), 0);
        assert_eq!(
            run(&[
                "sim",
                "--trace",
                "mvp",
                "--policy",
                "playbook-policy-pass-heavy"
            ]),
            0
        );
        assert_eq!(run(&["explain"]), 0);
        assert_eq!(run(&["status"]), 0);
    }
}
