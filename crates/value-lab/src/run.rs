//! Run records, evidence packages and the primary scoreboard (§24, §31).

use serde::{Deserialize, Serialize};

use crate::frozen::{DecisionSnapshot, Regime};
use crate::stats::{median, percentile};
use crate::value::{ocav, regret, utility, Utility, UtilityWeights};

/// One regime's outcome on one assignment, as measured by the lab.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub assignment_id: String,
    pub stratum: crate::corpus::Stratum,
    pub regime: Regime,
    /// The lab's Deliver Gate result (§12) — every mandatory gate passed.
    pub completed: bool,
    pub cost_micros: u64,
    pub latency_ms: u64,
    pub tokens: u64,
    pub model_calls: u32,
    pub tool_calls: u32,
    pub retries: u32,
    pub failed_calls: u32,
    pub human_interventions: u32,
    /// Evidence-backed yards, in milli-yards.
    pub validated_progress_milli: u32,
    pub constraint_compliant: bool,
    pub catastrophic_failure: bool,
    /// The run faced an injected fault (§17) and still reached its gates.
    pub fault_injected: bool,
    pub recovered: bool,
    /// The subject's own overhead attributable to orchestration (§29).
    pub complexity_tax_micros: u64,
    /// The frozen decision snapshots of this run.
    pub decisions: Vec<DecisionSnapshot>,
    /// The lab's OCAV for this run, computed from the best frozen
    /// alternative at the final decision.
    pub ocav: Option<i64>,
    /// The lab's cumulative regret over the run's decisions.
    pub cumulative_regret: i64,
}

impl RunRecord {
    /// Utility under the registered weights.
    pub fn utility(&self, weights: &UtilityWeights) -> i64 {
        utility(
            &Utility {
                validated_progress_milli: self.validated_progress_milli,
                completed: self.completed,
                cost_micros: self.cost_micros,
                latency_ms: self.latency_ms,
                constraint_compliant: self.constraint_compliant,
                catastrophic_failure: self.catastrophic_failure,
            },
            weights,
        )
    }

    /// Validated Completion Cost (§2): total cost per validated
    /// completion. One completion contributes its own cost; incomplete
    /// runs spread nothing — they are counted in the rate, not hidden in
    /// the average.
    pub fn completion_cost_micros(&self) -> Option<u64> {
        if self.completed {
            Some(self.cost_micros.saturating_add(self.complexity_tax_micros))
        } else {
            None
        }
    }

    /// The decision regret the lab measures across this run's frozen
    /// decisions.
    pub fn measure_regret(&mut self) {
        let mut cumulative: i64 = 0;
        for decision in &self.decisions {
            if let (Some(best), Some(selected)) =
                (decision.best_available(), decision.selected_option())
            {
                cumulative += regret(best.predicted_utility, selected.predicted_utility);
            }
        }
        self.cumulative_regret = cumulative;
    }
}

/// Reconstruct OCAV (§3) for each run: the utility of the actual outcome
/// against the best frozen alternative at the run's final decision. The
/// lab recomputes; the subject's Shadow Defence opinion is scored
/// separately (§15).
pub fn attach_ocav(runs: &mut [RunRecord], weights: &UtilityWeights) {
    for run in runs.iter_mut() {
        if let Some(decision) = run.decisions.last() {
            if let Some(alternative) = decision.best_available() {
                let actual = run.utility(weights);
                run.ocav = Some(ocav(actual, alternative.predicted_utility));
            }
        }
    }
}

/// One row of the primary scoreboard (§24).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreRow {
    pub regime: Regime,
    pub runs: u32,
    /// Validated completion rate, ppm.
    pub completion_rate_ppm: u32,
    pub median_completion_cost_micros: Option<u64>,
    pub median_wall_time_ms: Option<u64>,
    /// Median evidence-backed yards per run, milli-yards.
    pub median_progress_milli: Option<u32>,
    /// Recovery rate over fault-injected runs, ppm (§4/H4).
    pub recovery_rate_ppm: Option<u32>,
    pub human_interventions_total: u32,
    pub cumulative_regret_total: i64,
    pub median_ocav: Option<i64>,
}

/// Aggregate run records into the scoreboard, per regime (§24). No single
/// vanity metric: every axis of §24 is reported.
fn median_u64(samples: &[u64]) -> Option<u64> {
    let mut sorted = samples.to_vec();
    sorted.sort();
    if sorted.is_empty() {
        return None;
    }
    Some(sorted[sorted.len() / 2])
}

pub fn scoreboard(runs: &[RunRecord]) -> Vec<ScoreRow> {
    let mut rows = Vec::new();
    for regime in Regime::all() {
        let selected: Vec<&RunRecord> = runs.iter().filter(|run| run.regime == regime).collect();
        if selected.is_empty() {
            continue;
        }
        let completed = selected.iter().filter(|run| run.completed).count();
        let completion_rate_ppm = ((completed as u64 * 1_000_000) / selected.len() as u64) as u32;
        let costs: Vec<u64> = selected
            .iter()
            .filter_map(|run| run.completion_cost_micros())
            .collect();
        let times: Vec<i64> = selected
            .iter()
            .map(|run| i64::try_from(run.latency_ms).unwrap_or(i64::MAX))
            .collect();
        let progress: Vec<i64> = selected
            .iter()
            .map(|run| i64::from(run.validated_progress_milli))
            .collect();
        let faulted: Vec<&RunRecord> = selected
            .iter()
            .filter(|run| run.fault_injected)
            .copied()
            .collect();
        let recovery = if faulted.is_empty() {
            None
        } else {
            let recovered = faulted.iter().filter(|run| run.recovered).count();
            Some(((recovered as u64 * 1_000_000) / faulted.len() as u64) as u32)
        };
        let ocavs: Vec<i64> = selected.iter().filter_map(|run| run.ocav).collect();
        rows.push(ScoreRow {
            regime,
            runs: selected.len() as u32,
            completion_rate_ppm,
            median_completion_cost_micros: median_u64(&costs),
            median_wall_time_ms: median(&times).map(|m| m as u64),
            median_progress_milli: percentile(&progress, 500_000).map(|m| m as u32),
            recovery_rate_ppm: recovery,
            human_interventions_total: selected.iter().map(|run| run.human_interventions).sum(),
            cumulative_regret_total: selected.iter().map(|run| run.cumulative_regret).sum(),
            median_ocav: median(&ocavs),
        });
    }
    rows
}

/// The self-contained evidence package for one run (§31).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePackage {
    pub assignment_id: String,
    pub snap_hash: String,
    pub playbook_version: String,
    pub policy_version: String,
    pub regime: Regime,
    pub run: RunRecord,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::Stratum;
    use crate::frozen::PersonnelOption;

    fn decision() -> DecisionSnapshot {
        DecisionSnapshot {
            snap_hash: "snap".into(),
            at_ms: 0,
            ytg_milli: 1_000,
            gate_digest: "gates".into(),
            options: vec![
                PersonnelOption {
                    name: "chosen".into(),
                    predicted_utility: 300,
                    cost_micros: 10,
                    latency_ms: 10,
                    reliability_ppm: 900_000,
                },
                PersonnelOption {
                    name: "better".into(),
                    predicted_utility: 500,
                    cost_micros: 10,
                    latency_ms: 10,
                    reliability_ppm: 900_000,
                },
            ],
            selected: "chosen".into(),
            rejected: vec!["better".into()],
        }
    }

    fn run(regime: Regime, completed: bool, cost: u64) -> RunRecord {
        let mut run = RunRecord {
            assignment_id: "t1".into(),
            stratum: Stratum::Routine,
            regime,
            completed,
            cost_micros: cost,
            latency_ms: 1_000,
            tokens: 1_000,
            model_calls: 2,
            tool_calls: 3,
            retries: 0,
            failed_calls: 0,
            human_interventions: 0,
            validated_progress_milli: 500,
            constraint_compliant: true,
            catastrophic_failure: false,
            fault_injected: false,
            recovered: false,
            complexity_tax_micros: 10,
            decisions: vec![decision()],
            ocav: None,
            cumulative_regret: 0,
        };
        run.measure_regret();
        run
    }

    #[test]
    fn regret_is_measured_from_the_frozen_options() {
        let run = run(Regime::P0, true, 1_000);
        // The frozen options said "better" was 200 better than "chosen".
        assert_eq!(run.cumulative_regret, 200);
        assert!(!run.decisions[0].optimal());
    }

    #[test]
    fn ocav_compares_the_actual_outcome_to_the_best_frozen_alternative() {
        let weights = UtilityWeights::registered();
        let mut runs = vec![run(Regime::P0, true, 1_000)];
        attach_ocav(&mut runs, &weights);
        let actual = runs[0].utility(&weights);
        assert_eq!(runs[0].ocav, Some(actual - 500));
    }

    #[test]
    fn completion_cost_includes_the_complexity_tax() {
        let run = run(Regime::P0, true, 1_000);
        assert_eq!(run.completion_cost_micros(), Some(1_010));
        let mut incomplete = run.clone();
        incomplete.completed = false;
        assert_eq!(incomplete.completion_cost_micros(), None);
    }

    #[test]
    fn the_scoreboard_reports_every_axis_of_record() {
        let mut runs = vec![
            run(Regime::P0, true, 1_000),
            run(Regime::C1Strongest, false, 500),
        ];
        runs[0].fault_injected = true;
        runs[0].recovered = true;
        runs[1].fault_injected = true;
        runs[1].recovered = false;
        let weights = UtilityWeights::registered();
        attach_ocav(&mut runs, &weights);
        let board = scoreboard(&runs);
        assert_eq!(board.len(), 2);
        let p0 = board.iter().find(|row| row.regime == Regime::P0).unwrap();
        assert_eq!(p0.completion_rate_ppm, 1_000_000);
        assert_eq!(p0.recovery_rate_ppm, Some(1_000_000));
        let c1 = board
            .iter()
            .find(|row| row.regime == Regime::C1Strongest)
            .unwrap();
        assert_eq!(c1.completion_rate_ppm, 0);
        assert_eq!(c1.recovery_rate_ppm, Some(0));
        // C6 is absent from an empty set and never invented.
        assert!(board.iter().all(|row| row.regime != Regime::C6Oracle));
    }
}
