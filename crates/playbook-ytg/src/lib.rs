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
}
