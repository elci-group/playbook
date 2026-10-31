//! The value math: utility, opportunity-cost-adjusted value, regret, and
//! Shadow Defence prediction scoring. The lab's own arithmetic — it never
//! asks the subject what it is worth.

use serde::{Deserialize, Serialize};

/// One measured outcome, in the lab's own units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Utility {
    /// Evidence-backed yards gained, in milli-yards.
    pub validated_progress_milli: u32,
    /// All mandatory Deliver Gates passed (§12) — the lab's gate result,
    /// never the subject's claim.
    pub completed: bool,
    /// Total monetary cost, in micros.
    pub cost_micros: u64,
    pub latency_ms: u64,
    /// The run stayed inside its declared constraints.
    pub constraint_compliant: bool,
    /// An irreversible failure occurred (destructive side effect, lost
    /// possession of the artifact).
    pub catastrophic_failure: bool,
}

/// The utility weights, frozen at pre-registration (§27). Weights are
/// 0..=1_000_000 ppm and frozen into the protocol hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtilityWeights {
    pub completion_ppm: u32,
    pub progress_ppm: u32,
    pub cost_ppm: u32,
    pub latency_ppm: u32,
    pub compliance_ppm: u32,
    /// The penalty for one catastrophic failure, in scaled units.
    pub catastrophe_penalty: i64,
}

impl UtilityWeights {
    /// The pre-registered default weighting.
    pub fn registered() -> Self {
        Self {
            completion_ppm: 1_000_000,
            progress_ppm: 100_000,
            cost_ppm: 100_000,
            latency_ppm: 10_000,
            compliance_ppm: 200_000,
            catastrophe_penalty: 10_000_000,
        }
    }
}

/// `U(outcome)` under frozen weights. Signed: a catastrophe dominates.
pub fn utility(outcome: &Utility, weights: &UtilityWeights) -> i64 {
    let completion = if outcome.completed {
        i64::from(weights.completion_ppm)
    } else {
        0
    };
    let progress =
        i64::from(outcome.validated_progress_milli) * i64::from(weights.progress_ppm) / 1_000;
    let cost = i64::try_from(outcome.cost_micros / 1_000)
        .unwrap_or(i64::MAX)
        .saturating_mul(i64::from(weights.cost_ppm))
        / 1_000_000;
    let latency = (i64::try_from(outcome.latency_ms)
        .unwrap_or(i64::MAX)
        .saturating_mul(i64::from(weights.latency_ppm)))
        / 1_000;
    let compliance = if outcome.constraint_compliant {
        i64::from(weights.compliance_ppm)
    } else {
        0
    };
    let catastrophe = if outcome.catastrophic_failure {
        weights.catastrophe_penalty
    } else {
        0
    };
    completion + progress - cost - latency + compliance - catastrophe
}

/// Opportunity-Cost Adjusted Value (§3): the utility of the actual outcome
/// minus the utility of the best credible alternative available at
/// decision time. The alternative is reconstructed by the lab from the
/// frozen decision snapshot — hindsight does not enter.
pub fn ocav(actual_utility: i64, best_alternative_utility: i64) -> i64 {
    actual_utility.saturating_sub(best_alternative_utility)
}

/// Decision regret (§16): what the best available action would have
/// earned minus what the selected action earned, in the lab's units.
pub fn regret(best_available_utility: i64, selected_utility: i64) -> i64 {
    (best_available_utility - selected_utility).max(0)
}

/// A Shadow Defence prediction made at decision time, frozen before the
/// outcome existed (§15).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowPrediction {
    /// The decision snapshot this prediction was frozen against.
    pub decision_ref: String,
    pub selected_name: String,
    /// Shadow Defence's predicted utility for the selected line-up.
    pub selected_predicted: i64,
    pub opposition_name: String,
    /// Shadow Defence's predicted utility for the best unfielded line-up.
    pub opposition_predicted: i64,
}

/// The lab's scoring of Shadow Defence after outcomes exist (§15):
/// calibration, ranking accuracy, regret estimate error — and agreement
/// with the lab's own counterfactual, which is itself a measured
/// variable (§3: Shadow Defence shall not validate itself).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadowScorecard {
    pub predictions: u32,
    /// Mean |predicted − actual| over the selected line-up, in lab units.
    pub calibration_error_mean: i64,
    /// How often the shadow correctly predicted the selected line-up would
    /// beat the unfielded opposition.
    pub ranking_accuracy_ppm: u32,
    /// Mean |predicted regret − measured regret|.
    pub regret_error_mean: i64,
    /// How often the shadow named the same opposition as the lab's
    /// independent reconstruction.
    pub agreement_ppm: u32,
}

/// Score frozen predictions against measured outcomes. `actual` maps a
/// decision reference to the measured utility of the selected line-up;
/// `opposition_actual` to the measured utility of the shadow's named
/// opposition; `lab_regret` to the lab's own measured regret.
pub fn score_shadow(
    predictions: &[ShadowPrediction],
    actual: &dyn Fn(&ShadowPrediction) -> Option<i64>,
    opposition_actual: &dyn Fn(&ShadowPrediction) -> Option<i64>,
    lab_regret: &dyn Fn(&ShadowPrediction) -> Option<i64>,
) -> ShadowScorecard {
    let mut card = ShadowScorecard {
        predictions: predictions.len() as u32,
        ..ShadowScorecard::default()
    };
    let mut calibration_total: i64 = 0;
    let mut calibration_count: i64 = 0;
    let mut ranking_hits: u32 = 0;
    let mut ranking_count: u32 = 0;
    let mut regret_total: i64 = 0;
    let mut regret_count: i64 = 0;
    let mut agreement_hits: u32 = 0;
    let mut agreement_count: u32 = 0;
    for prediction in predictions {
        if let Some(measured) = actual(prediction) {
            calibration_total += (prediction.selected_predicted - measured).abs();
            calibration_count += 1;
        }
        if let Some(measured) = opposition_actual(prediction) {
            ranking_count += 1;
            // The shadow's claim: the selected line-up is at least as good
            // as the unfielded opposition.
            if prediction.selected_predicted >= prediction.opposition_predicted {
                // Claiming dominance: it holds only if the opposition
                // actually measured no better than the selected line-up.
                if measured <= actual(prediction).unwrap_or(i64::MIN) {
                    ranking_hits += 1;
                }
            } else {
                // Claiming the opposition would have won: it holds only if
                // the opposition really did measure better.
                if measured > actual(prediction).unwrap_or(i64::MAX) {
                    ranking_hits += 1;
                }
            }
        }
        if let Some(measured) = lab_regret(prediction) {
            regret_total += (lab_regret_estimate(prediction) - measured).abs();
            regret_count += 1;
        }
        // Agreement: did the shadow's opposition turn out to be a real
        // threat — i.e. its predicted direction matched the lab's regret
        // direction?
        if let Some(measured) = lab_regret(prediction) {
            agreement_count += 1;
            let shadow_threat = prediction.opposition_predicted > prediction.selected_predicted;
            let lab_threat = measured > 0;
            if shadow_threat == lab_threat {
                agreement_hits += 1;
            }
        }
    }
    if calibration_count > 0 {
        card.calibration_error_mean = calibration_total / calibration_count;
    }
    if ranking_count > 0 {
        card.ranking_accuracy_ppm =
            ((ranking_hits as u64 * 1_000_000) / ranking_count as u64) as u32;
    }
    if regret_count > 0 {
        card.regret_error_mean = regret_total / regret_count;
    }
    if agreement_count > 0 {
        card.agreement_ppm = ((agreement_hits as u64 * 1_000_000) / agreement_count as u64) as u32;
    }
    card
}

/// The shadow's implied regret estimate: how much better it thought the
/// opposition was.
pub fn lab_regret_estimate(prediction: &ShadowPrediction) -> i64 {
    (prediction.opposition_predicted - prediction.selected_predicted).max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(progress: u32, completed: bool, cost: u64) -> Utility {
        Utility {
            validated_progress_milli: progress,
            completed,
            cost_micros: cost,
            latency_ms: 1_000,
            constraint_compliant: true,
            catastrophic_failure: false,
        }
    }

    #[test]
    fn completion_dominates_and_catastrophe_swamps() {
        let weights = UtilityWeights::registered();
        let done = utility(&outcome(0, true, 1_000), &weights);
        let partial = utility(&outcome(500, false, 1_000), &weights);
        assert!(done > partial, "completion outweighs bare progress");
        let mut disaster = outcome(0, true, 1_000);
        disaster.catastrophic_failure = true;
        assert!(
            utility(&disaster, &weights) < utility(&outcome(0, false, 0), &weights),
            "one catastrophe swamps a completed drive"
        );
    }

    #[test]
    fn ocav_and_regret_are_signed_toward_the_alternative() {
        assert_eq!(ocav(500, 400), 100, "beating the alternative is positive");
        assert_eq!(
            ocav(400, 500),
            -100,
            "losing to the alternative is negative"
        );
        assert_eq!(regret(500, 400), 100);
        assert_eq!(regret(400, 500), 0, "no regret for the winning choice");
    }

    #[test]
    fn shadow_scoring_rewards_calibration_and_honest_ranking() {
        let weights = UtilityWeights::registered();
        let actual_selected = utility(&outcome(0, true, 1_000), &weights);
        let prediction = ShadowPrediction {
            decision_ref: "d1".into(),
            selected_name: "luna".into(),
            selected_predicted: actual_selected,
            opposition_name: "glm".into(),
            opposition_predicted: actual_selected - 100,
        };
        let card = score_shadow(
            &[prediction],
            &|_| Some(actual_selected),
            &|_| Some(actual_selected - 150),
            &|_| Some(0),
        );
        assert_eq!(card.calibration_error_mean, 0);
        assert_eq!(card.ranking_accuracy_ppm, 1_000_000, "dominance held");
        assert_eq!(card.agreement_ppm, 1_000_000, "both said no threat");
    }

    #[test]
    fn shadow_scoring_catches_a_wrong_dominance_claim() {
        let weights = UtilityWeights::registered();
        let actual_selected = utility(&outcome(0, true, 1_000), &weights);
        let prediction = ShadowPrediction {
            decision_ref: "d1".into(),
            selected_name: "luna".into(),
            selected_predicted: actual_selected,
            opposition_name: "glm".into(),
            opposition_predicted: actual_selected - 100,
        };
        // The opposition actually measured BETTER: the dominance claim was
        // wrong, and the lab measured regret where the shadow saw none.
        let card = score_shadow(
            &[prediction],
            &|_| Some(actual_selected),
            &|_| Some(actual_selected + 200),
            &|_| Some(200),
        );
        assert_eq!(card.ranking_accuracy_ppm, 0);
        assert_eq!(card.agreement_ppm, 0, "shadow said safe, lab said threat");
        assert!(card.regret_error_mean > 0);
    }
}
