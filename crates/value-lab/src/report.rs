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

/// VALUE_CORPUS.jsonl (§32): the frozen corpus, one assignment per line.
pub fn emit_corpus(corpus: &crate::corpus::Corpus) -> Result<String, playbook_core::PlaybookError> {
    let mut out = String::new();
    for assignment in corpus.assignments() {
        let line = serde_json::to_string(assignment).map_err(playbook_core::PlaybookError::json)?;
        out.push_str(&line);
        out.push('\n');
    }
    Ok(out)
}

/// VALUE_ABLATIONS.md (§32): the subsystem contribution study.
pub fn emit_ablations(rows: &[crate::lab::AblationRow]) -> String {
    let mut out = String::new();
    out.push_str("# VALUE_ABLATIONS\n\n");
    out.push_str("| Mechanism | With | Without | Marginal |\n|---|---:|---:|---:|\n");
    for row in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {:+} |\n",
            row.mechanism, row.with_value, row.without_value, row.marginal
        ));
    }
    let earned: Vec<&crate::lab::AblationRow> =
        rows.iter().filter(|row| row.marginal > 0).collect();
    out.push_str(&format!(
        "\nMechanisms with measured marginal value: {}.\n",
        if earned.is_empty() {
            "none".to_string()
        } else {
            earned
                .iter()
                .map(|row| row.mechanism.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    ));
    out
}

/// VALUE_FAILURES.md (§32): the fault-injection analysis.
pub fn emit_failures(runs: &[crate::run::RunRecord]) -> String {
    let mut out = String::new();
    out.push_str("# VALUE_FAILURES\n\n");
    let faulted: Vec<&crate::run::RunRecord> =
        runs.iter().filter(|run| run.fault_injected).collect();
    out.push_str(&format!(
        "Faulted runs: {} of {} ({} ppm of the study).\n\n",
        faulted.len(),
        runs.len(),
        if runs.is_empty() {
            0
        } else {
            faulted.len() * 1_000_000 / runs.len()
        }
    ));
    out.push_str("| Regime | Faulted | Recovered | Rate |\n|---|---:|---:|---:|\n");
    for regime in crate::frozen::Regime::all() {
        let selected: Vec<&crate::run::RunRecord> = faulted
            .iter()
            .filter(|run| run.regime == regime)
            .copied()
            .collect();
        if selected.is_empty() {
            continue;
        }
        let recovered = selected.iter().filter(|run| run.recovered).count();
        out.push_str(&format!(
            "| {} | {} | {} | {:.1}% |\n",
            regime.key(),
            selected.len(),
            recovered,
            recovered as f64 / selected.len() as f64 * 100.0
        ));
    }
    out
}

/// VALUE_SHADOW_DEFENCE.md (§32): counterfactual accuracy and regret,
/// including the lab-subject agreement rate (§3).
pub fn emit_shadow(card: &crate::value::ShadowScorecard) -> String {
    let mut out = String::new();
    out.push_str("# VALUE_SHADOW_DEFENCE\n\n");
    out.push_str(&format!(
        "| Measure | Value |\n|---|---:|\n| Predictions scored | {} |\n| Mean calibration error | {} |\n| Ranking accuracy | {:.1}% |\n| Mean regret estimate error | {} |\n| Lab agreement | {:.1}% |\n",
        card.predictions,
        card.calibration_error_mean,
        card.ranking_accuracy_ppm as f64 / 10_000.0,
        card.regret_error_mean,
        card.agreement_ppm as f64 / 10_000.0,
    ));
    out.push_str(
        "\nShadow Defence does not validate itself: every number above is the lab's\nscoring of frozen predictions against measured outcomes.\n",
    );
    out
}

/// VALUE_ECONOMICS.md (§32): cost and efficiency, with the complexity tax
/// on the record (§29).
pub fn emit_economics(runs: &[crate::run::RunRecord]) -> String {
    let mut out = String::new();
    out.push_str("# VALUE_ECONOMICS\n\n");
    out.push_str("| Regime | Median cost/run | Complexity tax | Median cost/completion |\n|---|---:|---:|---:|\n");
    for regime in crate::frozen::Regime::all() {
        let selected: Vec<&crate::run::RunRecord> =
            runs.iter().filter(|run| run.regime == regime).collect();
        if selected.is_empty() {
            continue;
        }
        let mut costs: Vec<u64> = selected.iter().map(|run| run.cost_micros).collect();
        costs.sort();
        let mut taxes: Vec<u64> = selected
            .iter()
            .map(|run| run.complexity_tax_micros)
            .collect();
        taxes.sort();
        let completions: Vec<u64> = selected
            .iter()
            .filter_map(|run| run.completion_cost_micros())
            .collect();
        let mut completions = completions;
        completions.sort();
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            regime.key(),
            costs.get(costs.len() / 2).copied().unwrap_or(0),
            taxes.get(taxes.len() / 2).copied().unwrap_or(0),
            completions.get(completions.len() / 2).copied().unwrap_or(0),
        ));
    }
    out
}

/// VALUE_REPORT.md (§38): the final statement, filled with measured
/// numbers wherever the study has them — and honest dashes where it does
/// not. Until every dash is a number, no superiority claim is made.
pub fn emit_report(board: &Scoreboard, protocol_hash: &str) -> String {
    let playbook = board
        .rows
        .iter()
        .find(|row| row.regime == crate::frozen::Regime::P0);
    let control = board
        .rows
        .iter()
        .filter(|row| row.regime.deployable() && row.regime != crate::frozen::Regime::P0)
        .max_by_key(|row| row.completion_rate_ppm);
    let fill = |value: Option<String>| value.unwrap_or_else(|| "—".to_string());
    let (n, x, y, a, b, t_a, t_b, r, c, v) = match (playbook, control) {
        (Some(pb), Some(ct)) => (
            format!("{}", pb.runs),
            Some(format!("{:.1}", pb.completion_rate_ppm as f64 / 10_000.0)),
            Some(format!("{:.1}", ct.completion_rate_ppm as f64 / 10_000.0)),
            pb.median_completion_cost_micros
                .map(|cost| format!("{}m¢", cost / 1_000)),
            ct.median_completion_cost_micros
                .map(|cost| format!("{}m¢", cost / 1_000)),
            pb.median_wall_time_ms.map(|ms| format!("{ms}ms")),
            ct.median_wall_time_ms.map(|ms| format!("{ms}ms")),
            pb.recovery_rate_ppm
                .map(|rate| format!("{:.1}", rate as f64 / 10_000.0)),
            ct.recovery_rate_ppm
                .map(|rate| format!("{:.1}", rate as f64 / 10_000.0)),
            pb.median_ocav.map(|ocav| format!("{ocav}")),
        ),
        _ => (
            "—".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
    };
    format!(
        "# VALUE_REPORT\n\nProtocol hash: `{protocol_hash}`\n\n> Across **{n}** previously unseen assignments, Playbook achieved **{}%**          independently validated completion compared with **{}%** for the strongest deployable control,          at **{}** versus **{}** median cost per completion and **{}** versus **{}** median completion time.          Under controlled failure, Playbook recovered **{}%** of interrupted drives versus **{}%** for the control.          Its median opportunity-cost-adjusted value was **{}**, while ablation testing attributed the largest          marginal gains to **[mechanisms — see VALUE_ABLATIONS]**. These results were obtained under protocol          `{protocol_hash}`, using frozen task, model, pricing and evaluation conditions.\n\nVerdict: {}\n",
        fill(x),
        fill(y),
        fill(a),
        fill(b),
        fill(t_a),
        fill(t_b),
        fill(r),
        fill(c),
        fill(v),
        board.verdict.key(),
    )
}

/// VALUE_REPLICATION.md (§32): the independent replication instructions.
pub fn emit_replication(protocol_hash: &str) -> String {
    format!(
        "# VALUE_REPLICATION\n\n1. Check out the repository at the policy and Playbook versions recorded in each evidence package.\n2. Regenerate the fault schedule: `FaultSchedule::from_seed(SEED).assign(&corpus, fraction)` — the seed and fraction are in the protocol.\n3. Re-run every regime of record against the frozen corpus; the snaps hash to `{protocol_hash}`.\n4. Fold each run's journal and confirm the recorded decisions match `derive::decision_snapshots`.\n5. Recompute the scoreboard and verdict with `value-lab scoreboard` — the numbers must reproduce exactly; the lab is deterministic by construction.\n"
    )
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
