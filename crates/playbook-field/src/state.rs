//! The fold-side field state: what `playbook-state` maintains from the
//! field-geometry journal events.
//!
//! The record is a pure projection of the journal: replay reproduces the
//! same banked frontier, penalties, range assessments, and score state
//! (I-17). The authoritative gate graph stays where it has always been —
//! in the gate table; this record carries the geometry the field protocol
//! reasons over.

use serde::{Deserialize, Serialize};

use crate::domain::{FieldAdjustment, FieldPosition, KickOutcome};
use crate::geometry::SegmentLedger;

/// The fold-maintained field state for one drive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldGeometryState {
    /// Banked and unvalidated progress.
    pub position: FieldPosition,
    /// The segment ledger: boundaries at every tenth yard.
    pub segments: SegmentLedger,
    /// The Opposition Kicker's declared field length.
    pub field_yards: u32,
    /// Red Zone state.
    pub red_zone_entered: bool,
    /// Field-goal-range state.
    pub field_goal_range_entered: bool,
    /// Causal field adjustments, in declaration order.
    pub adjustments: Vec<FieldAdjustment>,
    /// The kick outcome, when one attempt is on the record.
    pub kick_outcome: Option<KickOutcome>,
    /// A touchdown was scored through ordinary validated progression.
    pub touchdown: bool,
    pub kick_accuracy_ppm: u32,
}

impl FieldGeometryState {
    pub fn empty() -> Self {
        Self {
            position: FieldPosition::default(),
            segments: SegmentLedger::construct(
                0,
                crate::domain::BoundaryRequirement::new(vec![
                    crate::domain::ValidationEvidence::Test,
                ])
                .expect("test-class boundary"),
            ),
            field_yards: 0,
            red_zone_entered: false,
            field_goal_range_entered: false,
            adjustments: Vec::new(),
            kick_outcome: None,
            touchdown: false,
            kick_accuracy_ppm: 1_000_000,
        }
    }

    /// Record productive progress. Refuses at the ninth unvalidated yard
    /// (I-02).
    pub fn record_progress(&mut self, yards: u32) -> Result<(), playbook_core::PlaybookError> {
        crate::geometry::record_progress(&mut self.position, yards).map_err(|_| {
            playbook_core::PlaybookError::Inadmissible(
                "field".into(),
                "validation_boundary_reached".into(),
            )
        })
    }

    /// Bank at a passed boundary yard.
    pub fn bank(&mut self, yard: u32) {
        self.segments.pass(yard).ok();
        self.position.bank(yard);
    }

    /// Declare a field adjustment: validated, logged, and applied to the
    /// observed geometry (I-06/I-07: harder adjustments extend the field,
    /// easier adjustments contract it — never silently).
    pub fn declare_adjustment(
        &mut self,
        adjustment: FieldAdjustment,
    ) -> Result<(), playbook_core::PlaybookError> {
        adjustment.validate()?;
        let yard_delta = match adjustment.direction {
            crate::domain::AdjustmentDirection::Harder => i64::from(adjustment.magnitude_yards),
            crate::domain::AdjustmentDirection::Easier => -i64::from(adjustment.magnitude_yards),
        };
        self.field_yards = u32::try_from(i64::from(self.field_yards).saturating_add(yard_delta))
            .unwrap_or(self.field_yards);
        self.adjustments.push(adjustment);
        Ok(())
    }

    /// Enter the Red Zone.
    pub fn enter_red_zone(&mut self) {
        self.red_zone_entered = true;
    }

    /// Exit the Red Zone.
    pub fn exit_red_zone(&mut self) {
        self.red_zone_entered = false;
    }

    /// Enter field-goal range.
    pub fn enter_field_goal_range(&mut self) {
        self.field_goal_range_entered = true;
    }

    /// Exit field-goal range.
    pub fn exit_field_goal_range(&mut self) {
        self.field_goal_range_entered = false;
    }

    /// Record a kick outcome.
    pub fn record_kick(&mut self, outcome: KickOutcome, accuracy_ppm: u32) {
        self.kick_outcome = Some(outcome);
        self.kick_accuracy_ppm = accuracy_ppm;
    }

    /// Record a touchdown through ordinary validated progression.
    pub fn score_touchdown(&mut self) {
        self.touchdown = true;
    }
}
