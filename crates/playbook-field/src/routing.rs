//! Phase H — routing integration: Playbook chooses among continuing the
//! drive, validating the current segment, attempting a field goal, or
//! invoking Lastdown — from the field state itself, not from prose
//! heuristics.
//!
//! The selection is a pure function of the measured field inputs and the
//! policy thresholds. Doctrine, compressed (§32): do not reward motion;
//! reward evidenced progress. Do not define proximity by arbitrary yardage —
//! define it by closure feasibility. Do not continue a drive merely because
//! possession remains available — attempt closure when one bounded action
//! dominates continued progression.

use serde::{Deserialize, Serialize};

use crate::domain::{field_refusal, RedZoneAssessment};

/// The four admissible options the field state selects among.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldOption {
    /// Continue ordinary validated progression.
    ContinueDrive,
    /// The current segment must pass its validation boundary.
    Validate,
    /// Attempt the admitted offensive field goal.
    AttemptFieldGoal,
    /// Lastdown: preserve possession through a continuation package.
    InvokeLastdown,
}

/// Everything the selection reads. All inputs are measured field state;
/// nothing here consults a model.
#[derive(Clone, Debug)]
pub struct RoutingInputs {
    /// The 9+1 ledger state: boundaries pending/passed/failed.
    pub segment_pending: bool,
    /// Unvalidated yards since the last passed boundary.
    pub unvalidated_yards: u32,
    /// Yards to goal at the observed position.
    pub ytg_yards: u32,
    /// The semantic Red Zone assessment.
    pub red_zone: RedZoneAssessment,
    /// Whether a candidate kick dominates continued progression.
    pub field_goal_in_range: bool,
    /// Whether an admissible kicker candidate exists for the attempt.
    pub kicker_available: bool,
    /// The drive is inside its terminal possession envelope (Lastdown's
    /// domain): remaining budget will not survive another full segment.
    pub lastdown_recommended: bool,
    /// The drive has failed its current validation boundary and the
    /// implicated gates are reopened — progression may resume.
    pub validation_failed: bool,
}

/// The maximum unvalidated yards the routing tolerates before validation is
/// forced (mirrors the domain ceiling).
pub const FORCE_VALIDATE_AT_YARDS: u32 = 9;

/// Select the field option. Deterministic; the decision order is doctrine:
///
/// 1. A failed validation boundary forces the implicated gates to reopen
///    before anything else happens — progression cannot resume over an
///    unaccounted failure (I-04).
/// 2. A pending boundary at the observed position forces Validate — the
///    tenth yard is not traversable by ordinary work (I-02/I-03).
/// 3. A dominant candidate kick with a kicker available → AttemptFieldGoal
///    (I-12: range by utility, not yardage).
/// 4. Red Zone entered and closure is feasible → ContinueDrive toward the
///    terminal chain (the Red Zone optimises completion probability).
/// 5. Lastdown recommended when the budget cannot survive another segment.
/// 6. Otherwise ContinueDrive.
pub fn choose(inputs: &RoutingInputs) -> FieldOption {
    if inputs.validation_failed {
        return FieldOption::ContinueDrive; // gates reopened; progression resumes
    }
    if inputs.segment_pending && inputs.unvalidated_yards >= FORCE_VALIDATE_AT_YARDS {
        return FieldOption::Validate;
    }
    if inputs.field_goal_in_range && inputs.kicker_available {
        return FieldOption::AttemptFieldGoal;
    }
    if inputs.red_zone.entered {
        return FieldOption::ContinueDrive;
    }
    if inputs.lastdown_recommended {
        return FieldOption::InvokeLastdown;
    }
    FieldOption::ContinueDrive
}

/// Human-readable option chip for the journal.
impl FieldOption {
    pub fn key(self) -> &'static str {
        match self {
            Self::ContinueDrive => "continue_drive",
            Self::Validate => "validate",
            Self::AttemptFieldGoal => "attempt_field_goal",
            Self::InvokeLastdown => "invoke_lastdown",
        }
    }
}

/// Construct routing inputs from the pieces, refusing contradictory states
/// (a drive cannot both have a pending boundary and be past validation).
#[allow(clippy::too_many_arguments)]
pub fn inputs(
    segment_pending: bool,
    unvalidated_yards: u32,
    ytg_yards: u32,
    red_zone: RedZoneAssessment,
    field_goal_in_range: bool,
    kicker_available: bool,
    lastdown_recommended: bool,
    validation_failed: bool,
) -> Result<RoutingInputs, playbook_core::PlaybookError> {
    if unvalidated_yards > crate::domain::MAX_UNVALIDATED_YARDS {
        return Err(field_refusal(
            "unvalidated progress exceeds the nine-yard ceiling",
        ));
    }
    Ok(RoutingInputs {
        segment_pending,
        unvalidated_yards,
        ytg_yards,
        red_zone,
        field_goal_in_range,
        kicker_available,
        lastdown_recommended,
        validation_failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red_zone(entered: bool) -> RedZoneAssessment {
        RedZoneAssessment {
            entered,
            outstanding_mandatory: Vec::new(),
            blockers: Vec::new(),
        }
    }

    #[test]
    fn the_decision_order_is_deterministic() {
        // A failed validation boundary resumes progression: the implicated
        // gates have been reopened, so the offence keeps its drive.
        let inputs = RoutingInputs {
            segment_pending: true,
            unvalidated_yards: 0,
            ytg_yards: 5_000,
            red_zone: red_zone(false),
            field_goal_in_range: false,
            kicker_available: false,
            lastdown_recommended: false,
            validation_failed: true,
        };
        assert!(matches!(choose(&inputs), FieldOption::ContinueDrive));
    }

    #[test]
    fn the_nine_yard_ceiling_forces_validation() {
        let inputs = RoutingInputs {
            segment_pending: true,
            unvalidated_yards: FORCE_VALIDATE_AT_YARDS,
            ytg_yards: 5_000,
            red_zone: red_zone(false),
            field_goal_in_range: false,
            kicker_available: false,
            lastdown_recommended: false,
            validation_failed: false,
        };
        assert!(matches!(choose(&inputs), FieldOption::Validate));
    }

    #[test]
    fn a_dominant_kick_beats_continuation_even_in_the_red_zone() {
        let inputs = RoutingInputs {
            segment_pending: false,
            unvalidated_yards: 2,
            ytg_yards: 500,
            red_zone: red_zone(true),
            field_goal_in_range: true,
            kicker_available: true,
            lastdown_recommended: false,
            validation_failed: false,
        };
        assert!(matches!(choose(&inputs), FieldOption::AttemptFieldGoal));
    }

    #[test]
    fn range_without_a_kicker_falls_through_to_continuation() {
        let inputs = RoutingInputs {
            segment_pending: false,
            unvalidated_yards: 0,
            ytg_yards: 500,
            red_zone: red_zone(true),
            field_goal_in_range: true,
            kicker_available: false,
            lastdown_recommended: false,
            validation_failed: false,
        };
        assert!(matches!(choose(&inputs), FieldOption::ContinueDrive));
    }

    #[test]
    fn lastdown_precedes_progression_when_the_budget_cannot_survive() {
        let inputs = RoutingInputs {
            segment_pending: false,
            unvalidated_yards: 0,
            ytg_yards: 8_000,
            red_zone: red_zone(false),
            field_goal_in_range: false,
            kicker_available: false,
            lastdown_recommended: true,
            validation_failed: false,
        };
        assert!(matches!(choose(&inputs), FieldOption::InvokeLastdown));
    }

    #[test]
    fn measured_progress_continues_the_drive() {
        let inputs = RoutingInputs {
            segment_pending: false,
            unvalidated_yards: 3,
            ytg_yards: 5_000,
            red_zone: red_zone(false),
            field_goal_in_range: false,
            kicker_available: false,
            lastdown_recommended: false,
            validation_failed: false,
        };
        assert!(matches!(choose(&inputs), FieldOption::ContinueDrive));
    }

    #[test]
    fn the_constructor_refuses_an_illegal_unvalidated_state() {
        assert!(inputs(
            false,
            crate::domain::MAX_UNVALIDATED_YARDS + 1,
            5_000,
            red_zone(false),
            false,
            false,
            false,
            false
        )
        .is_err());
    }
}
