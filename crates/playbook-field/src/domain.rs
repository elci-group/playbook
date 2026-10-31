//! The frozen field-geometry contract (Huddle A).
//!
//! The YTG revision turns Playbook's field metaphor into a deterministic
//! completion-economics protocol. Every shared type lives here; the
//! specialised modules (geometry, kicker, penalties, red zone, field goal,
//! routing) compute over these types and may not redefine them.
//!
//! The doctrine, compressed:
//!
//! * progress exists in two materially different states — banked (survived
//!   a validation boundary, recoverable from the journal, attributable to
//!   its evidence) and unvalidated (provisional since the last boundary);
//! * no normal possession accumulates more than nine yards of unvalidated
//!   progress — every tenth yard is reserved for validation;
//! * a validation boundary is not traversable by ordinary implementation
//!   activity: it requires test, benchmark, or human-input evidence, or an
//!   admitted combination;
//! * penalties are causal field adjustments with provenance — a penalty is
//!   the field geometry becoming less or more favourable, never a mere
//!   failed play;
//! * the Red Zone is closure feasibility, not fixed yardage;
//! * field-goal range is terminal-action economics, not fixed yardage;
//! * the two are independent and shall not be conflated.
//!
//! The gate graph remains the authoritative record. These types derive the
//! geometry FROM that graph; they never replace it.

use serde::{Deserialize, Serialize};

use playbook_core::{Gate, PlaybookError};

/// The maximum productive decomposition resolution for one drive: 99
/// productive deliver gates. The number is a ceiling on decomposition
/// granularity — not ninety-nine mandatory steps, not equal weight, not a
/// requirement that complex work consume the full field.
pub const MAX_PRODUCTIVE_GATES: usize = 99;

/// Yards per validation segment. Every tenth yard is structurally reserved
/// for validation, consolidation, or authority input.
pub const SEGMENT_YARDS: u32 = 10;

/// The unvalidated-progress ceiling under normal possession: nine yards.
pub const MAX_UNVALIDATED_YARDS: u32 = 9;

/// The error the field geometry refuses with.
pub fn field_refusal(reason: &str) -> PlaybookError {
    PlaybookError::Inadmissible("field".into(), reason.into())
}

/// The evidence class a validation boundary requires. Test-constrained
/// boundaries suit correctness-bearing work; benchmark-constrained suit
/// measured-quality work; human-input-constrained suit conditions machine
/// evidence cannot authoritatively resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationEvidence {
    Test,
    Benchmark,
    HumanInput,
}

/// The all-of requirement at one tenth-yard boundary. Combinations ("test +
/// benchmark", "benchmark + human approval") are expressed as multiple
/// entries: every required class must be presented.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryRequirement {
    pub required: Vec<ValidationEvidence>,
}

impl BoundaryRequirement {
    /// A requirement with at least one class is constructible; an empty
    /// requirement would make the boundary traversable without evidence.
    pub fn new(required: Vec<ValidationEvidence>) -> Result<Self, PlaybookError> {
        if required.is_empty() {
            return Err(field_refusal(
                "a validation boundary requires at least one evidence class",
            ));
        }
        Ok(Self { required })
    }

    /// Whether a presented evidence set satisfies the requirement: every
    /// required class must be presented at least once.
    pub fn satisfied_by(&self, presented: &[ValidationEvidence]) -> bool {
        self.required
            .iter()
            .all(|required| presented.contains(required))
    }
}

/// The lifecycle of one validation boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryState {
    Pending,
    Passed,
    Failed,
}

/// One tenth-yard validation boundary in the drive's geometry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationBoundary {
    /// The yard this boundary sits on (10, 20, 30…).
    pub yard: u32,
    pub requirement: BoundaryRequirement,
    pub state: BoundaryState,
}

/// The two progress states, with the invariant that unvalidated progress
/// never exceeds nine yards under normal possession. Banked progress
/// survived its validation boundary; unvalidated progress is provisional.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldPosition {
    /// Yards banked at passed validation boundaries.
    pub banked: u32,
    /// Yards gained since the last passed boundary.
    pub unvalidated: u32,
}

/// Why a progression recording was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressRefusal {
    /// The recording would push unvalidated progress past nine yards: the
    /// next tenth yard is reserved for validation.
    ValidationBoundaryReached,
}

impl FieldPosition {
    /// The observed position: banked plus unvalidated.
    pub fn observed(&self) -> u32 {
        self.banked.saturating_add(self.unvalidated)
    }

    /// Record productive yardage. Refuses when the recording would exceed
    /// the nine-yard unvalidated ceiling: the boundary must be validated
    /// before further ordinary progression.
    pub fn record_progress(&mut self, yards: u32) -> Result<(), ProgressRefusal> {
        if self.unvalidated.saturating_add(yards) > MAX_UNVALIDATED_YARDS {
            return Err(ProgressRefusal::ValidationBoundaryReached);
        }
        self.unvalidated = self.unvalidated.saturating_add(yards);
        Ok(())
    }

    /// Pass the validation boundary at `yard`: bank the preceding segment,
    /// reset unvalidated progress, retain possession.
    pub fn bank(&mut self, yard: u32) {
        self.banked = self.banked.max(yard);
        self.unvalidated = 0;
    }
}

/// The direction a field adjustment moves the geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentDirection {
    /// Newly evidenced obligations make the field harder (offensive
    /// penalty).
    Harder,
    /// Newly evidenced information removes or simplifies assumed work
    /// (defensive penalty).
    Easier,
}

/// Why the field moved. The four-way distinction is mandatory provenance:
/// exogenous change is nobody's fault, discovery is new evidence,
/// estimation error belongs to the Opposition Kicker, and
/// execution-created scope is the offence's own doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Foreseeability {
    /// The environment or requirement changed outside the offence's control.
    Exogenous,
    /// Previously unavailable information became evidenced.
    Discovery,
    /// The original field construction should reasonably have accounted for
    /// this — attributed to the Opposition Kicker's quality.
    EstimationError,
    /// The offence itself generated the new work (regression, broken
    /// compatibility, unnecessary migration burden).
    ExecutionCreated,
}

/// One causal field adjustment (§14 provenance record). Penalties carry
/// this provenance to prevent gaming: topology movement is never silently
/// credited to execution, and execution-created scope never masquerades as
/// exogenous change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldAdjustment {
    pub adjustment_id: String,
    pub direction: AdjustmentDirection,
    /// The magnitude in equivalent weighted yards.
    pub magnitude_yards: u32,
    /// The machine cause code.
    pub cause: String,
    /// The evidence that established the adjustment.
    pub evidence_ref: Option<playbook_core::EvidenceId>,
    pub foreseeability: Foreseeability,
    /// The gate ids whose geometry the adjustment touches.
    pub affected_gates: Vec<playbook_core::GateId>,
    pub at_ms: u64,
}

impl FieldAdjustment {
    /// A field adjustment requires a cause, at least one affected gate, and
    /// evidence: an unattributed penalty is a fabricated one.
    pub fn validate(&self) -> Result<(), PlaybookError> {
        if self.cause.trim().is_empty() {
            return Err(field_refusal("adjustment requires a cause"));
        }
        if self.affected_gates.is_empty() {
            return Err(field_refusal("adjustment names its affected gates"));
        }
        if self.evidence_ref.is_none() {
            return Err(field_refusal(
                "adjustment requires establishing evidence — topology movement without evidence is unverifiable",
            ));
        }
        Ok(())
    }

    /// Whether this adjustment is attributable to the Opposition Kicker's
    /// initial decomposition quality (estimation error reduces Kicker
    /// accuracy) rather than to active personnel.
    pub fn is_kicker_estimation_error(&self) -> bool {
        self.foreseeability == Foreseeability::EstimationError
    }
}

/// The attribution class for field movement, separating earned execution
/// yards from topology movement in personnel priors (§15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementAttribution {
    /// Yards earned through execution.
    ExecutionProgress,
    /// Yards lost through execution regression.
    ExecutionRegression,
    /// Field hardened by newly evidenced obligations.
    OffensivePenalty,
    /// Field eased by newly evidenced simplification.
    DefensivePenalty,
    /// Movement caused by validation accounting.
    ValidationAdjustment,
    /// Movement caused by the external environment.
    ExternalEnvironmentAdjustment,
}

/// The semantic Red Zone (§8): every remaining mandatory gate is
/// session-closable — required resources are available, dependencies are
/// reachable, validation can occur, and no blocker makes completion
/// contingent on an unavailable future event. Not a yardage threshold.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedZoneAssessment {
    pub entered: bool,
    /// The mandatory gates still outstanding when the assessment ran.
    pub outstanding_mandatory: Vec<playbook_core::GateId>,
    /// Blockers that prevent session closure, when any.
    pub blockers: Vec<String>,
}

/// The terminal-action economics of one candidate kick (§17): the
/// comparison between one bounded decisive action and continued
/// progression, in utility units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickUtility {
    /// P(score) × V(goal) − C(kick) − R(kick), in utility units.
    pub kick_utility: i64,
    /// P(progress to score) × V(goal) − C(drive) − R(drive).
    pub drive_utility: i64,
}

impl KickUtility {
    /// Field-goal range: one bounded action dominates continued progression.
    pub fn in_range(&self) -> bool {
        self.kick_utility > self.drive_utility
    }
}

/// The §19 auditable range decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldGoalAssessment {
    pub candidate_action: String,
    pub candidate_personnel: String,
    pub score_probability_ppm: u32,
    pub expected_cost: u64,
    pub expected_latency_ms: u64,
    pub expected_downside: i64,
    pub continued_drive_cost: u64,
    pub continued_drive_risk: i64,
    pub utility: KickUtility,
    pub admitted: bool,
    pub reasons: Vec<String>,
}

/// The terminal outcome classes of one offensive kick attempt (§21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickOutcome {
    /// The action satisfied the goal and produced the required evidence.
    Score,
    /// The attempt failed without materially harming state; possession may
    /// resume from the prior validated frontier.
    CleanMiss,
    /// The action could not validly execute: external dependency,
    /// capability failure, environmental constraint.
    Blocked,
    /// The attempt produced unacceptable irreversible harm: destructive
    /// side effects, exhausted one-shot resources, policy breach.
    Turnover,
}

impl KickOutcome {
    /// Whether the kick scored.
    pub fn scored(self) -> bool {
        self == Self::Score
    }

    /// Whether possession may resume from the prior validated frontier.
    pub fn possession_recoverable(self) -> bool {
        matches!(self, Self::Score | Self::CleanMiss)
    }
}

/// Validate the productive gate count against the 99-gate decomposition
/// envelope (§2). The Opposition Kicker may not construct a field larger
/// than the maximum resolution, and a drive may not exist with zero gates.
pub fn validate_gate_count(gates: usize) -> Result<(), PlaybookError> {
    if gates == 0 {
        return Err(field_refusal("a drive requires at least one gate"));
    }
    if gates > MAX_PRODUCTIVE_GATES {
        return Err(field_refusal(&format!(
            "decomposition exceeds the {MAX_PRODUCTIVE_GATES}-gate envelope"
        )));
    }
    Ok(())
}

/// The consolidation checkpoint questions a passed boundary must answer
/// (§6): existence, individual validity, dependency validity, composition
/// validity, regression state, objective alignment. Each question is
/// answered with the evidence that settles it; a boundary pass requires all
/// questions answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsolidationCheckpoint {
    /// Answers keyed by question name: "existence", "individual_validity",
    /// "dependency_validity", "composition_validity", "regression_state",
    /// "objective_alignment".
    pub answers: std::collections::BTreeMap<String, playbook_core::EvidenceId>,
}

impl ConsolidationCheckpoint {
    /// The six mandatory question names, in order.
    pub const QUESTIONS: [&'static str; 6] = [
        "existence",
        "individual_validity",
        "dependency_validity",
        "composition_validity",
        "regression_state",
        "objective_alignment",
    ];

    /// All six questions answered with evidence.
    pub fn complete(&self) -> bool {
        Self::QUESTIONS
            .iter()
            .all(|question| self.answers.contains_key(*question))
    }
}

/// The total field length in yards over a gate set: the weighted
/// evidence-backed distance projected onto the yard scale, with validation
/// boundaries every [`SEGMENT_YARDS`] yards.
pub fn field_length_yards(gates: &[Gate], reference_milli: u32) -> u32 {
    let mut weighted_open: u128 = 0;
    let mut total: u128 = 0;
    for gate in gates {
        let weight = u128::from(gate.weight_ppm);
        let satisfaction = u128::from(gate.effective_satisfaction_ppm());
        total += weight;
        weighted_open += weight.saturating_mul(1_000_000 - satisfaction);
    }
    if total == 0 || reference_milli == 0 {
        return 0;
    }
    let milli = weighted_open * u128::from(reference_milli) / (total * 1_000_000);
    u32::try_from(milli).unwrap_or(reference_milli)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{EvidenceClass, EvidenceId, GateId, GateState, SatisfactionSource};

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
        gate.state = if satisfaction == 1_000_000 {
            GateState::Satisfied
        } else if satisfaction > 0 {
            GateState::InProgress
        } else {
            GateState::Unassessed
        };
        gate
    }

    #[test]
    fn the_nine_yard_ceiling_is_enforced_and_banking_resets_it() {
        let mut position = FieldPosition::default();
        position.record_progress(7).unwrap();
        // A 3-yard gain would cross the ninth yard: refused.
        assert_eq!(
            position.record_progress(3),
            Err(ProgressRefusal::ValidationBoundaryReached)
        );
        // Two more yards fit.
        position.record_progress(2).unwrap();
        assert_eq!(position.unvalidated, 9);
        // The boundary passes at yard 10: the segment banks.
        position.bank(10);
        assert_eq!(position.banked, 10);
        assert_eq!(position.unvalidated, 0);
        assert_eq!(position.observed(), 10);
    }

    #[test]
    fn the_boundary_requirement_is_all_of() {
        let requirement = BoundaryRequirement::new(vec![
            ValidationEvidence::Test,
            ValidationEvidence::Benchmark,
        ])
        .unwrap();
        assert!(
            requirement.satisfied_by(&[ValidationEvidence::Test, ValidationEvidence::Benchmark])
        );
        assert!(!requirement.satisfied_by(&[ValidationEvidence::Test]));
        assert!(BoundaryRequirement::new(Vec::new()).is_err());
    }

    #[test]
    fn adjustments_validate_provenance() {
        let complete = FieldAdjustment {
            adjustment_id: "adj-1".into(),
            direction: AdjustmentDirection::Harder,
            magnitude_yards: 6,
            cause: "compatibility_constraint_discovered".into(),
            evidence_ref: Some(playbook_core::EvidenceId::new("ev-1").unwrap()),
            foreseeability: Foreseeability::Discovery,
            affected_gates: vec![GateId::new("build").unwrap()],
            at_ms: 0,
        };
        complete.validate().unwrap();
        assert!(!complete.is_kicker_estimation_error());
        // No evidence: an unverifiable penalty is refused.
        let unattributed = FieldAdjustment {
            evidence_ref: None,
            ..complete.clone()
        };
        assert!(unattributed.validate().is_err());
        // No affected gates: not a geometry change.
        let unplaced = FieldAdjustment {
            affected_gates: Vec::new(),
            ..complete
        };
        assert!(unplaced.validate().is_err());
        // Estimation error belongs to the kicker.
        let estimation = FieldAdjustment {
            foreseeability: Foreseeability::EstimationError,
            ..unattributed
        };
        assert!(estimation.is_kicker_estimation_error());
        let _ = unplaced.evidence_ref;
        let _ = EvidenceId::new("ev-2").unwrap();
    }

    #[test]
    fn the_gate_count_envelope_holds() {
        assert!(validate_gate_count(1).is_ok());
        assert!(validate_gate_count(99).is_ok());
        assert!(validate_gate_count(100).is_err());
        assert!(validate_gate_count(0).is_err());
    }

    #[test]
    fn the_consolidation_checkpoint_requires_all_six_answers() {
        let mut checkpoint = ConsolidationCheckpoint {
            answers: std::collections::BTreeMap::new(),
        };
        assert!(!checkpoint.complete());
        for question in ConsolidationCheckpoint::QUESTIONS {
            checkpoint.answers.insert(
                question.to_string(),
                EvidenceId::new(format!("ev-{question}")).unwrap(),
            );
        }
        assert!(checkpoint.complete());
    }

    #[test]
    fn kick_utility_defines_range_by_dominance() {
        let kick_dominates = KickUtility {
            kick_utility: 40,
            drive_utility: 25,
        };
        assert!(kick_dominates.in_range());
        let drive_dominates = KickUtility {
            kick_utility: 25,
            drive_utility: 40,
        };
        assert!(!drive_dominates.in_range());
        let equal = KickUtility {
            kick_utility: 40,
            drive_utility: 40,
        };
        // Equality is not dominance: continued progression is the default.
        assert!(!equal.in_range());
    }

    #[test]
    fn field_length_scales_with_evidence_backed_distance() {
        // Fully unassessed gates span the whole reference field.
        let gates = vec![gate("a", 500_000, 0), gate("b", 500_000, 0)];
        assert_eq!(field_length_yards(&gates, 10_000), 10_000);
        // Halfway satisfied drives halve the field.
        let gates = vec![gate("a", 1_000_000, 500_000)];
        assert_eq!(field_length_yards(&gates, 10_000), 5_000);
        // No gates: no field.
        assert_eq!(field_length_yards(&[], 10_000), 0);
    }
}
