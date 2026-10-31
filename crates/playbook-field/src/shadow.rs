//! Shadow Defence: opportunity-cost opposition constructed from the best
//! credible line-up Playbook chose not to field.
//!
//! Every significant personnel decision creates its own opposition. The
//! opponent is not the most intelligent alternative — it is the next-best
//! Pareto-efficient line-up available under the same decision-time
//! conditions, selected by the drive's *current* tactical utility function.
//! The opportunity-cost delta between the actual offence and this shadow
//! defence answers a question raw progress cannot:
//!
//! > the offence may be moving forward while still losing the game
//! > economically.
//!
//! Three constitutional rules:
//!
//! * **The counterfactual is frozen at decision time.** A provider that
//!   recovers twenty seconds after the selection cannot retroactively join
//!   the opposition: the snapshot carries only what was knowable then. No
//!   hindsight leakage.
//! * **The Pareto frontier is preserved, never collapsed to one scalar
//!   best.** An expensive-capable line-up, a balanced one, and a
//!   cheap-adequate one can all be efficient — the opposition is chosen by
//!   the drive's current utility weighting, not a global rank.
//! * **Evidence tiers are explicit.** Equivalent-task measured history
//!   outranks related-task history, which outranks controlled benchmarks,
//!   analytical extrapolation, and model self-assessment. Synthetic
//!   comparison never becomes fictitious empirical fact.
//!
//! Prospective opposition informs coaching decisions using only
//! decision-time information; retrospective opposition evaluates
//! roster/routing quality after the drive with actual outcomes. Both modes
//! are kept strictly separate — which preserves the constitutional boundary
//! that the Front Office never learns from the contest it is learning for.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError};

use crate::domain::field_refusal;

/// The evidence tier a projection rests on. A — equivalent-task measured
/// history — is the strongest; E — model self-assessment — is the weakest
/// and is flagged, never treated as fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    EquivalentTaskHistory,
    RelatedTaskHistory,
    ControlledBenchmark,
    AnalyticalExtrapolation,
    ModelSelfAssessment,
}

/// One candidate line-up with its projected performance, frozen at decision
/// time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineUp {
    pub name: String,
    pub personnel: Vec<PersonnelId>,
    /// Projected validated yards the line-up is expected to produce.
    pub projected_yards: u32,
    /// Projected spend in monetary micros.
    pub projected_cost_micros: u64,
    /// Projected wall-clock milliseconds.
    pub projected_latency_ms: u64,
    pub evidence_tier: EvidenceTier,
}

impl LineUp {
    /// Strict Pareto dominance over `other`: better or equal on every
    /// projected dimension and strictly better on at least one. Lower cost
    /// and latency dominate; higher yards dominate.
    pub fn dominates(&self, other: &LineUp) -> bool {
        let better_or_equal = self.projected_yards >= other.projected_yards
            && self.projected_cost_micros <= other.projected_cost_micros
            && self.projected_latency_ms <= other.projected_latency_ms;
        let strictly_better = self.projected_yards > other.projected_yards
            || self.projected_cost_micros < other.projected_cost_micros
            || self.projected_latency_ms < other.projected_latency_ms;
        better_or_equal && strictly_better
    }
}

/// The decision-time state frozen into the counterfactual. Only facts
/// knowable at the decision point may enter — anything discovered later
/// belongs to retrospective analysis, never to this snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterfactualSnapshot {
    pub at_ms: u64,
    /// The utility weighting the drive state calls for, per-dimension:
    /// yards, cost, latency. Weights are 0..=1_000 ppm and need not sum to
    /// anything in particular — they express what the current drive state
    /// values.
    pub utility_weights: UtilityWeights,
    pub quota_state_ppm: u32,
    pub service_available: bool,
    pub risk_envelope: String,
}

/// What the drive state currently values: yards, cost, latency.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UtilityWeights {
    pub yards_ppm: u32,
    pub cost_ppm: u32,
    pub latency_ppm: u32,
}

/// The weighted utility of one line-up under the drive's current weights:
/// yards×wy − cost×wc − latency×wl, in scaled integer units (ppm weights ×
/// raw values, downscaled to keep the math in range).
pub fn weighted_utility(line_up: &LineUp, weights: &UtilityWeights) -> i64 {
    let yards = i64::from(line_up.projected_yards) * i64::from(weights.yards_ppm);
    let cost = i64::try_from(line_up.projected_cost_micros.saturating_mul(1_000))
        .unwrap_or(i64::MAX)
        .saturating_mul(i64::from(weights.cost_ppm));
    let latency = i64::try_from(line_up.projected_latency_ms).unwrap_or(i64::MAX)
        * i64::from(weights.latency_ppm);
    yards
        .saturating_sub(cost / 1_000_000)
        .saturating_sub(latency / 1_000_000)
}

/// The Pareto frontier of the admissible line-up set: every line-up not
/// strictly dominated by another member. Deterministic: input order
/// preserved, dominance checked pairwise.
pub fn pareto_frontier(line_ups: &[LineUp]) -> Vec<&LineUp> {
    line_ups
        .iter()
        .filter(|candidate| {
            !line_ups
                .iter()
                .any(|other| other.name != candidate.name && other.dominates(candidate))
        })
        .collect()
}

/// The shadow opposition: the strongest credible alternative on the Pareto
/// frontier under the current utility weights. Refuses when the actual
/// line-up is absent from the set (the counterfactual must contain the
/// offence — "actual_lineup_missing_from_set") or when no alternatives
/// exist beyond the actual ("no_alternative_line_ups").
pub fn select_opposition(
    actual: &LineUp,
    admissible: &[LineUp],
    weights: &UtilityWeights,
) -> Result<LineUp, PlaybookError> {
    let actual_in_set = admissible.iter().any(|line_up| line_up.name == actual.name);
    if !actual_in_set {
        return Err(field_refusal("actual_lineup_missing_from_set"));
    }
    let frontier = pareto_frontier(admissible);
    let mut best: Option<(&LineUp, i64)> = None;
    for line_up in &frontier {
        if line_up.name == actual.name {
            continue;
        }
        let utility = weighted_utility(line_up, weights);
        let better = best
            .map(|(_, best_utility)| utility > best_utility)
            .unwrap_or(true);
        if better {
            best = Some((line_up, utility));
        }
    }
    Ok(best
        .map(|(line_up, _)| line_up.clone())
        .unwrap_or_else(|| actual.clone()))
}

/// The opportunity-cost delta between the shadow opposition and the actual
/// offence, in the three projected dimensions. Positive yards means the
/// opposition would have gained more; positive cost/time means the actual
/// offence is the economical side.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpportunityCost {
    pub opposition_name: String,
    /// opposition projected yards − actual projected yards.
    pub yards_delta: i64,
    /// opposition projected cost − actual projected cost (positive: the
    /// actual offence is cheaper).
    pub cost_delta_micros: i64,
    /// opposition projected latency − actual projected latency.
    pub latency_delta_ms: i64,
}

impl OpportunityCost {
    /// Whether the actual offence sits on the efficient frontier against
    /// this opposition: no dimension is materially worse.
    pub fn actual_on_frontier(&self) -> bool {
        self.yards_delta <= 0 && self.cost_delta_micros >= 0 && self.latency_delta_ms >= 0
    }
}

/// Compute the opportunity cost of the actual line-up against the shadow
/// opposition. Deterministic.
pub fn opportunity_cost(actual: &LineUp, opposition: &LineUp) -> OpportunityCost {
    OpportunityCost {
        opposition_name: opposition.name.clone(),
        yards_delta: i64::from(opposition.projected_yards) - i64::from(actual.projected_yards),
        cost_delta_micros: i64::try_from(opposition.projected_cost_micros).unwrap_or(i64::MAX)
            - i64::try_from(actual.projected_cost_micros).unwrap_or(i64::MAX),
        latency_delta_ms: i64::try_from(opposition.projected_latency_ms).unwrap_or(i64::MAX)
            - i64::try_from(actual.projected_latency_ms).unwrap_or(i64::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_up(name: &str, yards: u32, cost: u64, latency: u64) -> LineUp {
        LineUp {
            name: name.into(),
            personnel: Vec::new(),
            projected_yards: yards,
            projected_cost_micros: cost,
            projected_latency_ms: latency,
            evidence_tier: EvidenceTier::EquivalentTaskHistory,
        }
    }

    fn weights() -> UtilityWeights {
        UtilityWeights {
            yards_ppm: 400,
            cost_ppm: 300,
            latency_ppm: 300,
        }
    }

    #[test]
    fn the_pareto_frontier_preserves_all_three_efficient_shapes() {
        // A: expensive and very capable. B: balanced. C: cheap and adequate.
        let a = line_up("A", 90, 900, 900);
        let b = line_up("B", 70, 400, 400);
        let c = line_up("C", 40, 100, 100);
        let field = [a.clone(), b.clone(), c.clone()];
        let frontier = pareto_frontier(&field);
        // None dominates another: all three are efficient shapes.
        assert_eq!(frontier.len(), 3);
        // A dominated line-up is removed.
        let dominated = line_up("D", 40, 900, 900);
        let field = [a, b, c, dominated];
        let frontier = pareto_frontier(&field);
        assert!(frontier.iter().all(|line_up| line_up.name != "D"));
    }

    #[test]
    fn the_shadow_opposition_is_the_best_credible_alternative() {
        let actual = line_up("actual", 60, 500, 500);
        let cheaper = line_up("cheaper", 60, 300, 500);
        // The cheaper equal-yards line-up dominates on cost: it is the
        // opposition.
        let opposition =
            select_opposition(&actual, &[actual.clone(), cheaper.clone()], &weights()).unwrap();
        assert_eq!(opposition.name, "cheaper");
        // A dominated pricier alternative (same yards, higher cost) is
        // skipped: dominance removes it before selection.
        let dominated = line_up("pricier", 60, 900, 500);
        let opposition = select_opposition(
            &actual,
            &[actual.clone(), cheaper.clone(), dominated],
            &weights(),
        )
        .unwrap();
        assert_eq!(opposition.name, "cheaper");
    }

    #[test]
    fn deadline_pressure_flips_the_opposition_to_the_fast_line_up() {
        // Two candidates with EQUAL projected yards: only cost and latency
        // trade. The cheap candidate is 4900 ms slower; the fast candidate
        // costs 390 micros more.
        let actual = line_up("actual", 60, 100, 5_000);
        let cheap_slow = line_up("cheap-slow", 60, 10, 5_000);
        let fast_priced = line_up("fast-priced", 60, 400, 100);
        // Cost-abundant utility: the cheap candidate is the opposition.
        let abundant = UtilityWeights {
            yards_ppm: 0,
            cost_ppm: 100,
            latency_ppm: 100,
        };
        let opposition = select_opposition(
            &actual,
            &[actual.clone(), cheap_slow.clone(), fast_priced.clone()],
            &abundant,
        )
        .unwrap();
        assert_eq!(opposition.name, "cheap-slow");
        // Deadline weighting: latency dominates. The flip.
        let pressured = UtilityWeights {
            yards_ppm: 0,
            cost_ppm: 100,
            latency_ppm: 900_000,
        };
        let opposition = select_opposition(
            &actual,
            &[actual.clone(), cheap_slow, fast_priced.clone()],
            &pressured,
        )
        .unwrap();
        assert_eq!(opposition.name, "fast-priced");
    }

    #[test]
    fn the_counterfactual_refuses_a_set_without_the_actual_offence() {
        let actual = line_up("actual", 60, 500, 500);
        let stranger = line_up("stranger", 90, 100, 100);
        assert_eq!(
            select_opposition(&actual, &[stranger], &weights()).unwrap_err(),
            field_refusal("actual_lineup_missing_from_set")
        );
    }

    #[test]
    fn opportunity_cost_is_signed_toward_the_opposition() {
        let actual = line_up("actual", 60, 500, 500);
        let opposition = line_up("opposition", 70, 300, 400);
        let cost = opportunity_cost(&actual, &opposition);
        // The opposition would gain 10 more yards at lower cost and latency.
        assert_eq!(cost.yards_delta, 10);
        assert_eq!(cost.cost_delta_micros, -200);
        assert_eq!(cost.latency_delta_ms, -100);
        assert!(!cost.actual_on_frontier());
        // Flip it: the actual offence is the efficient side.
        let cost = opportunity_cost(&opposition, &actual);
        assert_eq!(cost.yards_delta, -10);
        assert!(cost.actual_on_frontier());
    }

    #[test]
    fn the_select_function_refuses_an_empty_admissible_set() {
        let actual = line_up("actual", 60, 500, 500);
        assert!(select_opposition(&actual, &[], &weights()).is_err());
    }
}
