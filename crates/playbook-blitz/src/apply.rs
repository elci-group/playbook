//! Blitz state machine.
//!
//! Proposed → Authorised → FreezingTarget → Dispatching → Racing →
//! Verifying → Recovered → Cancelling, with Exhausted, Escalating, Failed,
//! and Aborted as the defensive exits. Every rule here is enforced at fold
//! time, so a replay of the journal reproduces the same blitz state or
//! refuses the tampered log.
//!
//! Wave 1 isolation is structural: no event can carry one worker's
//! reasoning or candidate to another wave-1 worker. The only sanctioned
//! cross-wave channel is `WaveClosed`, whose diagnostics are failure
//! evidence for a wave-2 reattack — never a live candidate.

use playbook_core::{BlitzBudget, BlitzId, PlaybookError};
use playbook_policy::{BlitzCorroboration, Policy};

use crate::budget;
use crate::domain::{
    refusal, ActiveBlitz, BlitzClosure, BlitzCost, BlitzFailure, BlitzLedger, BlitzOutcome,
    BlitzPhase, BlitzResourceMode, BlitzTrigger, CancelSummary, CancelledWorker, Candidate,
    DefensivePressure, HuddleRecommendation, SelectedWinner, TargetPackage, VerificationKind,
    VerificationRecord, WorkerExitKind, WorkerSlot, WorkerSpent,
};
use crate::roster;

/// The projection of a blitz event. `playbook-state` maps `DriveEvent`
/// variants onto this one-to-one and folds through `apply_blitz`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlitzEvent {
    Proposed {
        blitz_id: BlitzId,
        trigger: BlitzTrigger,
        target: Box<TargetPackage>,
        pressure: DefensivePressure,
        at_ms: u64,
    },
    Authorised {
        blitz_id: BlitzId,
        budget: BlitzBudget,
        corroboration: BlitzCorroboration,
        resource_mode: BlitzResourceMode,
        at_ms: u64,
    },
    Rejected {
        blitz_id: BlitzId,
        reason_code: String,
    },
    TargetFrozen {
        blitz_id: BlitzId,
        target_hash: String,
    },
    WorkerDispatched {
        blitz_id: BlitzId,
        worker: WorkerSlot,
        effective_parallelism_milli: u32,
    },
    WorkerStarted {
        blitz_id: BlitzId,
        worker_id: playbook_core::WorkerId,
        at_ms: u64,
    },
    WorkerExited {
        blitz_id: BlitzId,
        worker_id: playbook_core::WorkerId,
        kind: WorkerExitKind,
        at_ms: u64,
    },
    WorkerSpent {
        blitz_id: BlitzId,
        spent: WorkerSpent,
    },
    WaveClosed {
        blitz_id: BlitzId,
        wave: u16,
        diagnostics: Vec<String>,
    },
    CandidateReceived {
        blitz_id: BlitzId,
        candidate: Box<Candidate>,
    },
    VerificationStarted {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
    },
    CandidateRejected {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
        gate: VerificationKind,
        reason_code: String,
    },
    CandidateVerified {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
        passed: Vec<VerificationKind>,
    },
    VerificationUnavailable {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
    },
    HumanConfirmed {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
        authority: String,
    },
    WinnerSelected {
        blitz_id: BlitzId,
        candidate_id: playbook_core::CandidateId,
        worker_id: playbook_core::WorkerId,
        at_ms: u64,
    },
    CancelStarted {
        blitz_id: BlitzId,
        at_ms: u64,
    },
    WorkerCancelled {
        blitz_id: BlitzId,
        worker_id: playbook_core::WorkerId,
        avoided_tokens: u64,
        avoided_cost_microunits: u64,
    },
    CancelComplete {
        blitz_id: BlitzId,
        cancelled: u16,
        latency_ms: u64,
        avoided_tokens: u64,
        avoided_cost_microunits: u64,
    },
    RecoveryConfirmed {
        blitz_id: BlitzId,
        evidence_refs: Vec<playbook_core::EvidenceId>,
        at_ms: u64,
    },
    BudgetConsumed {
        blitz_id: BlitzId,
        cost: BlitzCost,
    },
    Exhausted {
        blitz_id: BlitzId,
        failure: BlitzFailure,
    },
    Escalated {
        blitz_id: BlitzId,
        recommendation: HuddleRecommendation,
    },
    Superseded {
        blitz_id: BlitzId,
        successor_id: BlitzId,
    },
    Failed {
        blitz_id: BlitzId,
        reason_code: String,
    },
    Aborted {
        blitz_id: BlitzId,
        reason_code: String,
    },
}

/// What the fold needs to know about the drive. No drive internals leak
/// into the blitz subsystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlitzView {
    pub active: bool,
    pub remaining_tokens: u64,
    pub remaining_cost_microunits: u64,
}

pub fn fold_blitz(
    ledger: &mut BlitzLedger,
    events: &[BlitzEvent],
    view: &BlitzView,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    for event in events {
        apply_blitz(ledger, event, view, policy)?;
    }
    Ok(())
}

/// The exhaustion economics sealed at closure time, built from the live
/// state while the blitz is still active.
fn exhaustion_outcome(blitz: &ActiveBlitz) -> BlitzOutcome {
    BlitzOutcome {
        target_hash: blitz.target.target_hash.clone(),
        winner: None,
        first_candidate_ms: blitz
            .candidates
            .iter()
            .map(|candidate| candidate.emitted_at_ms)
            .min()
            .map(|at| at.saturating_sub(blitz.at_ms)),
        time_to_verified_ms: None,
        time_to_recovery_ms: None,
        raw_parallelism: u16::try_from(blitz.workers.len()).unwrap_or(u16::MAX),
        effective_parallelism_milli: blitz.effective_parallelism_milli,
        correlated_failures: blitz.correlated_failures,
        tokens_to_recovery: blitz.spent.tokens,
        cost_to_recovery_microunits: blitz.spent.cost_microunits,
        cancellation: blitz.cancel_complete.clone(),
        worker_results: blitz.scouting(&blitz.worker_spend),
        failure: blitz.failure,
    }
}

pub fn apply_blitz(
    ledger: &mut BlitzLedger,
    event: &BlitzEvent,
    view: &BlitzView,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    match event {
        BlitzEvent::Proposed {
            blitz_id,
            trigger,
            target,
            pressure,
            at_ms,
        } => {
            if !view.active {
                return Err(PlaybookError::Closed);
            }
            if ledger.active_blitz().is_some() {
                return Err(refusal("formation_busy"));
            }
            if ledger.known_id(blitz_id) {
                return Err(refusal("duplicate blitz id"));
            }
            if let Some(record) = ledger.last_closed() {
                if record.target_hash == target.target_hash {
                    return Err(refusal("unchanged_target"));
                }
            }
            target.verify()?;
            ledger.push_active(ActiveBlitz {
                id: blitz_id.clone(),
                class: trigger.class,
                phase: BlitzPhase::Proposed,
                trigger: trigger.clone(),
                target: (**target).clone(),
                pressure: pressure.clone(),
                budget: BlitzBudget::zero(),
                spent: BlitzCost::zero(),
                corroboration: policy.blitz.corroboration,
                resource_mode: BlitzResourceMode::Normal,
                workers: Vec::new(),
                closed_waves: Vec::new(),
                exited: Vec::new(),
                candidates: Vec::new(),
                verifications: Vec::new(),
                human_confirmations: Vec::new(),
                winner: None,
                worker_spend: Vec::new(),
                cancellations: Vec::new(),
                cancel_started_at: None,
                cancel_complete: None,
                failure: None,
                correlated_failures: 0,
                effective_parallelism_milli: 0,
                at_ms: *at_ms,
                deadline_at_ms: at_ms.saturating_add(target.deadline_ms),
            });
            Ok(())
        }
        BlitzEvent::Authorised {
            blitz_id,
            budget,
            corroboration: _,
            resource_mode,
            at_ms: _,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.phase != BlitzPhase::Proposed {
                return Err(refusal("only a proposed blitz can be authorised"));
            }
            if budget.max_tokens == 0 || budget.max_workers == 0 || budget.max_waves == 0 {
                return Err(refusal("empty envelope"));
            }
            if budget.max_tokens > view.remaining_tokens
                || budget.max_cost_microunits > view.remaining_cost_microunits
            {
                return Err(refusal("no_budget"));
            }
            blitz.budget = *budget;
            blitz.resource_mode = *resource_mode;
            blitz.phase = BlitzPhase::Authorised;
            ledger.count_authorised();
            Ok(())
        }
        BlitzEvent::Rejected { blitz_id, .. } => {
            {
                let blitz = ledger.active_mut(blitz_id)?;
                if blitz.phase != BlitzPhase::Proposed {
                    return Err(refusal("only a proposed blitz can be rejected"));
                }
            }
            ledger.count_rejected();
            ledger.close_active(
                blitz_id,
                BlitzClosure::Rejected,
                "policy_rejection".into(),
                None,
            )
        }
        BlitzEvent::TargetFrozen {
            blitz_id,
            target_hash,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.phase != BlitzPhase::Authorised {
                return Err(refusal("only an authorised blitz can freeze its target"));
            }
            if *target_hash != blitz.target.target_hash {
                return Err(refusal("frozen hash does not match the sealed target"));
            }
            blitz.phase = BlitzPhase::FreezingTarget;
            Ok(())
        }
        BlitzEvent::WorkerDispatched {
            blitz_id,
            worker,
            effective_parallelism_milli,
        } => {
            {
                let blitz = ledger.active_mut(blitz_id)?;
                if matches!(blitz.phase, BlitzPhase::Proposed | BlitzPhase::Authorised) {
                    return Err(refusal("target is not frozen"));
                }
                if worker.wave == 0 || worker.wave > blitz.budget.max_waves {
                    return Err(refusal("wave outside the envelope"));
                }
                // The concurrency ceiling binds live workers, not
                // cumulative dispatches: terminal workers free their slots.
                if blitz.in_flight().len() >= usize::from(blitz.budget.max_workers) {
                    return Err(refusal("concurrency ceiling reached"));
                }
                if blitz
                    .workers
                    .iter()
                    .any(|slot| slot.worker_id == worker.worker_id)
                {
                    return Err(refusal("duplicate worker"));
                }
                if blitz.worker_terminal(&worker.worker_id) {
                    return Err(refusal("worker already terminal"));
                }
                if worker.wave > 1 && !blitz.wave_closed(worker.wave - 1) {
                    // Wave 2 may only learn from closed failure evidence.
                    return Err(refusal("previous wave is still open"));
                }
                if worker.dispatched_at_ms > blitz.deadline_at_ms {
                    return Err(refusal("dispatch past the deadline"));
                }
            }
            // Validate before mutating: a refused dispatch leaves the
            // formation untouched.
            let expected = {
                let blitz = ledger.active_mut(blitz_id)?;
                let mut scratch = blitz.workers.clone();
                scratch.push(worker.clone());
                roster::effective_parallelism_milli(&scratch)
            };
            if *effective_parallelism_milli != expected {
                return Err(refusal("effective parallelism mismatch"));
            }
            let blitz = ledger.active_mut(blitz_id)?;
            blitz.workers.push(worker.clone());
            blitz.effective_parallelism_milli = expected;
            if blitz.phase == BlitzPhase::FreezingTarget {
                blitz.phase = BlitzPhase::Dispatching;
            }
            Ok(())
        }
        BlitzEvent::WorkerStarted {
            blitz_id,
            worker_id,
            at_ms,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            let slot = blitz
                .workers
                .iter()
                .find(|slot| slot.worker_id == *worker_id)
                .ok_or_else(|| refusal("unknown worker"))?;
            if *at_ms < slot.dispatched_at_ms {
                return Err(refusal("worker started before dispatch"));
            }
            if blitz.phase == BlitzPhase::Dispatching {
                blitz.phase = BlitzPhase::Racing;
            }
            Ok(())
        }
        BlitzEvent::WorkerExited {
            blitz_id,
            worker_id,
            kind,
            at_ms: _,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if !blitz
                .workers
                .iter()
                .any(|slot| slot.worker_id == *worker_id)
            {
                return Err(refusal("unknown worker"));
            }
            if blitz.worker_terminal(worker_id) {
                return Err(refusal("worker already terminal"));
            }
            blitz.exited.push((worker_id.clone(), *kind));
            Ok(())
        }
        BlitzEvent::WorkerSpent { blitz_id, spent } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if !blitz
                .workers
                .iter()
                .any(|slot| slot.worker_id == spent.worker_id)
            {
                return Err(refusal("unknown worker"));
            }
            let row = blitz
                .worker_spend
                .iter_mut()
                .find(|row| row.worker_id == spent.worker_id);
            let tokens = spent.tokens;
            let cost = spent.cost_microunits;
            let latency = spent.latency_ms;
            match row {
                Some(row) => {
                    row.tokens = row.tokens.saturating_add(tokens);
                    row.cost_microunits = row.cost_microunits.saturating_add(cost);
                    row.latency_ms = row.latency_ms.max(latency);
                }
                None => blitz.worker_spend.push(WorkerSpent {
                    worker_id: spent.worker_id.clone(),
                    tokens,
                    cost_microunits: cost,
                    latency_ms: latency,
                }),
            }
            Ok(())
        }
        BlitzEvent::WaveClosed {
            blitz_id,
            wave,
            diagnostics,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.winner.is_some() {
                return Err(refusal("race already recovered"));
            }
            if diagnostics.iter().any(|line| line.trim().is_empty()) {
                return Err(refusal("wave diagnostics must be non-empty"));
            }
            if !blitz.wave_closed(*wave) {
                return Err(refusal("wave is not closed"));
            }
            if blitz.closed_waves.contains(wave) {
                return Err(refusal("wave already closed"));
            }
            blitz.closed_waves.push(*wave);
            blitz.closed_waves.sort_unstable();
            Ok(())
        }
        BlitzEvent::CandidateReceived {
            blitz_id,
            candidate,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if matches!(
                blitz.phase,
                BlitzPhase::Proposed | BlitzPhase::Authorised | BlitzPhase::FreezingTarget
            ) {
                return Err(refusal("race has not started"));
            }
            if blitz.phase == BlitzPhase::Verifying {
                // Streaming: candidates may arrive while verification runs.
            }
            let slot = blitz
                .workers
                .iter()
                .find(|slot| slot.worker_id == candidate.worker_id)
                .ok_or_else(|| refusal("unknown worker"))?;
            if blitz.worker_terminal(&candidate.worker_id) {
                return Err(refusal("worker already left the race"));
            }
            if slot.wave != candidate.wave {
                return Err(refusal("candidate wave does not match its worker"));
            }
            if &candidate.blitz_id != blitz_id {
                return Err(refusal("candidate names another blitz"));
            }
            // Single focus: a candidate is only valid for the problem the
            // worker was actually frozen against.
            if candidate.target_hash != blitz.target.target_hash {
                return Err(refusal("target hash mismatch"));
            }
            if blitz
                .candidates
                .iter()
                .any(|known| known.candidate_id == candidate.candidate_id)
            {
                return Err(refusal("duplicate candidate id"));
            }
            if blitz
                .candidates
                .iter()
                .any(|known| known.worker_id == candidate.worker_id)
            {
                return Err(refusal("worker already emitted a candidate"));
            }
            if candidate.emitted_at_ms > blitz.deadline_at_ms {
                return Err(refusal("candidate past the deadline"));
            }
            if candidate.confidence_ppm > playbook_core::PPM_SCALE {
                return Err(refusal("confidence_ppm out of range"));
            }
            blitz.candidates.push(*candidate.clone());
            Ok(())
        }
        BlitzEvent::VerificationStarted {
            blitz_id,
            candidate_id,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            let candidate = blitz
                .candidate(candidate_id)
                .ok_or_else(|| refusal("unknown candidate"))?;
            let worker_id = candidate.worker_id.clone();
            let attempts = blitz
                .verification(candidate_id)
                .map(|record| {
                    if !record.verifier_unavailable {
                        return Err(refusal("verification already started"));
                    }
                    if u32::from(record.attempts) > policy.blitz.max_verifier_retries {
                        return Err(refusal("verifier retry envelope exhausted"));
                    }
                    Ok(record.attempts)
                })
                .unwrap_or(Ok(0))?;
            if let Some(record) = blitz
                .verifications
                .iter_mut()
                .find(|record| record.candidate_id == *candidate_id)
            {
                record.verifier_unavailable = false;
                record.attempts = attempts + 1;
            } else {
                blitz.verifications.push(VerificationRecord {
                    candidate_id: candidate_id.clone(),
                    worker_id,
                    passed: Vec::new(),
                    failed: None,
                    verifier_unavailable: false,
                    attempts: attempts + 1,
                });
            }
            if blitz.phase == BlitzPhase::Racing || blitz.phase == BlitzPhase::Dispatching {
                blitz.phase = BlitzPhase::Verifying;
            }
            Ok(())
        }
        BlitzEvent::CandidateRejected {
            blitz_id,
            candidate_id,
            gate,
            reason_code,
        } => {
            let correlated = {
                let blitz = ledger.active_mut(blitz_id)?;
                let record = blitz
                    .verification(candidate_id)
                    .ok_or_else(|| refusal("verification has not started"))?;
                if !record.passed.is_empty() || record.failed.is_some() {
                    return Err(refusal("candidate already resolved"));
                }
                if reason_code.trim().is_empty() {
                    return Err(refusal("rejection requires a reason code"));
                }
                let candidate = blitz
                    .candidate(candidate_id)
                    .ok_or_else(|| refusal("unknown candidate"))?;
                let digest = candidate.artefact_digest.clone();
                let record = blitz
                    .verifications
                    .iter_mut()
                    .find(|record| record.candidate_id == *candidate_id)
                    .expect("verification record exists");
                record.failed = Some((*gate, reason_code.clone()));
                // Correlated failure: another rejected candidate proposed the
                // same artefact. Raw worker counts overstate the search.
                let correlated = blitz.candidates.iter().any(|other| {
                    other.artefact_digest == digest
                        && other.candidate_id != *candidate_id
                        && blitz
                            .verification(&other.candidate_id)
                            .is_some_and(|record| {
                                record.failed.is_some() || record.verifier_unavailable
                            })
                });
                if correlated {
                    blitz.correlated_failures = blitz.correlated_failures.saturating_add(1);
                }
                correlated
            };
            if correlated {
                ledger.count_correlated();
            }
            Ok(())
        }
        BlitzEvent::CandidateVerified {
            blitz_id,
            candidate_id,
            passed,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            let record = blitz
                .verification(candidate_id)
                .ok_or_else(|| refusal("verification has not started"))?;
            if !record.passed.is_empty() || record.failed.is_some() {
                return Err(refusal("candidate already resolved"));
            }
            let required = &blitz.target.verification_gates;
            if passed.len() != required.len() || !required.iter().all(|gate| passed.contains(gate))
            {
                return Err(refusal(
                    "gate set does not cover the frozen verification gates",
                ));
            }
            let record = blitz
                .verifications
                .iter_mut()
                .find(|record| record.candidate_id == *candidate_id)
                .expect("verification record exists");
            record.passed = passed.clone();
            Ok(())
        }
        BlitzEvent::VerificationUnavailable {
            blitz_id,
            candidate_id,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            let record = blitz
                .verification(candidate_id)
                .ok_or_else(|| refusal("verification has not started"))?;
            if !record.passed.is_empty() || record.failed.is_some() {
                return Err(refusal("candidate already resolved"));
            }
            let record = blitz
                .verifications
                .iter_mut()
                .find(|record| record.candidate_id == *candidate_id)
                .expect("verification record exists");
            record.verifier_unavailable = true;
            Ok(())
        }
        BlitzEvent::HumanConfirmed {
            blitz_id,
            candidate_id,
            authority,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if authority.trim().is_empty() {
                return Err(refusal("human confirmation requires a named authority"));
            }
            let verified = blitz.verified_candidates();
            if !verified
                .iter()
                .any(|candidate| candidate.candidate_id == *candidate_id)
            {
                return Err(refusal("only a verified candidate can be human-confirmed"));
            }
            if blitz
                .human_confirmations
                .iter()
                .any(|(confirmed, _)| confirmed == candidate_id)
            {
                return Err(refusal("candidate already human-confirmed"));
            }
            blitz
                .human_confirmations
                .push((candidate_id.clone(), authority.clone()));
            Ok(())
        }
        BlitzEvent::WinnerSelected {
            blitz_id,
            candidate_id,
            worker_id,
            at_ms,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.winner.is_some() {
                // Exactly one authoritative winning transition, ever.
                return Err(refusal("winner already selected"));
            }
            if matches!(
                blitz.phase,
                BlitzPhase::Proposed
                    | BlitzPhase::Authorised
                    | BlitzPhase::FreezingTarget
                    | BlitzPhase::Dispatching
                    | BlitzPhase::Exhausted
            ) {
                return Err(refusal("race is not verifying"));
            }
            let candidate = blitz
                .candidate(candidate_id)
                .ok_or_else(|| refusal("unknown candidate"))?;
            if &candidate.worker_id != worker_id {
                return Err(refusal("winner does not match its worker"));
            }
            if candidate.target_hash != blitz.target.target_hash {
                return Err(refusal("target hash mismatch"));
            }
            let record = blitz
                .verification(candidate_id)
                .ok_or_else(|| refusal("candidate is unverified"))?;
            if record.failed.is_some() || record.verifier_unavailable || record.passed.is_empty() {
                return Err(refusal("candidate is unverified"));
            }
            if !blitz.corroboration_satisfied(&blitz.corroboration) {
                return Err(refusal("corroboration is not satisfied"));
            }
            let human_confirmed = blitz
                .human_confirmations
                .iter()
                .any(|(confirmed, _)| confirmed == candidate_id);
            blitz.winner = Some(SelectedWinner {
                candidate_id: candidate_id.clone(),
                worker_id: worker_id.clone(),
                at_ms: *at_ms,
                human_confirmed,
            });
            blitz.phase = BlitzPhase::Recovered;
            Ok(())
        }
        BlitzEvent::CancelStarted { blitz_id, at_ms } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.winner.is_none() {
                return Err(refusal("nothing to cancel"));
            }
            if blitz.cancel_started_at.is_some() {
                return Err(refusal("cancellation already started"));
            }
            blitz.cancel_started_at = Some(*at_ms);
            blitz.phase = BlitzPhase::Cancelling;
            Ok(())
        }
        BlitzEvent::WorkerCancelled {
            blitz_id,
            worker_id,
            avoided_tokens,
            avoided_cost_microunits,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.cancel_started_at.is_none() {
                return Err(refusal("cancellation has not started"));
            }
            if !blitz
                .workers
                .iter()
                .any(|slot| slot.worker_id == *worker_id)
            {
                return Err(refusal("unknown worker"));
            }
            if blitz.worker_terminal(worker_id) {
                return Err(refusal("worker already terminal"));
            }
            blitz.cancellations.push(CancelledWorker {
                worker_id: worker_id.clone(),
                avoided_tokens: *avoided_tokens,
                avoided_cost_microunits: *avoided_cost_microunits,
            });
            Ok(())
        }
        BlitzEvent::CancelComplete {
            blitz_id,
            cancelled,
            latency_ms,
            avoided_tokens,
            avoided_cost_microunits,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.cancel_started_at.is_none() {
                return Err(refusal("cancellation has not started"));
            }
            if blitz.cancel_complete.is_some() {
                return Err(refusal("cancellation already complete"));
            }
            let in_flight = blitz.in_flight();
            let expected = in_flight.len();
            if !in_flight.is_empty() {
                return Err(refusal("workers are still in flight"));
            }
            if usize::from(*cancelled) != blitz.cancellations.len().max(expected) {
                // `cancelled` must account for every worker the cancellation
                // signal reached.
                return Err(refusal("cancelled count does not match the formation"));
            }
            blitz.cancel_complete = Some(CancelSummary {
                cancelled: *cancelled,
                latency_ms: *latency_ms,
                avoided_tokens: *avoided_tokens,
                avoided_cost_microunits: *avoided_cost_microunits,
            });
            Ok(())
        }
        BlitzEvent::RecoveryConfirmed {
            blitz_id,
            evidence_refs,
            at_ms,
        } => {
            let outcome = {
                let blitz = ledger.active_mut(blitz_id)?;
                let winner = blitz
                    .winner
                    .clone()
                    .ok_or_else(|| refusal("no winner to confirm"))?;
                if blitz.cancel_complete.is_none() {
                    return Err(refusal("cancellation must complete before recovery"));
                }
                if evidence_refs.is_empty() {
                    return Err(refusal("recovery requires evidence"));
                }
                let first_candidate_ms = blitz
                    .candidates
                    .iter()
                    .map(|candidate| candidate.emitted_at_ms)
                    .min()
                    .map(|at| at.saturating_sub(blitz.at_ms));
                let verified_ms = winner.at_ms.saturating_sub(blitz.at_ms);
                BlitzOutcome {
                    target_hash: blitz.target.target_hash.clone(),
                    winner: Some(winner),
                    first_candidate_ms,
                    time_to_verified_ms: Some(verified_ms),
                    time_to_recovery_ms: Some(at_ms.saturating_sub(blitz.at_ms)),
                    raw_parallelism: u16::try_from(blitz.workers.len()).unwrap_or(u16::MAX),
                    effective_parallelism_milli: blitz.effective_parallelism_milli,
                    correlated_failures: blitz.correlated_failures,
                    tokens_to_recovery: blitz.spent.tokens,
                    cost_to_recovery_microunits: blitz.spent.cost_microunits,
                    cancellation: blitz.cancel_complete.clone(),
                    worker_results: blitz.scouting(&blitz.worker_spend),
                    failure: None,
                }
            };
            ledger.count_recovered();
            ledger.close_active(
                blitz_id,
                BlitzClosure::Recovered,
                "recovery_confirmed".into(),
                Some(outcome),
            )
        }
        BlitzEvent::BudgetConsumed { blitz_id, cost } => {
            let blitz = ledger.active_mut(blitz_id)?;
            blitz.spent = budget::consume(blitz.spent, &blitz.budget, cost)?;
            Ok(())
        }
        BlitzEvent::Exhausted { blitz_id, failure } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.winner.is_some() {
                return Err(refusal("a recovered blitz cannot exhaust"));
            }
            if blitz.failure.is_some() {
                return Err(refusal("failure already recorded"));
            }
            match failure {
                BlitzFailure::PolicyRejection
                | BlitzFailure::TargetDrift
                | BlitzFailure::RecoveryRegression => {
                    return Err(refusal("failure class closes through another event"));
                }
                other => {
                    blitz.failure = Some(*other);
                }
            }
            // The blitz stays active in EXHAUSTED so the escalation policy
            // can observe it; Escalated or Aborted performs the closure and
            // seals the exhaustion economics.
            blitz.phase = BlitzPhase::Exhausted;
            ledger.count_exhausted();
            Ok(())
        }
        BlitzEvent::Escalated {
            blitz_id,
            recommendation,
        } => {
            let blitz = ledger.active_mut(blitz_id)?;
            if blitz.phase != BlitzPhase::Exhausted {
                return Err(refusal("only an exhausted blitz escalates"));
            }
            if blitz.winner.is_some() {
                return Err(refusal("a recovered blitz cannot escalate"));
            }
            if recommendation.scope_key.trim().is_empty()
                || !matches!(
                    recommendation.scope_key.as_str(),
                    "position" | "unit" | "coordinator" | "team" | "franchise"
                )
            {
                return Err(refusal("unknown huddle scope"));
            }
            if recommendation.reason_code.trim().is_empty() {
                return Err(refusal("escalation requires a reason code"));
            }
            if recommendation.failure != blitz.failure {
                return Err(refusal("escalation failure does not match the blitz"));
            }
            let outcome = exhaustion_outcome(blitz);
            ledger.count_escalated();
            ledger.close_active(
                blitz_id,
                BlitzClosure::Escalated,
                recommendation.reason_code.clone(),
                Some(outcome),
            )
        }
        BlitzEvent::Superseded {
            blitz_id,
            successor_id,
        } => {
            {
                let blitz = ledger.active_mut(blitz_id)?;
                if blitz.winner.is_some() {
                    return Err(refusal("a recovered blitz cannot be superseded"));
                }
                if successor_id == blitz_id {
                    return Err(refusal("a blitz cannot supersede itself"));
                }
                if ledger.known_id(successor_id) {
                    return Err(refusal("successor already exists"));
                }
            }
            ledger.close_active(
                blitz_id,
                BlitzClosure::Superseded,
                "target_drift".into(),
                None,
            )
        }
        BlitzEvent::Failed {
            blitz_id,
            reason_code,
        } => {
            {
                let blitz = ledger.active_mut(blitz_id)?;
                if blitz.winner.is_some() {
                    return Err(refusal("a recovered blitz cannot fail"));
                }
                if reason_code.trim().is_empty() {
                    return Err(refusal("failure requires a reason code"));
                }
                blitz.phase = BlitzPhase::Failed;
            }
            ledger.close_active(blitz_id, BlitzClosure::Failed, reason_code.clone(), None)
        }
        BlitzEvent::Aborted {
            blitz_id,
            reason_code,
        } => {
            let outcome = {
                let blitz = ledger.active_mut(blitz_id)?;
                if reason_code.trim().is_empty() {
                    return Err(refusal("abort requires a reason code"));
                }
                if blitz.winner.is_some() {
                    return Err(refusal("a recovered blitz cannot be aborted"));
                }
                blitz.phase = BlitzPhase::Aborted;
                // An exhausted blitz that closes without escalation still
                // seals its economics; an aborted live race has none.
                (blitz.failure.is_some()).then(|| exhaustion_outcome(blitz))
            };
            ledger.close_active(
                blitz_id,
                BlitzClosure::Aborted,
                reason_code.clone(),
                outcome,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{BlitzClass, BlitzCost, Severity, VerificationKind};
    use playbook_core::{CandidateId, EventId, EvidenceId, WorkerId, PPM_SCALE};
    use playbook_policy::BlitzCorroboration;

    fn policy() -> Policy {
        Policy::builtin()
    }

    fn view() -> BlitzView {
        BlitzView {
            active: true,
            remaining_tokens: 1_000_000,
            remaining_cost_microunits: 100_000_000,
        }
    }

    fn target(hash_seed: &str) -> TargetPackage {
        TargetPackage {
            problem_statement: "restore the failing component".into(),
            evidence_refs: Vec::new(),
            artefact_refs: Vec::new(),
            constraints: Vec::new(),
            forbidden_actions: Vec::new(),
            expected_output: "a patch".into(),
            win_condition: "gates pass".into(),
            verification_gates: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
            deadline_ms: 10_000,
            target_hash: String::new(),
        }
        .seal()
        .map(|mut sealed| {
            if !hash_seed.is_empty() {
                // A different problem seals to a different hash; the seed
                // keeps supersession scenarios readable.
                sealed.problem_statement = format!("restore the failing component ({hash_seed})");
                sealed = sealed.seal().unwrap();
            }
            sealed
        })
        .unwrap()
    }

    fn pressure() -> DefensivePressure {
        DefensivePressure {
            severity: Severity::Critical,
            deadline_ms: Some(10_000),
            deterioration_rate_ppm: 0,
            blast_radius_ppm: 900,
            solver_uncertainty_ppm: 700,
            containment_confidence_ppm: 100,
            recovery_confidence_ppm: 800,
            resource_decay_ppm: 400,
            expected_delay_loss: 50,
        }
    }

    fn trigger() -> BlitzTrigger {
        BlitzTrigger {
            class: BlitzClass::Instinctive,
            reason_code: "power_critical".into(),
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-1").unwrap(),
        }
    }

    fn budget() -> BlitzBudget {
        BlitzBudget {
            max_tokens: 40_000,
            max_cost_microunits: 4_000_000,
            max_duration_ms: 120_000,
            max_workers: 4,
            max_waves: 3,
        }
    }

    fn worker(id: &str, wave: u16, domains: &[&str]) -> WorkerSlot {
        WorkerSlot {
            worker_id: WorkerId::new(id).unwrap(),
            personnel_id: playbook_core::PersonnelId::new(id).unwrap(),
            provider: format!("provider-{}", id.chars().last().unwrap()),
            model_family: format!("family-{}", id.chars().last().unwrap()),
            failure_domains: domains.iter().map(|d| (*d).to_string()).collect(),
            wave,
            dispatched_at_ms: 100,
            deadline_ms: 5_000,
        }
    }

    fn candidate(id: &str, worker: &str, digest: &str) -> Candidate {
        Candidate {
            candidate_id: CandidateId::new(id).unwrap(),
            blitz_id: BlitzId::new("b-1").unwrap(),
            worker_id: WorkerId::new(worker).unwrap(),
            target_hash: target("").target_hash,
            artefact_digest: digest.into(),
            claim: "restores the component".into(),
            confidence_ppm: PPM_SCALE,
            evidence_refs: Vec::new(),
            verification_hint: String::new(),
            emitted_at_ms: 200,
            wave: 1,
        }
    }

    fn open_blitz(ledger: &mut BlitzLedger) -> BlitzId {
        let id = BlitzId::new("b-1").unwrap();
        fold_blitz(
            ledger,
            &[
                BlitzEvent::Proposed {
                    blitz_id: id.clone(),
                    trigger: trigger(),
                    target: Box::new(target("")),
                    pressure: pressure(),
                    at_ms: 100,
                },
                BlitzEvent::Authorised {
                    blitz_id: id.clone(),
                    budget: budget(),
                    corroboration: BlitzCorroboration::FirstPass,
                    resource_mode: BlitzResourceMode::PowerCritical,
                    at_ms: 100,
                },
                BlitzEvent::TargetFrozen {
                    blitz_id: id.clone(),
                    target_hash: target("").target_hash,
                },
            ],
            &view(),
            &policy(),
        )
        .unwrap();
        id
    }

    fn dispatch_two(ledger: &mut BlitzLedger, id: &BlitzId) {
        for (name, domains) in [("w-1", vec!["dc-a"]), ("w-2", vec!["dc-b"])] {
            let worker = worker(name, 1, &domains);
            let effective = roster::effective_parallelism_milli(&{
                let blitz = ledger.active_blitz().unwrap();
                let mut workers = blitz.workers.clone();
                workers.push(worker.clone());
                workers
            });
            apply_blitz(
                ledger,
                &BlitzEvent::WorkerDispatched {
                    blitz_id: id.clone(),
                    worker,
                    effective_parallelism_milli: effective,
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        apply_blitz(
            ledger,
            &BlitzEvent::WorkerStarted {
                blitz_id: id.clone(),
                worker_id: WorkerId::new("w-1").unwrap(),
                at_ms: 120,
            },
            &view(),
            &policy(),
        )
        .unwrap();
    }

    #[test]
    fn a_blitz_lifecycle_recovers_and_closes() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateReceived {
                blitz_id: id.clone(),
                candidate: Box::new(candidate("c-1", "w-1", "digest-1")),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateVerified {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
                passed: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WinnerSelected {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
                worker_id: WorkerId::new("w-1").unwrap(),
                at_ms: 300,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.active_blitz().unwrap().phase, BlitzPhase::Recovered);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CancelStarted {
                blitz_id: id.clone(),
                at_ms: 300,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerCancelled {
                blitz_id: id.clone(),
                worker_id: WorkerId::new("w-2").unwrap(),
                avoided_tokens: 500,
                avoided_cost_microunits: 50_000,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CancelComplete {
                blitz_id: id.clone(),
                cancelled: 1,
                latency_ms: 12,
                avoided_tokens: 500,
                avoided_cost_microunits: 50_000,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerSpent {
                blitz_id: id.clone(),
                spent: WorkerSpent {
                    worker_id: WorkerId::new("w-1").unwrap(),
                    tokens: 400,
                    cost_microunits: 40_000,
                    latency_ms: 200,
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::BudgetConsumed {
                blitz_id: id.clone(),
                cost: BlitzCost {
                    tokens: 900,
                    cost_microunits: 90_000,
                    duration_ms: 200,
                    workers: 2,
                    waves: 1,
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::RecoveryConfirmed {
                blitz_id: id.clone(),
                evidence_refs: vec![EvidenceId::new("ev-recovery").unwrap()],
                at_ms: 320,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(ledger.active().is_empty());
        assert_eq!(ledger.recovered_total(), 1);
        let record = &ledger.closed()[0];
        assert_eq!(record.closure, BlitzClosure::Recovered);
        let outcome = record.outcome.as_ref().unwrap();
        assert_eq!(outcome.time_to_recovery_ms, Some(220));
        assert_eq!(outcome.tokens_to_recovery, 900);
        assert_eq!(outcome.cancellation.as_ref().unwrap().avoided_tokens, 500);
        assert_eq!(outcome.worker_results.len(), 2);
    }

    #[test]
    fn a_fast_wrong_answer_never_wins() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateReceived {
                blitz_id: id.clone(),
                candidate: Box::new(candidate("c-1", "w-1", "digest-1")),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateRejected {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
                gate: VerificationKind::UnitTest,
                reason_code: "unit_failure".into(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // The fastest response lost; a winner may not be committed on it.
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::WinnerSelected {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new("c-1").unwrap(),
                    worker_id: WorkerId::new("w-1").unwrap(),
                    at_ms: 210,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("candidate is unverified")
        );
        // The race continues: w-2's slower correct answer wins.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateReceived {
                blitz_id: id.clone(),
                candidate: Box::new(candidate("c-2", "w-2", "digest-2")),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-2").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateVerified {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-2").unwrap(),
                passed: vec![VerificationKind::UnitTest, VerificationKind::Compilation],
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WinnerSelected {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-2").unwrap(),
                worker_id: WorkerId::new("w-2").unwrap(),
                at_ms: 400,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            ledger
                .active_blitz()
                .unwrap()
                .winner
                .as_ref()
                .unwrap()
                .worker_id,
            WorkerId::new("w-2").unwrap()
        );
    }

    #[test]
    fn an_unverified_or_mismatched_candidate_is_refused() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        // Single focus: a candidate for another target never enters.
        let mut drifted = candidate("c-1", "w-1", "digest-1");
        drifted.target_hash = "another-problem".into();
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateReceived {
                    blitz_id: id.clone(),
                    candidate: Box::new(drifted),
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("target hash mismatch")
        );
        // A cancelled worker cannot emit after leaving the race.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerExited {
                blitz_id: id.clone(),
                worker_id: WorkerId::new("w-2").unwrap(),
                kind: WorkerExitKind::ProviderFailure,
                at_ms: 150,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateReceived {
                    blitz_id: id.clone(),
                    candidate: Box::new(candidate("c-2", "w-2", "digest-2")),
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("worker already left the race")
        );
    }

    #[test]
    fn wave_two_requires_a_closed_wave_one() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        // Wave 1 is open: w-2 has not terminated.
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::WorkerDispatched {
                    blitz_id: id.clone(),
                    worker: worker("w-3", 2, &["dc-c"]),
                    effective_parallelism_milli: 3_000,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("previous wave is still open")
        );
        // w-1 rejects, w-2 exits: wave 1 closes, diagnostics flow, wave 2
        // may now attack with the failure evidence.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateReceived {
                blitz_id: id.clone(),
                candidate: Box::new(candidate("c-1", "w-1", "digest-1")),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateRejected {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
                gate: VerificationKind::UnitTest,
                reason_code: "unit_failure".into(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerExited {
                blitz_id: id.clone(),
                worker_id: WorkerId::new("w-2").unwrap(),
                kind: WorkerExitKind::Timeout,
                at_ms: 5_100,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WaveClosed {
                blitz_id: id.clone(),
                wave: 1,
                diagnostics: vec!["w-1: unit_failure".into(), "w-2: timeout".into()],
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // The engine computes effective parallelism itself; a mismatched
        // claim is refused and the true value is recorded instead.
        let mut workers = ledger.active_blitz().unwrap().workers.clone();
        let w3 = worker("w-3", 2, &["dc-c"]);
        workers.push(w3.clone());
        let with_w3 = roster::effective_parallelism_milli(&workers);
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::WorkerDispatched {
                    blitz_id: id.clone(),
                    worker: w3.clone(),
                    effective_parallelism_milli: 2_999,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("effective parallelism mismatch")
        );
        assert_eq!(
            ledger.active_blitz().unwrap().effective_parallelism_milli,
            roster::effective_parallelism_milli(&ledger.active_blitz().unwrap().workers)
        );
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerDispatched {
                blitz_id: id.clone(),
                worker: w3,
                effective_parallelism_milli: with_w3,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.active_blitz().unwrap().workers.len(), 3);
    }

    #[test]
    fn correlated_rejections_are_counted_once_per_pair() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        // Both workers converge on the same invalid fix.
        for (candidate_id, worker) in [("c-1", "w-1"), ("c-2", "w-2")] {
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateReceived {
                    blitz_id: id.clone(),
                    candidate: Box::new(candidate(candidate_id, worker, "same-digest")),
                },
                &view(),
                &policy(),
            )
            .unwrap();
            apply_blitz(
                &mut ledger,
                &BlitzEvent::VerificationStarted {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new(candidate_id).unwrap(),
                },
                &view(),
                &policy(),
            )
            .unwrap();
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateRejected {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new(candidate_id).unwrap(),
                    gate: VerificationKind::Compilation,
                    reason_code: "compile_failure".into(),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        assert_eq!(ledger.active_blitz().unwrap().correlated_failures, 1);
        assert_eq!(ledger.correlated_total(), 1);
    }

    #[test]
    fn budget_governor_refuses_expansion_and_exhaustion_closes() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::BudgetConsumed {
                blitz_id: id.clone(),
                cost: BlitzCost {
                    tokens: 40_000,
                    cost_microunits: 4_000_000,
                    duration_ms: 120_000,
                    workers: 2,
                    waves: 1,
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::BudgetConsumed {
                    blitz_id: id.clone(),
                    cost: BlitzCost {
                        tokens: 1,
                        cost_microunits: 0,
                        duration_ms: 0,
                        workers: 0,
                        waves: 0,
                    },
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            PlaybookError::BudgetExhausted
        );
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Exhausted {
                blitz_id: id.clone(),
                failure: BlitzFailure::BudgetExhausted,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // The blitz stays observable in EXHAUSTED for the escalation policy.
        let active = ledger.active_blitz().unwrap();
        assert_eq!(active.phase, BlitzPhase::Exhausted);
        assert_eq!(active.failure, Some(BlitzFailure::BudgetExhausted));
        assert_eq!(ledger.exhausted_total(), 1);
        // Without escalation the caller aborts; the economics still seal.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Aborted {
                blitz_id: id.clone(),
                reason_code: "no_escalation_available".into(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(ledger.active().is_empty());
        assert_eq!(ledger.closed()[0].closure, BlitzClosure::Aborted);
        assert_eq!(
            ledger.closed()[0].outcome.as_ref().unwrap().failure,
            Some(BlitzFailure::BudgetExhausted)
        );
    }

    #[test]
    fn exhausted_blitz_escalates_to_a_huddle_recommendation() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Exhausted {
                blitz_id: id.clone(),
                failure: BlitzFailure::InvalidCandidates,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        let recommendation =
            crate::escalation::huddle_target(ledger.active_blitz().unwrap()).unwrap();
        assert_eq!(recommendation.scope_key, "team");
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Escalated {
                blitz_id: id.clone(),
                recommendation,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(ledger.active().is_empty());
        assert_eq!(ledger.escalated_total(), 1);
        assert_eq!(ledger.closed()[0].closure, BlitzClosure::Escalated);
    }

    #[test]
    fn fail_closed_verifier_blocks_victory_until_a_policy_bounded_retry() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CandidateReceived {
                blitz_id: id.clone(),
                candidate: Box::new(candidate("c-1", "w-1", "digest-1")),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationUnavailable {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::WinnerSelected {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new("c-1").unwrap(),
                    worker_id: WorkerId::new("w-1").unwrap(),
                    at_ms: 300,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("candidate is unverified")
        );
        // One policy-bounded restart is allowed.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationStarted {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(
            !ledger
                .active_blitz()
                .unwrap()
                .verification(&CandidateId::new("c-1").unwrap())
                .unwrap()
                .verifier_unavailable
        );
        // A second unavailable verdict exhausts the retry envelope.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::VerificationUnavailable {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::VerificationStarted {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new("c-1").unwrap(),
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("verifier retry envelope exhausted")
        );
    }

    #[test]
    fn only_one_winner_ever_and_cancellation_precedes_recovery() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        for (candidate_id, worker) in [("c-1", "w-1"), ("c-2", "w-2")] {
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateReceived {
                    blitz_id: id.clone(),
                    candidate: Box::new(candidate(
                        candidate_id,
                        worker,
                        &format!("digest-{worker}"),
                    )),
                },
                &view(),
                &policy(),
            )
            .unwrap();
            apply_blitz(
                &mut ledger,
                &BlitzEvent::VerificationStarted {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new(candidate_id).unwrap(),
                },
                &view(),
                &policy(),
            )
            .unwrap();
            apply_blitz(
                &mut ledger,
                &BlitzEvent::CandidateVerified {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new(candidate_id).unwrap(),
                    passed: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WinnerSelected {
                blitz_id: id.clone(),
                candidate_id: CandidateId::new("c-1").unwrap(),
                worker_id: WorkerId::new("w-1").unwrap(),
                at_ms: 300,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // The simultaneous second winner is refused: atomicity.
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::WinnerSelected {
                    blitz_id: id.clone(),
                    candidate_id: CandidateId::new("c-2").unwrap(),
                    worker_id: WorkerId::new("w-2").unwrap(),
                    at_ms: 300,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("winner already selected")
        );
        // Recovery cannot be confirmed while redundant work is still running.
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::RecoveryConfirmed {
                    blitz_id: id.clone(),
                    evidence_refs: vec![EvidenceId::new("ev-1").unwrap()],
                    at_ms: 310,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("cancellation must complete before recovery")
        );
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CancelStarted {
                blitz_id: id.clone(),
                at_ms: 300,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::WorkerCancelled {
                blitz_id: id.clone(),
                worker_id: WorkerId::new("w-2").unwrap(),
                avoided_tokens: 700,
                avoided_cost_microunits: 70_000,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::CancelComplete {
                blitz_id: id.clone(),
                cancelled: 1,
                latency_ms: 5,
                avoided_tokens: 700,
                avoided_cost_microunits: 70_000,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::RecoveryConfirmed {
                blitz_id: id,
                evidence_refs: vec![EvidenceId::new("ev-1").unwrap()],
                at_ms: 320,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.closed()[0].closure, BlitzClosure::Recovered);
    }

    #[test]
    fn target_drift_supersedes_and_a_new_target_can_be_proposed() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        // Material incident facts changed mid-race.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Superseded {
                blitz_id: id.clone(),
                successor_id: BlitzId::new("b-2").unwrap(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.closed()[0].closure, BlitzClosure::Superseded);
        // The successor targets the revised problem.
        let revised = target("v2");
        fold_blitz(
            &mut ledger,
            &[
                BlitzEvent::Proposed {
                    blitz_id: BlitzId::new("b-2").unwrap(),
                    trigger: trigger(),
                    target: Box::new(revised),
                    pressure: pressure(),
                    at_ms: 400,
                },
                BlitzEvent::Authorised {
                    blitz_id: BlitzId::new("b-2").unwrap(),
                    budget: budget(),
                    corroboration: BlitzCorroboration::FirstPass,
                    resource_mode: BlitzResourceMode::Normal,
                    at_ms: 400,
                },
            ],
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            ledger.active_blitz().unwrap().id,
            BlitzId::new("b-2").unwrap()
        );
    }

    #[test]
    fn an_unchanged_target_is_a_refused_blitz_storm() {
        let mut ledger = BlitzLedger::empty();
        let id = BlitzId::new("b-1").unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Proposed {
                blitz_id: id.clone(),
                trigger: trigger(),
                target: Box::new(target("")),
                pressure: pressure(),
                at_ms: 100,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Rejected {
                blitz_id: id.clone(),
                reason_code: "expected_value".into(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            fold_blitz(
                &mut ledger,
                &[BlitzEvent::Proposed {
                    blitz_id: BlitzId::new("b-2").unwrap(),
                    trigger: trigger(),
                    target: Box::new(target("")),
                    pressure: pressure(),
                    at_ms: 200,
                }],
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("unchanged_target")
        );
    }

    #[test]
    fn inactive_drives_and_double_formations_are_refused() {
        let mut ledger = BlitzLedger::empty();
        let id = open_blitz(&mut ledger);
        dispatch_two(&mut ledger, &id);
        let closed_view = BlitzView {
            active: false,
            remaining_tokens: 1_000,
            remaining_cost_microunits: 1_000,
        };
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::Proposed {
                    blitz_id: BlitzId::new("b-2").unwrap(),
                    trigger: trigger(),
                    target: Box::new(target("v3")),
                    pressure: pressure(),
                    at_ms: 200,
                },
                &closed_view,
                &policy()
            )
            .unwrap_err(),
            PlaybookError::Closed
        );
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::Proposed {
                    blitz_id: BlitzId::new("b-2").unwrap(),
                    trigger: trigger(),
                    target: Box::new(target("v3")),
                    pressure: pressure(),
                    at_ms: 200,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("formation_busy")
        );
    }

    #[test]
    fn authorisation_refuses_an_empty_or_unaffordable_envelope() {
        let mut ledger = BlitzLedger::empty();
        let id = BlitzId::new("b-1").unwrap();
        // A bare proposal: not yet authorised, so authorisation still owns
        // the envelope decision.
        apply_blitz(
            &mut ledger,
            &BlitzEvent::Proposed {
                blitz_id: id.clone(),
                trigger: trigger(),
                target: Box::new(target("")),
                pressure: pressure(),
                at_ms: 100,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        let poor_view = BlitzView {
            active: true,
            remaining_tokens: 10,
            remaining_cost_microunits: 10,
        };
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::Authorised {
                    blitz_id: id.clone(),
                    budget: budget(),
                    corroboration: BlitzCorroboration::FirstPass,
                    resource_mode: BlitzResourceMode::Normal,
                    at_ms: 100,
                },
                &poor_view,
                &policy()
            )
            .unwrap_err(),
            refusal("no_budget")
        );
        let pocket = BlitzBudget {
            max_tokens: 0,
            max_cost_microunits: 0,
            max_duration_ms: 0,
            max_workers: 0,
            max_waves: 0,
        };
        assert_eq!(
            apply_blitz(
                &mut ledger,
                &BlitzEvent::Authorised {
                    blitz_id: id.clone(),
                    budget: pocket,
                    corroboration: BlitzCorroboration::FirstPass,
                    resource_mode: BlitzResourceMode::Normal,
                    at_ms: 100,
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("empty envelope")
        );
    }
}
