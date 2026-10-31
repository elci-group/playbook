//! The Opposition Kicker: initial field construction from a bounded
//! decomposition.
//!
//! Before the offence takes the field, the Opposition Kicker places them as
//! far from unjustified completion as the known task topology warrants. It
//! decomposes the objective into at most 99 productive deliver gates with
//! mandatory and optional work, dependencies, evidence obligations, and
//! consolidation boundaries at every tenth-yard mark. Its posture is
//! conservative but not adversarially pessimistic — the objective is to
//! maximise resistance to false completion while minimising avoidable
//! decomposition error.
//!
//! The Kicker is itself measured. Post-kick scope movement caused by
//! topology discovery is classified by foreseeability; estimation error —
//! scope the original construction should reasonably have accounted for —
//! reduces Kicker accuracy. The Kicker improves through historical evidence,
//! never through model self-confidence.

use serde::{Deserialize, Serialize};

use crate::domain::{field_refusal, validate_gate_count};

/// One decomposed gate as the Kicker places it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSpec {
    pub id: String,
    pub mandatory: bool,
    /// Weight in 1..=1_000_000 ppm of the objective.
    pub weight_ppm: u32,
    pub dependencies: Vec<String>,
    pub required_evidence: Vec<playbook_core::EvidenceClass>,
}

/// The Kicker's input: the objective and its bounded decomposition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecompositionInput {
    pub objective: String,
    pub gates: Vec<GateSpec>,
    /// Known Red Zone conditions the Kicker already sees at construction.
    pub known_red_zone_conditions: Vec<String>,
}

/// The constructed field: the validated decomposition plus the consolidation
/// boundaries the 9+1 rhythm implies for the productive gate sequence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OppositionKick {
    pub objective: String,
    pub gate_count: usize,
    pub mandatory_count: usize,
    /// The summed gate weight across the field, in ppm.
    pub total_weight_ppm: u64,
    pub gates: Vec<GateSpec>,
    /// Consolidation boundaries at every tenth productive gate: 10, 20, 30…
    /// up to the largest multiple of ten at or below the gate count.
    pub consolidation_boundaries: Vec<u32>,
    pub known_red_zone_conditions: Vec<String>,
}

/// Construct the initial field from the decomposition. Validates the 99-gate
/// envelope, refuses duplicate gate ids ("duplicate_gate_id"), zero-weight
/// gates ("zero_weight_gate"), and dependencies referencing unknown ids
/// ("unknown_dependency"). Consolidation boundaries sit at every tenth
/// productive gate — one boundary per ten units of decomposition resolution.
pub fn decompose(
    input: &DecompositionInput,
) -> Result<OppositionKick, playbook_core::PlaybookError> {
    validate_gate_count(input.gates.len())?;
    if input.objective.trim().is_empty() {
        return Err(field_refusal("the kick names its objective"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut total_weight_ppm = 0u64;
    for gate in &input.gates {
        if gate.id.trim().is_empty() {
            return Err(field_refusal("gate id is empty"));
        }
        if !seen.insert(gate.id.as_str()) {
            return Err(field_refusal("duplicate_gate_id"));
        }
        if gate.weight_ppm == 0 {
            return Err(field_refusal("zero_weight_gate"));
        }
        total_weight_ppm += u64::from(gate.weight_ppm);
    }
    for gate in &input.gates {
        for dependency in &gate.dependencies {
            if !seen.contains(dependency.as_str()) {
                return Err(field_refusal(&format!("unknown_dependency {dependency}")));
            }
        }
    }
    let mandatory_count = input.gates.iter().filter(|gate| gate.mandatory).count();
    let gate_count = input.gates.len();
    let mut consolidation_boundaries = Vec::new();
    let seat_count = u64::from(u32::try_from(gate_count).unwrap_or(u32::MAX));
    let mut boundary = 10u32;
    while u64::from(boundary) <= seat_count {
        consolidation_boundaries.push(boundary);
        boundary += 10;
    }
    let _ = seat_count;
    Ok(OppositionKick {
        objective: input.objective.clone(),
        gate_count: input.gates.len(),
        mandatory_count,
        total_weight_ppm,
        gates: input.gates.clone(),
        consolidation_boundaries,
        known_red_zone_conditions: input.known_red_zone_conditions.clone(),
    })
}

/// The Kicker's measured accuracy record (§10). Estimation error is the
/// scope movement the decomposition should reasonably have predicted;
/// exogenous environment change is excluded from the penalty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickerAccuracy {
    pub initial_gate_count: usize,
    pub initial_weight_ppm: u64,
    pub post_kick_added_scope_yards: u32,
    pub post_kick_removed_scope_yards: u32,
    pub avoidable_estimation_error_yards: u32,
    pub unavoidable_environment_change_yards: u32,
}

/// KickerAccuracy in parts-per-million (§10):
///
/// ```text
/// accuracy = 1 - avoidable_post_kick_scope_movement / initial_scope
/// ```
///
/// saturating at 0, with a zero initial scope defined as perfect accuracy
/// (nothing to misjudge). Avoidable movement is estimation error;
/// unavoidable environment change is excluded from the penalty because the
/// Kicker cannot foresee what did not yet exist.
pub fn accuracy_ppm(accuracy: &KickerAccuracy) -> u32 {
    let initial = accuracy.initial_weight_ppm.max(1);
    let avoidable = u64::from(accuracy.avoidable_estimation_error_yards);
    let drift = avoidable.saturating_mul(1_000_000) / initial;
    u32::try_from(1_000_000u64.saturating_sub(drift)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(id: &str, weight: u32, deps: &[&str]) -> GateSpec {
        GateSpec {
            id: id.into(),
            mandatory: true,
            weight_ppm: weight,
            dependencies: deps.iter().map(|d| (*d).to_string()).collect(),
            required_evidence: vec![playbook_core::EvidenceClass::TestReport],
        }
    }

    #[test]
    fn a_three_gate_field_constructs_with_one_boundary() {
        let input = DecompositionInput {
            objective: "repair the parser".into(),
            gates: vec![
                gate("parse", 400_000, &[]),
                gate("validate", 300_000, &["parse"]),
                gate("serialize", 300_000, &["parse"]),
            ],
            known_red_zone_conditions: vec!["test suite green".into()],
        };
        let kick = decompose(&input).unwrap();
        assert_eq!(kick.gate_count, 3);
        assert_eq!(kick.mandatory_count, 3);
        assert_eq!(kick.total_weight_ppm, 1_000_000);
        // Three productive gates never reach the first tenth-gate boundary:
        // consolidation boundaries start at every tenth productive gate.
        assert!(kick.consolidation_boundaries.is_empty());
    }

    #[test]
    fn boundaries_scale_with_the_decomposition() {
        let gates: Vec<GateSpec> = (0..25)
            .map(|index| gate(&format!("g{index}"), 40_000, &[]))
            .collect();
        let input = DecompositionInput {
            objective: "wide decomposition".into(),
            gates,
            known_red_zone_conditions: Vec::new(),
        };
        let kick = decompose(&input).unwrap();
        // 25 productive gates: boundaries after gates 10 and 20.
        assert_eq!(kick.consolidation_boundaries, vec![10, 20]);
    }

    #[test]
    fn the_decomposition_envelope_and_integrity_rules_hold() {
        // 100 gates exceed the envelope.
        let gates: Vec<GateSpec> = (0..100)
            .map(|index| gate(&format!("g{index}"), 10_000, &[]))
            .collect();
        let oversized = DecompositionInput {
            objective: "too fine".into(),
            gates,
            known_red_zone_conditions: Vec::new(),
        };
        assert!(decompose(&oversized).is_err());
        // Duplicate ids refused.
        let duplicated = DecompositionInput {
            objective: "dup".into(),
            gates: vec![gate("g", 500_000, &[]), gate("g", 500_000, &[])],
            known_red_zone_conditions: Vec::new(),
        };
        assert_eq!(
            decompose(&duplicated).unwrap_err(),
            field_refusal("duplicate_gate_id")
        );
        // Unknown dependency refused.
        let dangling = DecompositionInput {
            objective: "dangling".into(),
            gates: vec![gate("g", 500_000, &["phantom"])],
            known_red_zone_conditions: Vec::new(),
        };
        assert!(matches!(
            decompose(&dangling).unwrap_err(),
            playbook_core::PlaybookError::Inadmissible(_, _)
        ));
        // Zero weight refused.
        let weightless = DecompositionInput {
            objective: "weightless".into(),
            gates: vec![gate("g", 0, &[])],
            known_red_zone_conditions: Vec::new(),
        };
        assert_eq!(
            decompose(&weightless).unwrap_err(),
            field_refusal("zero_weight_gate")
        );
    }

    #[test]
    fn accuracy_is_one_minus_avoidable_error_over_initial_scope() {
        let perfect = KickerAccuracy {
            initial_gate_count: 12,
            initial_weight_ppm: 1_000_000,
            post_kick_added_scope_yards: 0,
            post_kick_removed_scope_yards: 0,
            avoidable_estimation_error_yards: 0,
            unavoidable_environment_change_yards: 300,
        };
        assert_eq!(accuracy_ppm(&perfect), 1_000_000);
        // Half the initial scope was avoidable misjudgement: 500_000.
        let half = KickerAccuracy {
            avoidable_estimation_error_yards: 500_000,
            initial_weight_ppm: 1_000_000,
            ..perfect
        };
        assert_eq!(accuracy_ppm(&half), 500_000);
        // Unavoidable environment change is excluded from the penalty.
        let env_only = KickerAccuracy {
            avoidable_estimation_error_yards: 0,
            unavoidable_environment_change_yards: 400_000,
            initial_weight_ppm: 1_000_000,
            ..perfect
        };
        assert_eq!(accuracy_ppm(&env_only), 1_000_000);
        // Beyond the initial scope saturates at 0.
        let worse = KickerAccuracy {
            avoidable_estimation_error_yards: 2_000_000,
            initial_weight_ppm: 1_000_000,
            ..perfect
        };
        assert_eq!(accuracy_ppm(&worse), 0);
    }

    #[test]
    fn construction_is_deterministic() {
        let input = DecompositionInput {
            objective: "repair".into(),
            gates: vec![
                gate("a", 300_000, &[]),
                gate("b", 300_000, &["a"]),
                gate("c", 400_000, &["a"]),
            ],
            known_red_zone_conditions: Vec::new(),
        };
        assert_eq!(decompose(&input).unwrap(), decompose(&input).unwrap());
    }
}
