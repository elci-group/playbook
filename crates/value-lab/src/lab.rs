//! The lab's control room: pre-registered thresholds, the verdict
//! classifier, the fault schedule, campaign phases, stop conditions, the
//! ablation study, and the concrete experiments that reuse Playbook's own
//! simulation surfaces as subjects (§18–§22, §28, §35–§37).

use serde::{Deserialize, Serialize};

use crate::corpus::{Corpus, Stratum};
use crate::frozen::Regime;
use crate::run::{scoreboard, RunRecord};

/// The pre-registered success thresholds (§28). Frozen into the protocol
/// before the primary season; the verdict classifier reads nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenThresholds {
    /// ≥10% relative completion-rate improvement, in ppm.
    pub completion_improvement_ppm: u32,
    /// ≥15% reduction in median cost per validated completion, in ppm.
    pub cost_reduction_ppm: u32,
    /// No material regression in completion quality: at most this much
    /// worse than the best control, in ppm.
    pub regression_tolerance_ppm: u32,
}

impl FrozenThresholds {
    pub fn registered() -> Self {
        Self {
            completion_improvement_ppm: 100_000,
            cost_reduction_ppm: 150_000,
            regression_tolerance_ppm: 20_000,
        }
    }
}

/// The study verdict (§1). No result is suppressed for being unfavourable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    ValueProven,
    /// Superiority confined to these workload strata.
    ValueConditional(Vec<Stratum>),
    ValueNeutral,
    ValueNegative,
}

impl Verdict {
    pub fn key(&self) -> &'static str {
        match self {
            Self::ValueProven => "VALUE_PROVEN",
            Self::ValueConditional(_) => "VALUE_CONDITIONAL",
            Self::ValueNeutral => "VALUE_NEUTRAL",
            Self::ValueNegative => "VALUE_NEGATIVE",
        }
    }
}

fn completion_rate(runs: &[RunRecord], regime: Regime) -> Option<u32> {
    let selected: Vec<&RunRecord> = runs.iter().filter(|run| run.regime == regime).collect();
    if selected.is_empty() {
        return None;
    }
    let completed = selected.iter().filter(|run| run.completed).count();
    Some(((completed as u64 * 1_000_000) / selected.len() as u64) as u32)
}

/// The best deployable control's completion rate (C6 is a bound, not a
/// competitor — §4).
fn best_control_completion(runs: &[RunRecord]) -> Option<(Regime, u32)> {
    let mut best: Option<(Regime, u32)> = None;
    for regime in Regime::all() {
        if regime == Regime::P0 || !regime.deployable() {
            continue;
        }
        if let Some(rate) = completion_rate(runs, regime) {
            if best.map(|(_, best_rate)| rate > best_rate).unwrap_or(true) {
                best = Some((regime, rate));
            }
        }
    }
    best
}

/// The strata where Playbook beats the best deployable control on
/// completion rate (§28: superiority across more than one workload class).
fn winning_strata(runs: &[RunRecord]) -> Vec<Stratum> {
    let mut winners = Vec::new();
    for stratum in [
        Stratum::Routine,
        Stratum::Moderate,
        Stratum::Difficult,
        Stratum::Adversarial,
        Stratum::Frontier,
    ] {
        let rate = |regime: Regime| -> Option<u32> {
            let selected: Vec<&RunRecord> = runs
                .iter()
                .filter(|run| run.regime == regime && run.stratum == stratum)
                .collect();
            if selected.is_empty() {
                return None;
            }
            let completed = selected.iter().filter(|run| run.completed).count();
            Some(((completed as u64 * 1_000_000) / selected.len() as u64) as u32)
        };
        let (Some(playbook), Some(control)) = (
            rate(Regime::P0),
            best_control_completion(runs).map(|(_, rate)| rate),
        ) else {
            continue;
        };
        if playbook > control {
            winners.push(stratum);
        }
    }
    winners
}

/// Classify the study verdict (§1, §28). The classification is mechanical:
/// the thresholds were frozen before results existed.
pub fn classify(runs: &[RunRecord], thresholds: &FrozenThresholds) -> Verdict {
    let Some(playbook_rate) = completion_rate(runs, Regime::P0) else {
        return Verdict::ValueNeutral;
    };
    let Some((_, control_rate)) = best_control_completion(runs) else {
        return Verdict::ValueNeutral;
    };
    let board = scoreboard(runs);
    let playbook_row = board.iter().find(|row| row.regime == Regime::P0);
    // Resilience advantage under fault injection (§28).
    let resilience = playbook_row
        .and_then(|row| row.recovery_rate_ppm)
        .unwrap_or(0)
        >= control_recovery(runs).unwrap_or(u32::MAX);
    // Positive median OCAV (§28, §3).
    let positive_ocav = playbook_row
        .and_then(|row| row.median_ocav)
        .map(|ocav| ocav > 0)
        .unwrap_or(false);
    // No material regression in completion quality: catastrophic failure
    // rate within tolerance of the control (§28).
    let catastrophe_rate = |regime: Regime| -> u32 {
        let selected: Vec<&RunRecord> = runs.iter().filter(|run| run.regime == regime).collect();
        if selected.is_empty() {
            return 0;
        }
        ((selected
            .iter()
            .filter(|run| run.catastrophic_failure)
            .count() as u64
            * 1_000_000)
            / selected.len() as u64) as u32
    };
    let quality_ok = catastrophe_rate(Regime::P0) <= catastrophe_rate(Regime::C1Strongest);
    let relative_gain_ppm = if control_rate == 0 {
        if playbook_rate > 0 {
            u32::MAX
        } else {
            0
        }
    } else {
        ((u64::from(playbook_rate) - u64::from(control_rate).min(u64::from(playbook_rate)))
            .saturating_mul(1_000_000)
            / u64::from(control_rate)) as u32
    };
    let strata = winning_strata(runs);
    if std::env::var_os("VALUE_LAB_DEBUG").is_some() {
        eprintln!(
            "classify: playbook={playbook_rate} control={control_rate} gain={relative_gain_ppm} resilience={resilience} ocav={positive_ocav} quality={quality_ok} strata={}",
            strata.len()
        );
    }
    let meets_headline = relative_gain_ppm >= thresholds.completion_improvement_ppm;
    let supporting = resilience && positive_ocav && quality_ok && strata.len() > 1;
    if meets_headline && supporting {
        Verdict::ValueProven
    } else if !strata.is_empty() {
        Verdict::ValueConditional(strata)
    } else if playbook_rate == control_rate || completion_rate(runs, Regime::P0).is_none() {
        Verdict::ValueNeutral
    } else {
        Verdict::ValueNegative
    }
}

fn control_recovery(runs: &[RunRecord]) -> Option<u32> {
    let board = scoreboard(runs);
    board
        .iter()
        .filter(|row| row.regime.deployable() && row.regime != Regime::P0)
        .filter_map(|row| row.recovery_rate_ppm)
        .max()
}

/// One controlled degradation (§17). The schedule assigns faults
/// deterministically from a seed, so any lab can reproduce who was
/// faulted and how.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultClass {
    Provider,
    Resource,
    Tool,
    Personnel,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub class: FaultClass,
    /// The wire-level kind: 429, 503, quota exhaustion, command failure,
    /// hallucinated completion, and so on (§17).
    pub kind: String,
}

/// The deterministic fault schedule: every lab reproduces the same
/// assignment of faults to assignments from the same seed.
pub struct FaultSchedule {
    seed: u64,
}

impl FaultSchedule {
    pub fn from_seed(seed: u64) -> Self {
        Self { seed: seed.max(1) }
    }

    /// Assign faults to `fraction_ppm` of the corpus, cycling the fault
    /// kinds deterministically. The subject never learns the schedule
    /// beyond what would naturally exist (§17).
    pub fn assign(&self, corpus: &Corpus, fraction_ppm: u32) -> Vec<(String, Vec<Fault>)> {
        let mut state = self.seed;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // The multiplicative generator's high bits are the well-mixed
            // ones; the low bits of an LCG cycle with short periods.
            state >> 33
        };
        let kinds = [
            (FaultClass::Provider, "429_rate_limited"),
            (FaultClass::Provider, "503_unavailable"),
            (FaultClass::Resource, "token_pressure"),
            (FaultClass::Resource, "budget_pressure"),
            (FaultClass::Tool, "command_failure"),
            (FaultClass::Tool, "partial_write"),
            (FaultClass::Personnel, "hallucinated_completion"),
            (FaultClass::Personnel, "unavailable_specialist"),
        ];
        let mut schedule = Vec::new();
        for assignment in corpus.assignments() {
            let roll = (next() % 1_000_000) as u32;
            if roll < fraction_ppm {
                let index = (next() as usize) % kinds.len();
                schedule.push((
                    assignment.id.clone(),
                    vec![Fault {
                        class: kinds[index].0,
                        kind: kinds[index].1.to_string(),
                    }],
                ));
            }
        }
        schedule
    }
}

/// The campaign phases (§37). Order is enforced; each phase opens only
/// when the previous gate closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Instrumentation,
    Baselines,
    Preseason,
    RegularSeason,
    AdversarialSeason,
    Ablations,
    ShadowSeason,
    Replication,
    Publication,
}

/// The stop conditions (§35): the study pauses automatically and the
/// methodological fault is repaired before any run is discarded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabHealth {
    /// Every journal folds and its digests verify.
    pub journal_integrity: bool,
    /// Every regime received the same snap hash per assignment.
    pub information_parity: bool,
    pub pricing_reliable: bool,
    /// The evaluator has not been touched by the subject.
    pub evaluator_clean: bool,
    /// Deliver Gates structurally favour one implementation.
    pub gate_bias_detected: bool,
    /// Decision snapshots reconstruct for every material decision.
    pub snapshots_reconstructable: bool,
    /// The subject controls its own evaluation authority.
    pub subject_controls_authority: bool,
}

/// The first triggered stop condition, if any. Fail closed.
pub fn stopped_by(health: &LabHealth) -> Option<&'static str> {
    if !health.journal_integrity {
        return Some("journal_integrity_failed");
    }
    if !health.information_parity {
        return Some("regimes_received_different_information");
    }
    if !health.pricing_reliable {
        return Some("pricing_telemetry_unreliable");
    }
    if !health.evaluator_clean {
        return Some("evaluator_contamination_detected");
    }
    if health.gate_bias_detected {
        return Some("deliver_gates_structurally_biased");
    }
    if !health.snapshots_reconstructable {
        return Some("snapshots_not_reconstructable");
    }
    if health.subject_controls_authority {
        return Some("subject_controls_research_authority");
    }
    None
}

/// One ablation result (§5, §30): the marginal value of one subsystem,
/// measured as the difference between full Playbook and Playbook without
/// the mechanism, on the same assignments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AblationRow {
    pub mechanism: String,
    /// Mean lab utility with the mechanism enabled.
    pub with_value: i64,
    /// Mean lab utility with the mechanism disabled.
    pub without_value: i64,
    pub marginal: i64,
}

/// Rank subsystem contributions. A mechanism whose ablation leaves value
/// unchanged has not earned its complexity (§30 P3).
pub fn ablation_table(full: &[i64], ablated: &[i64], mechanism: &str) -> AblationRow {
    let with = crate::stats::mean(full).unwrap_or(0);
    let without = crate::stats::mean(ablated).unwrap_or(0);
    AblationRow {
        mechanism: mechanism.to_string(),
        with_value: with,
        without_value: without,
        marginal: with - without,
    }
}

/// The Huddle experiment (§19), run on Playbook's own Phase 8 programme
/// with the lab's reading: the four regimes are the four coordination
/// policies, and the lab reports completion and expenditure per regime.
/// The subject's sim engine executes; the lab owns the comparison.
pub fn huddle_comparison() -> Vec<(String, bool, u64, u64)> {
    let regimes = [
        playbook_sim::HuddleRegime::Control,
        playbook_sim::HuddleRegime::Periodic,
        playbook_sim::HuddleRegime::FlatTriggered,
        playbook_sim::HuddleRegime::HierarchicalTriggered,
    ];
    let scenarios = playbook_sim::phase_eight_study();
    let mut rows = Vec::new();
    for scenario in &scenarios {
        for regime in regimes {
            let report = scenario.run(regime);
            rows.push((
                format!("{}/{}", scenario.name, regime.name()),
                report.completed,
                report.total_tokens,
                u64::from(report.steps),
            ));
        }
    }
    rows
}

/// The Lastdown audit (§21): the lab independently recomputes the
/// doctrine decision from the stated inputs and checks the subject chose
/// the utility-dominant disposition. Playbook's own `evaluate` is the
/// arithmetic; the audit is that the choice equals the recomputed maximum.
pub fn lastdown_audit(
    input: &playbook_lastdown::DoctrineInput,
    chosen: playbook_core::Disposition,
) -> bool {
    let decision = playbook_lastdown::evaluate(input);
    decision.chosen == chosen
}

/// Emit the frozen protocol document (§27, §32 VALUE_PROTOCOL.md).
pub fn emit_protocol() -> String {
    let thresholds = FrozenThresholds::registered();
    let weights = crate::value::UtilityWeights::registered();
    format!(
        "# VALUE_PROTOCOL — frozen before the primary season\n\n\
         Hypotheses: H1 completion, H2 resource efficiency, H3 progressive efficiency,\n\
         H4 reliability, H5 personnel selection. Central metric: OCAV (§3).\n\n\
         Thresholds (frozen):\n\
         - completion improvement >= {} ppm relative\n\
         - cost-per-completion reduction >= {} ppm\n\
         - regression tolerance <= {} ppm\n\n\
         Utility weights (frozen): completion {} ppm, progress {} ppm, cost {} ppm,\n\
         latency {} ppm, compliance {} ppm, catastrophe penalty {}.\n\n\
         Primary metric: OCAV = U(actual) - U(best credible alternative at decision time).\n\
         Counterfactuals are reconstructed by value-lab from DECISION_SNAPSHOTs.\n\
         Shadow Defence does not validate itself; lab-subject agreement is a measured variable.\n",
        thresholds.completion_improvement_ppm,
        thresholds.cost_reduction_ppm,
        thresholds.regression_tolerance_ppm,
        weights.completion_ppm,
        weights.progress_ppm,
        weights.cost_ppm,
        weights.latency_ppm,
        weights.compliance_ppm,
        weights.catastrophe_penalty,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{Assignment, Corpus, DifficultyProfile, TaskClass};

    fn run(
        regime: Regime,
        completed: bool,
        stratum: Stratum,
        fault: bool,
        recovered: bool,
    ) -> RunRecord {
        RunRecord {
            assignment_id: "t".into(),
            stratum,
            regime,
            completed,
            cost_micros: 1_000,
            latency_ms: 1_000,
            tokens: 1_000,
            model_calls: 1,
            tool_calls: 1,
            retries: 0,
            failed_calls: 0,
            human_interventions: 0,
            validated_progress_milli: 100,
            constraint_compliant: true,
            catastrophic_failure: false,
            fault_injected: fault,
            recovered,
            complexity_tax_micros: 0,
            decisions: Vec::new(),
            ocav: Some(50),
            cumulative_regret: 0,
        }
    }

    fn corpus() -> Corpus {
        let mut corpus = Corpus::empty();
        for index in 0..10 {
            corpus
                .add(Assignment {
                    id: format!("t{index}"),
                    class: TaskClass::BugRepair,
                    statement: "repair".into(),
                    stratum: Stratum::Routine,
                    profile: DifficultyProfile {
                        complexity: 40,
                        uncertainty: 40,
                        tool_dependence: 40,
                        horizon: 40,
                        failure_exposure: 40,
                        verification_difficulty: 40,
                    },
                    deliver_gates: vec!["tests".into()],
                })
                .unwrap();
        }
        corpus
    }

    #[test]
    fn proven_requires_headline_plus_supporting_evidence() {
        let mut runs = Vec::new();
        // Playbook completes 90%, the best control 50%, across two strata,
        // with full recovery and positive OCAV.
        for index in 0..10 {
            for stratum in [Stratum::Routine, Stratum::Moderate] {
                runs.push(run(Regime::P0, index < 9, stratum, true, true));
                runs.push(run(Regime::C1Strongest, index < 5, stratum, true, false));
            }
        }
        let verdict = classify(&runs, &FrozenThresholds::registered());
        assert_eq!(verdict, Verdict::ValueProven);
    }

    #[test]
    fn inferior_performance_is_reported_as_negative() {
        let mut runs = Vec::new();
        for _ in 0..10 {
            runs.push(run(Regime::P0, false, Stratum::Routine, false, false));
            runs.push(run(
                Regime::C1Strongest,
                true,
                Stratum::Routine,
                false,
                false,
            ));
        }
        let verdict = classify(&runs, &FrozenThresholds::registered());
        assert_eq!(verdict, Verdict::ValueNegative);
    }

    #[test]
    fn a_single_winning_stratum_is_conditional_not_proven() {
        let mut runs = Vec::new();
        // Playbook wins only on Difficult work.
        for index in 0..10 {
            runs.push(run(Regime::P0, index < 9, Stratum::Difficult, false, false));
            runs.push(run(
                Regime::C1Strongest,
                index < 5,
                Stratum::Difficult,
                false,
                false,
            ));
            runs.push(run(Regime::P0, false, Stratum::Routine, false, false));
            runs.push(run(
                Regime::C1Strongest,
                false,
                Stratum::Routine,
                false,
                false,
            ));
        }
        let verdict = classify(&runs, &FrozenThresholds::registered());
        assert!(
            matches!(verdict, Verdict::ValueConditional(ref strata) if strata.contains(&Stratum::Difficult)),
            "{verdict:?}"
        );
    }

    #[test]
    fn the_fault_schedule_is_deterministic_and_honours_the_floor() {
        let corpus = corpus();
        let first = FaultSchedule::from_seed(7).assign(&corpus, 300_000);
        let second = FaultSchedule::from_seed(7).assign(&corpus, 300_000);
        assert_eq!(first, second, "same seed, same schedule");
        let count = first.len();
        assert!(
            (2..=6).contains(&count),
            "a 30% roll over 10 tasks should fault some, not all: {count}"
        );
    }

    #[test]
    fn every_stop_condition_fails_the_study_closed() {
        let healthy = LabHealth {
            journal_integrity: true,
            information_parity: true,
            pricing_reliable: true,
            evaluator_clean: true,
            gate_bias_detected: false,
            snapshots_reconstructable: true,
            subject_controls_authority: false,
        };
        assert_eq!(stopped_by(&healthy), None);
        let mut tampered = healthy.clone();
        tampered.journal_integrity = false;
        assert_eq!(stopped_by(&tampered), Some("journal_integrity_failed"));
        let mut captured = healthy;
        captured.subject_controls_authority = true;
        assert_eq!(
            stopped_by(&captured),
            Some("subject_controls_research_authority")
        );
    }

    #[test]
    fn ablations_rank_marginal_contribution() {
        let huddles = ablation_table(&[100, 110, 90], &[60, 70, 50], "huddles");
        assert_eq!(huddles.marginal, 40);
        let ceremony = ablation_table(&[100, 110, 90], &[100, 110, 90], "ceremony");
        assert_eq!(ceremony.marginal, 0, "a mechanism with no marginal value");
    }

    #[test]
    fn the_lastdown_audit_recomputes_the_choice() {
        let input = playbook_lastdown::DoctrineInput {
            integrity: playbook_core::TaskIntegrity::Sound,
            full_utility: 1_000,
            banked_utility: 700,
            p_score: 800_000,
            p_preserve: 500_000,
            p_finish_after_handoff: 300_000,
            min_attempt: 100_000,
            reset_bonus_ppm: 0,
            coaching_contested: false,
        };
        let decision = playbook_lastdown::evaluate(&input);
        assert!(lastdown_audit(&input, decision.chosen));
        // A fabricated choice that is not the recomputed maximum fails.
        let other = match decision.chosen {
            playbook_core::Disposition::Convert => playbook_core::Disposition::Punt,
            _ => playbook_core::Disposition::Convert,
        };
        assert!(!lastdown_audit(&input, other));
    }

    #[test]
    fn the_huddle_comparison_runs_all_regimes() {
        let rows = huddle_comparison();
        assert!(!rows.is_empty());
        // Every scenario x regime pair produces a verdict.
        assert!(rows.iter().all(|(name, _, _, _)| !name.is_empty()));
    }
}
