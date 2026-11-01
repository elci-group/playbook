//! The resolution event model (§39): immutable, append-only records along
//! the recovery timeline.

use serde::{Deserialize, Serialize};

/// The recovery lifecycle (§35, §39).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionStage {
    RefusalObserved,
    RefusalClassified,
    ResolutionProposed,
    ResolutionAuthorised,
    ResolutionApplied,
    ReplayRequested,
    ReplayStarted,
    ReplayCompleted,
    ResolutionSucceeded,
    ResolutionFailed,
    EscalationRequested,
    HumanInterventionObserved,
}

impl ResolutionStage {
    pub fn key(self) -> &'static str {
        match self {
            Self::RefusalObserved => "refusal_observed",
            Self::RefusalClassified => "refusal_classified",
            Self::ResolutionProposed => "resolution_proposed",
            Self::ResolutionAuthorised => "resolution_authorised",
            Self::ResolutionApplied => "resolution_applied",
            Self::ReplayRequested => "replay_requested",
            Self::ReplayStarted => "replay_started",
            Self::ReplayCompleted => "replay_completed",
            Self::ResolutionSucceeded => "resolution_succeeded",
            Self::ResolutionFailed => "resolution_failed",
            Self::EscalationRequested => "escalation_requested",
            Self::HumanInterventionObserved => "human_intervention_observed",
        }
    }

    /// Whether this stage closes a recovery, one way or the other.
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::ResolutionSucceeded | Self::ResolutionFailed | Self::EscalationRequested
        )
    }
}

/// One immutable event on the recovery timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionEvent {
    pub stage: ResolutionStage,
    pub at_ms: u64,
    /// The refusal this event belongs to.
    pub refusal_digest: String,
    /// Whether the acceptance standard changed. Every remediation event
    /// carries it, and it MUST read false (§3.1, §36) unless a separately
    /// governed policy revision occurred outside the run.
    pub acceptance_standard_changed: bool,
    pub detail: String,
}

impl ResolutionEvent {
    pub fn new(stage: ResolutionStage, refusal_digest: &str, at_ms: u64, detail: &str) -> Self {
        Self {
            stage,
            at_ms,
            refusal_digest: refusal_digest.to_string(),
            acceptance_standard_changed: false,
            detail: detail.to_string(),
        }
    }
}
