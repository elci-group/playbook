//! Unresolved task distance.
//!
//! `YTG = Σ wᵢ(1 − sᵢ)`, scaled to Lastdown's 10.000 yard reference.
//! `sᵢ` moves only when a gate has evidence or an explicit waiver.

#![forbid(unsafe_code)]

use playbook_core::{Gate, PPM_SCALE, YTG_REFERENCE_MILLI};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Efficiency {
    pub yards_gained_ppm: i64,
    pub budget_spent_ppm: i64,
    pub ratio_milli: i64,
}

pub fn ytg_milli(gates: &[Gate]) -> u32 {
    let mut weighted_open: u128 = 0;
    let mut total: u128 = 0;
    for gate in gates {
        let weight = u128::from(gate.weight_ppm);
        let satisfaction = u128::from(gate.effective_satisfaction_ppm());
        total += weight;
        weighted_open += weight.saturating_mul(u128::from(PPM_SCALE) - satisfaction);
    }
    if total == 0 {
        return 0;
    }
    let milli = weighted_open * u128::from(YTG_REFERENCE_MILLI) / (total * u128::from(PPM_SCALE));
    u32::try_from(milli).unwrap_or(YTG_REFERENCE_MILLI)
}

pub fn gate_unresolved_milli(gate: &Gate, gates: &[Gate]) -> u32 {
    if gate.is_terminal() {
        return 0;
    }
    let total: u128 = gates.iter().map(|item| u128::from(item.weight_ppm)).sum();
    if total == 0 {
        return 0;
    }
    let open = u128::from(gate.weight_ppm)
        * (u128::from(PPM_SCALE) - u128::from(gate.effective_satisfaction_ppm()));
    let milli = open * u128::from(YTG_REFERENCE_MILLI) / (total * u128::from(PPM_SCALE));
    u32::try_from(milli).unwrap_or(0)
}

/// Positive ratio means yards gained per budget spent. Constituents stay available.
pub fn drive_efficiency(
    ytg_before: u32,
    ytg_after: u32,
    budget_before_ppm: u32,
    budget_after_ppm: u32,
) -> Efficiency {
    let yards_gained_ppm = if ytg_before == 0 {
        0
    } else {
        (i64::from(ytg_before) - i64::from(ytg_after)) * i64::from(PPM_SCALE)
            / i64::from(ytg_before)
    };
    let budget_spent_ppm = i64::from(budget_before_ppm) - i64::from(budget_after_ppm);
    let ratio_milli = if budget_spent_ppm <= 0 {
        0
    } else {
        yards_gained_ppm * 1000 / budget_spent_ppm
    };
    Efficiency {
        yards_gained_ppm,
        budget_spent_ppm,
        ratio_milli,
    }
}

/// The multi-dimensional progress reading. The vector is authoritative: a
/// scalar score may be derived from it for routing convenience, never the
/// reverse — apparent progress must not be able to hide a regression in one
/// dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressVector {
    /// Weight share of terminal gates, in ppm of the total declared weight.
    pub resolved_requirements_ppm: u32,
    /// Weight share of gates still open, in ppm. Resolved + unresolved +
    /// failing always reconstructs the whole.
    pub unresolved_requirements_ppm: u32,
    /// Number of gates in the `Failed` state.
    pub failing_gates: u32,
    /// Weight share of never-assessed gates, in ppm: how much of the field
    /// has not even been scouted.
    pub uncertainty_ppm: u32,
    /// Evidence-bearing satisfaction over total satisfaction, in ppm: how
    /// much of the claimed progress rests on sealed evidence or waivers
    /// rather than nothing.
    pub evidence_strength_ppm: u32,
    pub regression_count: u32,
}

/// Derive the authoritative vector from the declared gates. Deterministic.
pub fn progress_vector(gates: &[Gate], regression_count: u32) -> ProgressVector {
    let total: u128 = gates.iter().map(|gate| u128::from(gate.weight_ppm)).sum();
    let share = |weight: u128| -> u32 {
        weight
            .checked_mul(u128::from(PPM_SCALE))
            .and_then(|scaled| scaled.checked_div(total))
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0)
    };
    let mut resolved = 0u128;
    let mut unresolved = 0u128;
    let mut uncertainty = 0u128;
    let mut failing = 0u32;
    let mut satisfied_weight = 0u128;
    let mut evidenced_weight = 0u128;
    for gate in gates {
        let weight = u128::from(gate.weight_ppm);
        match gate.state {
            playbook_core::GateState::Satisfied | playbook_core::GateState::Waived => {
                resolved += weight;
                satisfied_weight += weight;
                if gate.satisfaction_source != playbook_core::SatisfactionSource::Unset {
                    evidenced_weight += weight;
                }
            }
            playbook_core::GateState::Failed => {
                failing += 1;
                unresolved += weight;
            }
            playbook_core::GateState::Unassessed => uncertainty += weight,
            _ => unresolved += weight,
        }
    }
    ProgressVector {
        resolved_requirements_ppm: share(resolved),
        unresolved_requirements_ppm: share(unresolved),
        failing_gates: failing,
        uncertainty_ppm: share(uncertainty),
        evidence_strength_ppm: evidenced_weight
            .checked_mul(u128::from(PPM_SCALE))
            .and_then(|scaled| scaled.checked_div(satisfied_weight))
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0),
        regression_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{EvidenceClass, Gate, GateId, GateState, SatisfactionSource};

    fn gate(id: &str, weight: u32, satisfaction: u32) -> Gate {
        let mut gate = Gate::declare(
            GateId::new(id).unwrap(),
            id,
            weight,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap();
        gate.satisfaction_ppm = satisfaction;
        gate.satisfaction_source = if satisfaction > 0 {
            SatisfactionSource::Evidence
        } else {
            SatisfactionSource::Unset
        };
        gate.state = if satisfaction == PPM_SCALE {
            GateState::Satisfied
        } else if satisfaction > 0 {
            GateState::InProgress
        } else {
            GateState::Unassessed
        };
        gate
    }

    #[test]
    fn weighted_example_is_3500_milli() {
        let gates = vec![
            gate("implementation", 250_000, PPM_SCALE),
            gate("unit_tests", 200_000, PPM_SCALE),
            gate("integration_tests", 200_000, PPM_SCALE / 2),
            gate("windows_validation", 150_000, 0),
            gate("benchmark_regression", 100_000, PPM_SCALE),
            gate("commit_push", 100_000, 0),
        ];
        assert_eq!(ytg_milli(&gates), 3_500);
    }

    #[test]
    fn efficiency_keeps_the_constituents() {
        let efficiency = drive_efficiency(10_000, 6_000, PPM_SCALE, 800_000);
        assert_eq!(efficiency.yards_gained_ppm, 400_000);
        assert_eq!(efficiency.budget_spent_ppm, 200_000);
        assert!(efficiency.ratio_milli > 0);
    }

    #[test]
    fn the_vector_reconstructs_the_scalar_and_names_every_dimension() {
        let gates = vec![
            gate("implementation", 250_000, PPM_SCALE),
            gate("unit_tests", 200_000, PPM_SCALE),
            gate("integration_tests", 200_000, PPM_SCALE / 2),
            gate("windows_validation", 150_000, 0),
            gate("benchmark_regression", 100_000, PPM_SCALE),
            gate("commit_push", 100_000, 0),
        ];
        let vector = progress_vector(&gates, 1);
        // The resolved, unresolved, and uncertainty shares reconstruct the
        // whole declared weight.
        assert_eq!(
            PPM_SCALE,
            vector.resolved_requirements_ppm
                + vector.unresolved_requirements_ppm
                + vector.uncertainty_ppm
        );
        // The scalar is a derivation of the vector, never its source.
        assert_eq!(ytg_milli(&gates), 3_500);
        assert_eq!(vector.failing_gates, 0);
        assert_eq!(vector.regression_count, 1);
        // Satisfied gates rest on evidence; the in-progress half-gate
        // carries satisfaction with evidence too.
        assert_eq!(vector.evidence_strength_ppm, PPM_SCALE);
    }

    #[test]
    fn failed_and_unassessed_gates_are_distinguished() {
        let mut failed = gate("build", 500_000, 0);
        failed.state = GateState::Failed;
        let gates = vec![failed, gate("docs", 500_000, 0)];
        let vector = progress_vector(&gates, 0);
        assert_eq!(vector.failing_gates, 1);
        assert_eq!(vector.unresolved_requirements_ppm, 500_000);
        assert_eq!(vector.uncertainty_ppm, 500_000);
        assert_eq!(vector.resolved_requirements_ppm, 0);
        // No satisfaction anywhere: evidence strength is vacuously zero.
        assert_eq!(vector.evidence_strength_ppm, 0);
    }

    #[test]
    fn unevidenced_satisfaction_is_weak_progress() {
        let mut claimed = gate("build", PPM_SCALE, PPM_SCALE);
        claimed.satisfaction_source = playbook_core::SatisfactionSource::Unset;
        let vector = progress_vector(&[claimed], 0);
        assert_eq!(vector.resolved_requirements_ppm, PPM_SCALE);
        // Claimed progress with no evidence has zero strength: the vector
        // refuses to dress assertion up as measurement.
        assert_eq!(vector.evidence_strength_ppm, 0);
    }
}
