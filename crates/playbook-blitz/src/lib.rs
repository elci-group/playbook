//! Blitz: defensive parallel single-focus inference.
//!
//! When the expected cost of delay exceeds the expected marginal cost of
//! redundant inference, Playbook may deliberately purchase time with
//! parallel inference. A blitz dispatches multiple independently operating
//! workers against the same frozen target, races their candidate artefacts
//! through deterministic verification, accepts the earliest candidate that
//! satisfies the win condition, and cancels redundant work the moment
//! continued execution stops buying defensive value.
//!
//! The rules this crate owns:
//!
//! * single focus — every worker attempts the same frozen, content-
//!   addressable target; candidates attributable to any other problem are
//!   refused;
//! * independent first wave — workers share frozen evidence and
//!   deterministic tool outputs, never each other's reasoning or
//!   candidates;
//! * verification before victory — the first response never wins; the first
//!   independently verified candidate does;
//! * bounded expenditure — concurrency, tokens, money, deadline, waves, and
//!   retries are policy ceilings; no worker may expand its envelope;
//! * early cancellation — the moment recovery is verified, redundant
//!   inference stops;
//! * fail closed — a failed controller, verifier, or budget governor never
//!   commits an unverified artefact;
//! * evidence preservation — every blitz reconstructs why it was invoked,
//!   who played, what each attempted, what won, what was cancelled, what it
//!   cost, and whether recovery actually occurred.
//!
//! Waves preserve independence before paying for coherence: wave 1 attacks
//! in isolation, wave 2 receives only failure evidence and verifier
//! diagnostics, and only a failed independence attempt escalates into a
//! Huddle (`playbook-huddle`).
//!
//! Persistence, event sourcing, and the drive fold live in
//! `playbook-state`, which projects drive events onto
//! [`apply::BlitzEvent`] and folds them through [`apply::apply_blitz`].

#![forbid(unsafe_code)]

pub mod admission;
pub mod apply;
pub mod budget;
pub mod domain;
pub mod economics;
pub mod escalation;
pub mod race;
pub mod roster;
pub mod trigger;
pub mod verify;

pub use admission::{authorise, Authorisation, AuthorisationInputs, BlitzValue};
pub use apply::{apply_blitz, fold_blitz, BlitzEvent, BlitzView};
pub use budget::{consume, exhausted};
pub use domain::{
    evidence_bundle_digest, refusal, ActiveBlitz, BlitzClass, BlitzClosure, BlitzCost,
    BlitzFailure, BlitzLedger, BlitzOutcome, BlitzPhase, BlitzRecord, BlitzResourceMode,
    BlitzTrigger, CancelSummary, CancelledWorker, Candidate, DefensivePressure,
    HuddleRecommendation, SelectedWinner, Severity, TargetPackage, VerificationKind,
    VerificationRecord, WorkerExitKind, WorkerScouting, WorkerSlot, WorkerSpent,
};
pub use economics::{report, BlitzReport, SequentialEstimate};
pub use escalation::{huddle_target, should_escalate_from_lastdown, wave_failure, ManmarkRequest};
pub use playbook_policy::BlitzCorroboration;
pub use race::{run_race, RaceSpec, VerifierPlan, WorkerPlan, CANCELLATION_LATENCY_MS};
pub use roster::{
    effective_parallelism_milli, marginal_independence_milli, select, worker_score, WorkerCandidate,
};
pub use trigger::{evaluate_eligibility, pressure_score, urgency_score, PressureInputs};
pub use verify::{verify_candidate, winner_may_commit, Verification};

/// Derived defensive readiness: the machine-readable summary of the
/// defensive state of a drive.
pub fn readiness(ledger: &domain::BlitzLedger) -> domain::BlitzPhase {
    match ledger.active_blitz() {
        Some(blitz) => blitz.phase,
        None if ledger.recovered_total() > 0 => domain::BlitzPhase::Recovered,
        None => domain::BlitzPhase::Proposed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readiness_reflects_the_ledger() {
        assert_eq!(
            readiness(&domain::BlitzLedger::empty()),
            domain::BlitzPhase::Proposed
        );
    }
}
