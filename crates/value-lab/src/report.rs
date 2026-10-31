//! The VALUE artifacts (§32): the scoreboard, rendered for people.

use serde::{Deserialize, Serialize};

use crate::frozen::Regime;
use crate::lab::Verdict;
pub use crate::run::scoreboard;

/// The primary scoreboard (§24): every axis of record, per regime.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scoreboard {
    pub rows: Vec<crate::run::ScoreRow>,
    pub verdict: Verdict,
}

/// Render the scoreboard as the compact top-level table. The best
/// deployable control — never the hindsight bound — is the comparison
/// column (§4).
pub fn emit_scoreboard(board: &Scoreboard) -> String {
    let mut out = String::new();
    out.push_str("# VALUE_SCOREBOARD\n\n");
    out.push_str("| Metric | Playbook (P0) | Best deployable control | Delta |\n");
    out.push_str("|---|---:|---:|---:|\n");
    let playbook = board.rows.iter().find(|row| row.regime == Regime::P0);
    let best_control = board
        .rows
        .iter()
        .filter(|row| row.regime.deployable() && row.regime != Regime::P0)
        .max_by_key(|row| row.completion_rate_ppm);
    let (Some(playbook), Some(control)) = (playbook, best_control) else {
        out.push_str("| (insufficient runs) | — | — | — |\n");
        out.push_str(&format!("\nVerdict: {}\n", board.verdict.key()));
        return out;
    };
    let mut delta = |playbook_value: String, control_value: String| {
        format!("{} vs {}", playbook_value, control_value)
    };
    let _ = &mut delta;
    out.push_str(&format!(
        "| Runs | {} | {} | — |\n",
        playbook.runs, control.runs
    ));
    out.push_str(&format!(
        "| Validated completion rate | {:.1}% | {:.1}% | {:+.1}pp |\n",
        playbook.completion_rate_ppm as f64 / 10_000.0,
        control.completion_rate_ppm as f64 / 10_000.0,
        (playbook.completion_rate_ppm as f64 - control.completion_rate_ppm as f64) / 10_000.0,
    ));
    if let (Some(pb), Some(ct)) = (
        playbook.median_completion_cost_micros,
        control.median_completion_cost_micros,
    ) {
        out.push_str(&format!(
            "| Median completion cost | {}m¢ | {}m¢ | {:+.0}% |\n",
            pb / 1_000,
            ct / 1_000,
            percent_delta(pb, ct)
        ));
    }
    if let (Some(pb), Some(ct)) = (playbook.median_wall_time_ms, control.median_wall_time_ms) {
        out.push_str(&format!(
            "| Median wall time | {}ms | {}ms | {:+.0}% |\n",
            pb,
            ct,
            percent_delta(pb, ct)
        ));
    }
    if let (Some(pb), Some(ct)) = (playbook.recovery_rate_ppm, control.recovery_rate_ppm) {
        out.push_str(&format!(
            "| Recovery rate (faulted) | {:.1}% | {:.1}% | {:+.1}pp |\n",
            pb as f64 / 10_000.0,
            ct as f64 / 10_000.0,
            (pb as f64 - ct as f64) / 10_000.0,
        ));
    }
    out.push_str(&format!(
        "| Human interventions | {} | {} | {:+} |\n",
        playbook.human_interventions_total,
        control.human_interventions_total,
        playbook.human_interventions_total as i64 - control.human_interventions_total as i64,
    ));
    out.push_str(&format!(
        "| Cumulative regret | {} | {} | {:+} |\n",
        playbook.cumulative_regret_total,
        control.cumulative_regret_total,
        playbook.cumulative_regret_total - control.cumulative_regret_total,
    ));
    if let (Some(pb), Some(ct)) = (playbook.median_ocav, control.median_ocav) {
        out.push_str(&format!("| Median OCAV | {pb} | {ct} | {:+} |\n", pb - ct));
    }
    out.push_str(&format!("\nVerdict: {}\n", board.verdict.key()));
    out
}

fn percent_delta(playbook_value: u64, control_value: u64) -> f64 {
    if control_value == 0 {
        return 0.0;
    }
    (playbook_value as f64 - control_value as f64) / control_value as f64 * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::Stratum;
    use crate::frozen::Regime;
    use crate::lab::{classify, FrozenThresholds};
    use crate::run::RunRecord;

    fn run(regime: Regime, completed: bool) -> RunRecord {
        RunRecord {
            assignment_id: "t".into(),
            stratum: Stratum::Routine,
            regime,
            completed,
            cost_micros: 2_000,
            latency_ms: 3_000,
            tokens: 1_000,
            model_calls: 1,
            tool_calls: 1,
            retries: 0,
            failed_calls: 0,
            human_interventions: 0,
            validated_progress_milli: 100,
            constraint_compliant: true,
            catastrophic_failure: false,
            fault_injected: false,
            recovered: false,
            complexity_tax_micros: 0,
            decisions: Vec::new(),
            ocav: Some(10),
            cumulative_regret: 0,
        }
    }

    #[test]
    fn the_scoreboard_table_renders_both_columns() {
        let runs: Vec<RunRecord> = (0..10)
            .flat_map(|index| {
                vec![
                    run(Regime::P0, index < 8),
                    run(Regime::C1Strongest, index < 5),
                ]
            })
            .collect();
        let verdict = classify(&runs, &FrozenThresholds::registered());
        let board = Scoreboard {
            rows: scoreboard(&runs),
            verdict,
        };
        let table = emit_scoreboard(&board);
        assert!(table.contains("# VALUE_SCOREBOARD"));
        assert!(table.contains("Validated completion rate"));
        assert!(table.contains("80.0%"));
        assert!(table.contains("50.0%"));
        assert!(table.contains("Verdict:"));
        // The oracle bound never appears as a competitor column.
        assert!(!table.contains("C6"));
    }

    #[test]
    fn an_empty_study_renders_an_honest_empty_table() {
        let board = Scoreboard {
            rows: Vec::new(),
            verdict: Verdict::ValueNeutral,
        };
        let table = emit_scoreboard(&board);
        assert!(table.contains("insufficient runs"));
        assert!(table.contains("VALUE_NEUTRAL"));
    }
}
