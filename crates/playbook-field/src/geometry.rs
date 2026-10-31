//! The 9+1 segment geometry: banked and unvalidated progress over the
//! tenth-yard validation rhythm.
//!
//! Progress exists in two materially different states. Banked progress has
//! survived a validation boundary — it is recoverable from the journal,
//! attributable to its evidence, and survives ordinary personnel
//! substitution; it remains reopenable where later contradictory evidence
//! proves the underlying work invalid (I-05). Unvalidated progress is
//! provisional since the last boundary: it may represent legitimate work,
//! but it has not been consolidated and can never exceed nine yards (I-02).
//!
//! The [`SegmentLedger`] carries the tenth-yard boundaries in yard order.
//! The fold appends pass/fail transitions; the geometry enforces that
//! boundaries resolve in order, that a failed boundary does not bank its
//! segment (I-04), and that the checkpoint is complete before a pass.

use serde::{Deserialize, Serialize};

use crate::domain::{
    field_refusal, BoundaryRequirement, BoundaryState, ConsolidationCheckpoint, FieldPosition,
    ProgressRefusal, ValidationEvidence, SEGMENT_YARDS,
};

/// One tenth-yard validation segment in the ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentRecord {
    /// 1-based segment ordinal.
    pub segment_number: u16,
    /// The yard the segment starts from (exclusive).
    pub start_yard: u32,
    /// The yard of the validation boundary (inclusive).
    pub end_yard: u32,
    pub requirement: BoundaryRequirement,
    pub state: BoundaryState,
}

/// The segment ledger: every validation boundary in the drive, in yard
/// order. Constructed once from the field length; mutated only by
/// fold-validated pass/fail transitions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentLedger {
    records: Vec<SegmentRecord>,
}

fn unknown_yard(yard: u32) -> playbook_core::PlaybookError {
    field_refusal(&format!("no validation boundary at yard {yard}"))
}

impl SegmentLedger {
    /// Construct the full ledger for a field of `field_yards` yards:
    /// boundaries at 10, 20, 30… every [`SEGMENT_YARDS`] yards up to and
    /// including the largest multiple at or below `field_yards`. A 0-yard
    /// field produces an empty ledger.
    pub fn construct(field_yards: u32, requirement: BoundaryRequirement) -> Self {
        let mut records = Vec::new();
        let mut yard = SEGMENT_YARDS;
        let mut segment_number = 1u16;
        while yard <= field_yards {
            records.push(SegmentRecord {
                segment_number,
                start_yard: yard - SEGMENT_YARDS,
                end_yard: yard,
                requirement: requirement.clone(),
                state: BoundaryState::Pending,
            });
            yard += SEGMENT_YARDS;
            segment_number += 1;
        }
        Self { records }
    }

    pub fn records(&self) -> &[SegmentRecord] {
        &self.records
    }

    /// The next pending boundary at or beyond the observed position.
    pub fn next_pending(&self, observed_yard: u32) -> Option<&SegmentRecord> {
        self.records.iter().find(|record| {
            record.state == BoundaryState::Pending && record.end_yard >= observed_yard
        })
    }

    /// Pass the boundary at `yard`: bank the segment. Only the next pending
    /// boundary may pass — skipping is refused (I-03: the tenth yard must be
    /// validated, in order).
    pub fn pass(&mut self, yard: u32) -> Result<(), playbook_core::PlaybookError> {
        let index = self
            .records
            .iter()
            .position(|record| record.end_yard == yard)
            .ok_or_else(|| unknown_yard(yard))?;
        if self.records[index].state != BoundaryState::Pending {
            return Err(field_refusal("boundary already resolved"));
        }
        if self
            .records
            .iter()
            .any(|record| record.end_yard < yard && record.state == BoundaryState::Pending)
        {
            return Err(field_refusal("boundaries must be validated in order"));
        }
        self.records[index].state = BoundaryState::Passed;
        Ok(())
    }

    /// Fail the boundary at `yard`: the segment does not bank (I-04) and the
    /// reason names the causally implicated upstream gate set for the fold
    /// to reopen.
    pub fn fail(&mut self, yard: u32, reason: &str) -> Result<(), playbook_core::PlaybookError> {
        let index = self
            .records
            .iter()
            .position(|record| record.end_yard == yard)
            .ok_or_else(|| unknown_yard(yard))?;
        if self.records[index].state == BoundaryState::Passed {
            return Err(field_refusal("a passed boundary cannot fail"));
        }
        if reason.trim().is_empty() {
            return Err(field_refusal("a failed boundary names its reason"));
        }
        self.records[index].state = BoundaryState::Failed;
        Ok(())
    }

    /// Banked yards: the highest passed boundary (or 0).
    pub fn banked_yards(&self) -> u32 {
        self.records
            .iter()
            .rev()
            .find(|record| record.state == BoundaryState::Passed)
            .map(|record| record.end_yard)
            .unwrap_or(0)
    }

    /// Unvalidated yards: observed position minus the banked frontier.
    pub fn unvalidated_yards(&self, observed_yard: u32) -> u32 {
        observed_yard.saturating_sub(self.banked_yards())
    }
}

/// Record productive progress onto the position, enforcing the 9+1 rule at
/// the geometry level: ordinary progression refuses to cross the ninth
/// unvalidated yard (I-02).
pub fn record_progress(position: &mut FieldPosition, yards: u32) -> Result<(), ProgressRefusal> {
    position.record_progress(yards)
}

/// Pass the validation boundary for the current segment using the
/// consolidation checkpoint: the checkpoint must be complete (all six
/// questions answered — §6) and the presented evidence must satisfy the
/// boundary requirement (I-03). On success the segment banks; on failure
/// the geometry records the failed state and the fold reopens the causally
/// implicated gates.
pub fn pass_validation(
    ledger: &mut SegmentLedger,
    position: &mut FieldPosition,
    checkpoint: &ConsolidationCheckpoint,
    boundary_requirement: &crate::domain::BoundaryRequirement,
    presented_evidence: &[ValidationEvidence],
    yard: u32,
) -> Result<(), playbook_core::PlaybookError> {
    if !checkpoint.complete() {
        return Err(field_refusal("checkpoint_incomplete"));
    }
    if !boundary_requirement.satisfied_by(presented_evidence) {
        return Err(field_refusal("evidence_classes_unmet"));
    }
    ledger.pass(yard)?;
    position.bank(yard);
    Ok(())
}

/// Record a failed validation: the boundary is marked failed and does not
/// bank. The reason names the causally implicated upstream gate set.
pub fn fail_validation(
    ledger: &mut SegmentLedger,
    yard: u32,
    reason: &str,
) -> Result<(), playbook_core::PlaybookError> {
    ledger.fail(yard, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::EvidenceId;

    fn test_boundary() -> BoundaryRequirement {
        BoundaryRequirement::new(vec![ValidationEvidence::Test]).unwrap()
    }

    fn complete_checkpoint() -> ConsolidationCheckpoint {
        ConsolidationCheckpoint {
            answers: ConsolidationCheckpoint::QUESTIONS
                .iter()
                .map(|question| {
                    (
                        question.to_string(),
                        EvidenceId::new(format!("ev-{question}")).unwrap(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn the_ledger_constructs_tenth_yard_boundaries() {
        let ledger = SegmentLedger::construct(35, test_boundary());
        // Boundaries sit on multiples of ten at or below the field length.
        assert_eq!(ledger.records().len(), 3);
        assert_eq!(ledger.records()[0].end_yard, 10);
        assert_eq!(ledger.records()[1].end_yard, 20);
        assert_eq!(ledger.records()[2].end_yard, 30);
        let empty = SegmentLedger::construct(0, test_boundary());
        assert!(empty.records().is_empty());
    }

    #[test]
    fn nine_yards_progress_then_the_boundary_refuses_the_tenth() {
        let mut ledger = SegmentLedger::construct(10, test_boundary());
        let mut position = FieldPosition::default();
        record_progress(&mut position, 9).unwrap();
        assert_eq!(
            record_progress(&mut position, 1),
            Err(ProgressRefusal::ValidationBoundaryReached)
        );
        // The boundary itself passes with the checkpoint complete.
        pass_validation(
            &mut ledger,
            &mut position,
            &complete_checkpoint(),
            &test_boundary(),
            &[ValidationEvidence::Test],
            10,
        )
        .unwrap();
        assert_eq!(position.banked, 10);
        assert_eq!(position.unvalidated, 0);
        assert_eq!(ledger.banked_yards(), 10);
    }

    #[test]
    fn a_failed_boundary_does_not_bank_and_names_its_reason() {
        let mut ledger = SegmentLedger::construct(10, test_boundary());
        ledger.fail(10, "composition_incoherent").unwrap();
        assert!(ledger.pass(10).is_err());
        assert_eq!(ledger.banked_yards(), 0);
        assert!(ledger.fail(10, "  ").is_err());
        assert!(ledger.fail(99, "no such boundary").is_err());
    }

    #[test]
    fn boundaries_resolve_in_order_and_checkpoint_evidence_binds() {
        let mut ledger = SegmentLedger::construct(20, test_boundary());
        // Yard 20 cannot pass while yard 10 is pending.
        assert!(ledger.pass(20).is_err());
        // The checkpoint is complete but the evidence does not meet the
        // boundary requirement (human input required, test presented).
        let human_boundary = BoundaryRequirement::new(vec![
            ValidationEvidence::Test,
            ValidationEvidence::HumanInput,
        ])
        .unwrap();
        let mut human_ledger = SegmentLedger::construct(10, human_boundary.clone());
        let mut position = FieldPosition::default();
        assert!(pass_validation(
            &mut human_ledger,
            &mut position,
            &complete_checkpoint(),
            &human_boundary,
            &[ValidationEvidence::Test],
            10,
        )
        .is_err());
    }

    #[test]
    fn banked_and_unvalidated_decompose_the_observed_position() {
        let mut ledger = SegmentLedger::construct(30, test_boundary());
        let mut position = FieldPosition::default();
        record_progress(&mut position, 9).unwrap();
        pass_validation(
            &mut ledger,
            &mut position,
            &complete_checkpoint(),
            &test_boundary(),
            &[ValidationEvidence::Test],
            10,
        )
        .unwrap();
        record_progress(&mut position, 4).unwrap();
        assert_eq!(ledger.banked_yards(), 10);
        assert_eq!(ledger.unvalidated_yards(14), 4);
        // Passing the next boundary banks through its yard.
        ledger.pass(20).unwrap();
        position.bank(20);
        assert_eq!(ledger.banked_yards(), 20);
        assert_eq!(ledger.unvalidated_yards(20), 0);
    }

    #[test]
    fn pass_validation_is_deterministic() {
        let mut first = SegmentLedger::construct(10, test_boundary());
        let mut second = SegmentLedger::construct(10, test_boundary());
        let mut first_position = FieldPosition::default();
        let mut second_position = FieldPosition::default();
        pass_validation(
            &mut first,
            &mut first_position,
            &complete_checkpoint(),
            &test_boundary(),
            &[ValidationEvidence::Test],
            10,
        )
        .unwrap();
        pass_validation(
            &mut second,
            &mut second_position,
            &complete_checkpoint(),
            &test_boundary(),
            &[ValidationEvidence::Test],
            10,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first_position, second_position);
    }
}
