//! Cancellation semantics for live concurrent execution.
//!
//! Live cancellation is not a boolean. Every adapter reports one of six
//! states, and the conservative rule binds all of them: an unknown
//! cancellation state is an unknown effect. It is reconciled, never
//! optimistically booked — which matters most for Blitz, where the entire
//! point of cancellation is to stop paying for redundant work, and booking
//! avoided spend that never materialised would be fiction.

use serde::{Deserialize, Serialize};

use playbook_core::ExecutionId;

/// What a live adapter reports when cancellation was requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationState {
    /// The invocation never started.
    CancelledBeforeExecution,
    /// The invocation was interrupted mid-flight; partial effects may exist.
    CancelledDuringExecution,
    /// The request was accepted but no outcome has been observed yet.
    CancellationRequested,
    /// The capability cannot be cancelled; it will run to completion.
    NonCancellable,
    /// The invocation completed before the cancellation arrived, so the
    /// outcome is real and must be processed.
    CompletedBeforeCancel,
    /// The adapter cannot say. Conservative treatment applies.
    Unknown,
}

impl CancellationState {
    /// The effect of the invocation is still unknown after this
    /// cancellation report. Only these states demand reconciliation before
    /// the ledger may close the invocation.
    pub fn leaves_effect_unknown(self) -> bool {
        matches!(
            self,
            Self::CancelledDuringExecution
                | Self::CancellationRequested
                | Self::Unknown
        )
    }

    /// Whether avoided-spend may be booked for this state. Where partial
    /// effects may exist, or the state is unknown, the spend is reserved
    /// until reconciliation answers — booking avoided cost on fiction is
    /// how budgets learn to lie.
    pub fn may_book_avoided_spend(self) -> bool {
        matches!(
            self,
            Self::CancelledBeforeExecution | Self::CompletedBeforeCancel | Self::NonCancellable
        )
    }

    /// Whether a real outcome may still arrive later. A late result after
    /// cancellation is a first-class failure mode: the adapter must keep the
    /// invocation reconcilable when this is true.
    pub fn late_result_possible(self) -> bool {
        matches!(
            self,
            Self::CancelledDuringExecution
                | Self::CancellationRequested
                | Self::Unknown
                | Self::NonCancellable
        )
    }
}

/// One adapter's cancellation answer for one invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationReport {
    pub execution_id: ExecutionId,
    pub state: CancellationState,
    /// What the adapter observed, for the reconciliation trail.
    pub note: String,
}

impl CancellationReport {
    pub fn new(
        execution_id: ExecutionId,
        state: CancellationState,
        note: impl Into<String>,
    ) -> Self {
        Self {
            execution_id,
            state,
            note: note.into(),
        }
    }

    /// The dispatch-stage this report demands: conservative states keep the
    /// invocation in `EffectUnknown` until reconciliation.
    pub fn effect_unknown(&self) -> bool {
        self.state.leaves_effect_unknown()
    }
}

/// Count how many reports may book avoided spend, for Blitz economics.
pub fn cancellation_counts_as_effect_unknown(reports: &[CancellationReport]) -> u32 {
    reports
        .iter()
        .filter(|report| report.effect_unknown())
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(state: CancellationState) -> CancellationReport {
        CancellationReport::new(
            ExecutionId::new("e-cancel").unwrap(),
            state,
            "requested after winner selection",
        )
    }

    #[test]
    fn unknown_states_are_conservative() {
        // Clean states close the book.
        for clean in [
            CancellationState::CancelledBeforeExecution,
            CancellationState::CompletedBeforeCancel,
            CancellationState::NonCancellable,
        ] {
            assert!(!report(clean).effect_unknown(), "{clean:?}");
            assert!(report(clean).may_book_avoided_spend(), "{clean:?}");
        }
        // Dirty states keep the invocation reconcilable and the spend
        // reserved.
        for dirty in [
            CancellationState::CancelledDuringExecution,
            CancellationState::CancellationRequested,
            CancellationState::Unknown,
        ] {
            assert!(report(dirty).effect_unknown(), "{dirty:?}");
            assert!(!report(dirty).may_book_avoided_spend(), "{dirty:?}");
            assert!(report(dirty).state.late_result_possible(), "{dirty:?}");
        }
        // A completed invocation cannot have a late result; a non-cancellable
        // one will deliver one by definition.
        assert!(!report(CancellationState::CompletedBeforeCancel)
            .state
            .late_result_possible());
        assert_eq!(
            cancellation_counts_as_effect_unknown(&[
                report(CancellationState::Unknown),
                report(CancellationState::CancelledBeforeExecution),
                report(CancellationState::CancellationRequested),
            ]),
            2
        );
    }

    #[test]
    fn completed_before_cancel_outweighs_the_cancellation() {
        // The outcome is real: the cancellation lost the race, and the race
        // winner must process it, not discard it.
        let finished = report(CancellationState::CompletedBeforeCancel);
        assert!(!finished.effect_unknown());
        assert!(finished.may_book_avoided_spend());
    }
}
