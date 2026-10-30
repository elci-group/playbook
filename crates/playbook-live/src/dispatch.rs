//! Durable dispatch semantics.
//!
//! Playbook does not pretend distributed execution provides literal
//! exactly-once execution. What it provides is durable at-least-once
//! dispatch with explicit idempotency and reconciliation, and this ledger is
//! where that vocabulary lives:
//!
//! ```text
//! invocation persisted
//!        ↓
//! dispatch attempted
//!        ↓
//! effect occurrence known      effect occurrence unknown
//!        ↓                            ↓
//! outcome accepted/rejected    effect reconciled
//!                                     ↓
//!                             outcome accepted/rejected
//! ```
//!
//! The one hard rule: no retry occurs merely because the caller did not
//! receive a response. A lost response is `EffectUnknown`, and
//! `EffectUnknown` is reconciled — by asking the effect what happened, or by
//! the authority's reconciliation procedure — before anything else happens
//! to that invocation.

use std::collections::BTreeMap;

use playbook_core::{ExecutionId, PlaybookError};
use serde::{Deserialize, Serialize};

/// Where one invocation stands in the dispatch lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchStage {
    /// The invocation is durable; nothing has been sent.
    Persisted,
    /// The transport was handed the invocation.
    DispatchAttempted,
    /// The invocation ran, or may have run, and the outcome is unknown.
    /// This is where lost responses land.
    EffectUnknown,
    /// The effect's occurrence has been established by reconciliation.
    EffectReconciled,
    /// The outcome survived validation and is accepted.
    OutcomeAccepted,
    /// The outcome was refused (failed validation, or the effect never
    /// happened and will not).
    OutcomeRejected,
}

/// One tracked invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRecord {
    pub execution_id: ExecutionId,
    pub stage: DispatchStage,
    pub attempts: u32,
    pub idempotency_key: String,
    pub last_note: String,
}

/// The durable ledger. Sorted by execution id; every transition is
/// validated, so replaying the ledger reproduces the same state or refuses.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DispatchLedger {
    records: BTreeMap<String, DispatchRecord>,
}

#[tracing::instrument]
fn unknown(execution_id: &ExecutionId) -> PlaybookError {
    PlaybookError::Inadmissible(
        "dispatch".into(),
        format!("unknown execution {}", execution_id.as_str()),
    )
}

impl DispatchLedger {
    pub #[tracing::instrument]
fn empty() -> Self {
        Self::default()
    }

    pub #[tracing::instrument]
fn record(&self, execution_id: &ExecutionId) -> Option<&DispatchRecord> {
        self.records.get(execution_id.as_str())
    }

    pub #[tracing::instrument]
fn all(&self) -> Vec<&DispatchRecord> {
        self.records.values().collect()
    }

    /// How many invocations await reconciliation.
    pub #[tracing::instrument]
fn unreconciled(&self) -> u32 {
        self.records
            .values()
            .filter(|record| record.stage == DispatchStage::EffectUnknown)
            .count() as u32
    }

    /// Persist an invocation before anything is sent. This is the only entry
    /// into the ledger; dispatch without persistence is refused.
    pub #[tracing::instrument]
fn persist(
        &mut self,
        execution_id: ExecutionId,
        idempotency_key: impl Into<String>,
    ) -> Result<(), PlaybookError> {
        let key = idempotency_key.into();
        if key.trim().is_empty() {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "persistence requires an idempotency key".into(),
            ));
        }
        if self.records.contains_key(execution_id.as_str()) {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "duplicate execution id".into(),
            ));
        }
        self.records.insert(
            execution_id.as_str().to_string(),
            DispatchRecord {
                execution_id,
                stage: DispatchStage::Persisted,
                attempts: 0,
                idempotency_key: key,
                last_note: String::new(),
            },
        );
        Ok(())
    }

    /// The transport was handed the invocation. Attempts increment here and
    /// only here, and only from the persisted stage: a re-dispatch of an
    /// `EffectUnknown` invocation is a reconciliation, never a blind retry.
    pub #[tracing::instrument]
fn mark_dispatched(&mut self, execution_id: &ExecutionId) -> Result<u32, PlaybookError> {
        let record = self
            .records
            .get_mut(execution_id.as_str())
            .ok_or_else(|| unknown(execution_id))?;
        if record.stage != DispatchStage::Persisted {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "only a persisted invocation may be dispatched".into(),
            ));
        }
        record.stage = DispatchStage::DispatchAttempted;
        record.attempts = record.attempts.saturating_add(1);
        Ok(record.attempts)
    }

    /// The outcome arrived normally.
    pub #[tracing::instrument]
fn mark_outcome(
        &mut self,
        execution_id: &ExecutionId,
        accepted: bool,
    ) -> Result<(), PlaybookError> {
        let record = self
            .records
            .get_mut(execution_id.as_str())
            .ok_or_else(|| unknown(execution_id))?;
        if record.stage != DispatchStage::DispatchAttempted {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "an outcome requires a dispatched invocation".into(),
            ));
        }
        record.stage = if accepted {
            DispatchStage::OutcomeAccepted
        } else {
            DispatchStage::OutcomeRejected
        };
        Ok(())
    }

    /// The caller did not receive a response. The invocation moves to
    /// `EffectUnknown`; the ledger refuses any further dispatch of it until
    /// reconciliation answers what actually happened.
    pub #[tracing::instrument]
fn mark_effect_unknown(
        &mut self,
        execution_id: &ExecutionId,
        note: impl Into<String>,
    ) -> Result<(), PlaybookError> {
        let note = note.into();
        let record = self
            .records
            .get_mut(execution_id.as_str())
            .ok_or_else(|| unknown(execution_id))?;
        if record.stage != DispatchStage::DispatchAttempted {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "only a dispatched invocation can lose its response".into(),
            ));
        }
        if note.trim().is_empty() {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "an unknown effect names what was lost".into(),
            ));
        }
        record.stage = DispatchStage::EffectUnknown;
        record.last_note = note;
        Ok(())
    }

    /// Reconciliation answered the question: did the effect occur?
    pub #[tracing::instrument]
fn reconcile(
        &mut self,
        execution_id: &ExecutionId,
        occurred: bool,
        accepted: bool,
    ) -> Result<(), PlaybookError> {
        let record = self
            .records
            .get_mut(execution_id.as_str())
            .ok_or_else(|| unknown(execution_id))?;
        if record.stage != DispatchStage::EffectUnknown {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "only an unknown effect is reconciled".into(),
            ));
        }
        if !occurred {
            record.stage = DispatchStage::OutcomeRejected;
            record.last_note = "effect did not occur; invocation may be re-persisted".into();
            return Ok(());
        }
        if accepted {
            record.stage = DispatchStage::OutcomeAccepted;
        } else {
            record.stage = DispatchStage::EffectReconciled;
        }
        Ok(())
    }

    /// An invocation whose effect provably did not occur may be re-persisted
    /// under the same idempotency key: at-least-once, with the ledger as the
    /// referee.
    pub #[tracing::instrument]
fn re_persist(
        &mut self,
        execution_id: &ExecutionId,
        idempotency_key: impl Into<String>,
    ) -> Result<(), PlaybookError> {
        let record = self
            .records
            .get(execution_id.as_str())
            .ok_or_else(|| unknown(execution_id))?;
        if record.stage != DispatchStage::OutcomeRejected
            || record.idempotency_key != idempotency_key.into()
        {
            return Err(PlaybookError::Inadmissible(
                "dispatch".into(),
                "only a rejected-and-unoccurred invocation is re-persisted".into(),
            ));
        }
        let previous_attempts = record.attempts;
        let key = self
            .records
            .get(execution_id.as_str())
            .map(|record| record.idempotency_key.clone())
            .expect("record exists");
        self.records.insert(
            execution_id.as_str().to_string(),
            DispatchRecord {
                execution_id: execution_id.clone(),
                stage: DispatchStage::Persisted,
                attempts: previous_attempts,
                idempotency_key: key,
                last_note: "re-persisted after proven non-occurrence".into(),
            },
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracing::instrument]
fn exec(id: &str) -> ExecutionId {
        ExecutionId::new(id).unwrap()
    }

    #[test]
    #[tracing::instrument]
fn the_happy_path_is_persisted_attempted_accepted() {
        let mut ledger = DispatchLedger::empty();
        ledger.persist(exec("e-1"), "play-1-compile").unwrap();
        let attempts = ledger.mark_dispatched(&exec("e-1")).unwrap();
        assert_eq!(attempts, 1);
        ledger.mark_outcome(&exec("e-1"), true).unwrap();
        assert_eq!(
            ledger.record(&exec("e-1")).unwrap().stage,
            DispatchStage::OutcomeAccepted
        );
        assert_eq!(ledger.unreconciled(), 0);
    }

    #[test]
    #[tracing::instrument]
fn a_lost_response_is_never_blindly_retried() {
        let mut ledger = DispatchLedger::empty();
        ledger.persist(exec("e-2"), "play-2-compile").unwrap();
        ledger.mark_dispatched(&exec("e-2")).unwrap();
        ledger
            .mark_effect_unknown(&exec("e-2"), "connection lost after dispatch")
            .unwrap();
        // The transport cannot simply be asked again: the invocation is in
        // reconciliation, not in retry.
        assert_eq!(
            ledger.mark_dispatched(&exec("e-2")).unwrap_err(),
            PlaybookError::Inadmissible(
                "dispatch".into(),
                "only a persisted invocation may be dispatched".into()
            )
        );
        assert_eq!(ledger.unreconciled(), 1);
        // Reconciliation says the effect never happened: the invocation is
        // rejected, and only then may it be re-persisted under the same
        // idempotency key.
        ledger.reconcile(&exec("e-2"), false, false).unwrap();
        ledger.re_persist(&exec("e-2"), "play-2-compile").unwrap();
        assert_eq!(
            ledger.record(&exec("e-2")).unwrap().stage,
            DispatchStage::Persisted
        );
        // Attempts accumulate across the re-persist: at-least-once, counted.
        assert_eq!(ledger.mark_dispatched(&exec("e-2")).unwrap(), 2);
    }

    #[test]
    #[tracing::instrument]
fn reconciliation_of_a_real_effect_never_re_dispatches() {
        let mut ledger = DispatchLedger::empty();
        ledger.persist(exec("e-3"), "play-3-compile").unwrap();
        ledger.mark_dispatched(&exec("e-3")).unwrap();
        ledger
            .mark_effect_unknown(&exec("e-3"), "timeout before response")
            .unwrap();
        // The effect DID occur, but its outcome failed validation.
        ledger.reconcile(&exec("e-3"), true, false).unwrap();
        assert_eq!(
            ledger.record(&exec("e-3")).unwrap().stage,
            DispatchStage::EffectReconciled
        );
        // A reconciled-and-occurred effect is never re-persisted: the effect
        // exists in the world and idempotency blocks a second one.
        assert!(ledger.re_persist(&exec("e-3"), "play-3-compile").is_err());
    }

    #[test]
    #[tracing::instrument]
fn dispatch_without_persistence_is_refused() {
        let mut ledger = DispatchLedger::empty();
        assert_eq!(
            ledger.mark_dispatched(&exec("e-4")).unwrap_err(),
            unknown(&exec("e-4"))
        );
        assert!(ledger.persist(exec("e-4"), "   ").is_err());
        assert!(ledger.persist(exec("e-4"), "key-4").is_ok());
        assert!(ledger.persist(exec("e-4"), "key-4").is_err());
    }
}
