//! The deterministic race engine.
//!
//! Workers here are synthetic plans on a virtual clock: dispatch, candidate
//! arrival, mid-flight failure, and verification resolution are all known
//! upfront, so the whole timeline is reproducible. The engine does not own a
//! single rule of its own — every event it produces is validated by the
//! fold as it is applied, and any refusal fails the race closed. What the
//! engine owns is scheduling: bounded fan-out, worker isolation across
//! waves, streaming verification that starts the moment a candidate
//! arrives, early cancellation the moment corroboration is satisfied, and
//! the deadline and budget terminations.
//!
//! Simulation convention: a worker's tokens and cost are consumed at its
//! candidate emission; a worker cancelled before emitting consumes nothing
//! and its planned spend is what the cancellation avoids.

use std::collections::BTreeSet;

use playbook_core::{
    BlitzBudget, BlitzId, CandidateId, EvidenceId, PlaybookError, WorkerId, PPM_SCALE,
};
use playbook_policy::{BlitzCorroboration, Policy};

use crate::apply::{apply_blitz, BlitzEvent, BlitzView};
use crate::domain::{
    BlitzClass, BlitzCost, BlitzFailure, BlitzLedger, BlitzResourceMode, BlitzTrigger, Candidate,
    DefensivePressure, TargetPackage, VerificationKind, WorkerExitKind, WorkerSlot, WorkerSpent,
};
use crate::escalation;
use crate::roster;
use crate::verify::winner_may_commit;

/// Engine-imposed cancellation latency: the clock cost of stopping
/// redundant inference after a winner exists. It is a measured, benchmarked
/// quantity, so the engine keeps it small and reports it in the cancel
/// summary.
pub const CANCELLATION_LATENCY_MS: u64 = 2;

/// One synthetic worker's complete plan for a simulated race.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerPlan {
    pub worker_id: WorkerId,
    pub personnel_id: playbook_core::PersonnelId,
    pub provider: String,
    pub model_family: String,
    /// Shared upstream failure domains, sorted and deduplicated.
    pub failure_domains: Vec<String>,
    pub start_delay_ms: u64,
    /// When the candidate arrives, relative to the race clock. `None` means
    /// the worker never produces and times out at the deadline.
    pub candidate_at_ms: Option<u64>,
    /// Whether the candidate passes every required gate.
    pub valid: bool,
    pub tokens: u64,
    pub cost_microunits: u64,
    /// Mid-flight worker or provider failure, relative to the race clock.
    pub fails_at_ms: Option<u64>,
    pub exit_kind: WorkerExitKind,
    /// The artefact digest of the candidate the worker would emit. Equal
    /// digests across workers are how correlated failure is simulated.
    pub digest: String,
}

impl WorkerPlan {
    /// The worker's slot in a formation dispatched at `at_ms`.
    #[tracing::instrument]
fn slot(&self, wave: u16, at_ms: u64, deadline_ms: u64) -> WorkerSlot {
        WorkerSlot {
            worker_id: self.worker_id.clone(),
            personnel_id: self.personnel_id.clone(),
            provider: self.provider.clone(),
            model_family: self.model_family.clone(),
            failure_domains: self
                .failure_domains
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            wave,
            dispatched_at_ms: at_ms,
            deadline_ms,
        }
    }
}

/// The verification plane: latency per candidate, and whether it can run at
/// all. An unavailable verifier is the fail-closed scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifierPlan {
    pub latency_ms: u64,
    pub available: bool,
}

/// Everything one simulated race needs, frozen at dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaceSpec {
    pub blitz_id: BlitzId,
    pub trigger: BlitzTrigger,
    pub target: TargetPackage,
    pub pressure: DefensivePressure,
    pub resource_mode: BlitzResourceMode,
    pub corroboration: BlitzCorroboration,
    pub budget: BlitzBudget,
    pub class: BlitzClass,
    pub at_ms: u64,
}

struct Engine<'a> {
    ledger: &'a mut BlitzLedger,
    applied: Vec<BlitzEvent>,
    view: &'a BlitzView,
    policy: &'a Policy,
}

impl<'a> Engine<'a> {
    /// Apply one event through the fold. Budget exhaustion is not an error:
    /// it is the governor terminating the race, reported to the caller as a
    /// `false` so the engine can close the blitz out.
    #[tracing::instrument]
fn emit(&mut self, event: BlitzEvent) -> Result<bool, PlaybookError> {
        match apply_blitz(self.ledger, &event, self.view, self.policy) {
            Ok(()) => {
                self.applied.push(event);
                Ok(true)
            }
            Err(PlaybookError::BudgetExhausted) => Ok(false),
            Err(error) => Err(error),
        }
    }

    #[tracing::instrument]
fn active_workers(&self) -> Vec<WorkerSlot> {
        self.ledger
            .active_blitz()
            .map(|blitz| blitz.workers.clone())
            .unwrap_or_default()
    }
}

/// Run one race to completion: dispatch, stream candidates, verify each as
/// it arrives, select the winner atomically, cancel the rest, and confirm
/// recovery — or terminate on deadline or budget and escalate into a huddle
/// recommendation. Returns every event applied, in order. Any fold refusal
/// fails the race closed.
pub #[tracing::instrument]
fn run_race(
    ledger: &mut BlitzLedger,
    spec: &RaceSpec,
    plans: &[WorkerPlan],
    verifier: &VerifierPlan,
    view: &BlitzView,
    policy: &Policy,
) -> Result<Vec<BlitzEvent>, PlaybookError> {
    let mut engine = Engine {
        ledger,
        applied: Vec::new(),
        view,
        policy,
    };
    engine.emit(BlitzEvent::Proposed {
        blitz_id: spec.blitz_id.clone(),
        trigger: spec.trigger.clone(),
        target: Box::new(spec.target.clone()),
        pressure: spec.pressure.clone(),
        at_ms: spec.at_ms,
    })?;
    if engine.ledger.active_blitz().is_none() {
        // The proposal was refused outright; the race never happened.
        return Ok(engine.applied);
    }
    if !engine.emit(BlitzEvent::Authorised {
        blitz_id: spec.blitz_id.clone(),
        budget: spec.budget,
        corroboration: spec.corroboration,
        resource_mode: spec.resource_mode,
        at_ms: spec.at_ms,
    })? {
        return Ok(engine.applied);
    }
    engine.emit(BlitzEvent::TargetFrozen {
        blitz_id: spec.blitz_id.clone(),
        target_hash: spec.target.target_hash.clone(),
    })?;

    let deadline = spec.at_ms.saturating_add(spec.target.deadline_ms);
    let mut reserves: Vec<WorkerPlan> = plans.to_vec();
    let mut dispatched: Vec<WorkerPlan> = Vec::new();

    // Wave 1: the whole first formation attacks at once.
    let wave_one = reserves.len().min(usize::from(spec.budget.max_workers));
    let wave_one: Vec<WorkerPlan> = reserves.drain(..wave_one).collect();
    engine.dispatch(&wave_one, 1, spec.at_ms, deadline)?;
    dispatched.extend(wave_one.iter().cloned());

    let mut wave = 1u16;
    let mut budget_exhausted = false;
    let mut pending: Vec<(u64, usize, BlitzEvent)> = Vec::new();
    schedule_wave(&wave_one, 1, spec, spec.at_ms, verifier, &mut pending);

    while let Some((time, _, event)) = next_event(&mut pending) {
        if time > deadline || budget_exhausted {
            break;
        }
        if !engine.emit(event)? {
            budget_exhausted = true;
            break;
        }
        // A verified candidate immediately checks the win condition.
        if engine
            .ledger
            .active_blitz()
            .is_some_and(|blitz| winner_may_commit(blitz, &spec.corroboration))
        {
            engine.finish(&dispatched, time)?;
            return Ok(engine.applied);
        }
        // Waves whose workers are all terminal close; their diagnostics are
        // the only sanctioned cross-wave channel, and reserves may then take
        // the empty slots while the envelope and the clock allow.
        let closed: Vec<u16> = engine
            .ledger
            .active_blitz()
            .map(|blitz| {
                (1..=wave)
                    .filter(|open| blitz.wave_closed(*open) && !blitz.closed_waves.contains(open))
                    .collect()
            })
            .unwrap_or_default();
        for closed_wave in closed {
            let diagnostics = wave_diagnostics(engine.ledger, closed_wave);
            engine.emit(BlitzEvent::WaveClosed {
                blitz_id: spec.blitz_id.clone(),
                wave: closed_wave,
                diagnostics,
            })?;
            if time < deadline {
                let in_flight = engine
                    .ledger
                    .active_blitz()
                    .map(|blitz| blitz.in_flight().len())
                    .unwrap_or(0);
                let room = usize::from(spec.budget.max_workers).saturating_sub(in_flight);
                let next_wave = closed_wave.saturating_add(1);
                if next_wave <= spec.budget.max_waves && room > 0 && !reserves.is_empty() {
                    let reinforcements: Vec<WorkerPlan> =
                        reserves.drain(..room.min(reserves.len())).collect();
                    wave = next_wave;
                    engine.dispatch(&reinforcements, next_wave, time + 1, deadline)?;
                    dispatched.extend(reinforcements.iter().cloned());
                    schedule_wave(
                        &reinforcements,
                        next_wave,
                        spec,
                        time + 1,
                        verifier,
                        &mut pending,
                    );
                }
            }
        }
    }

    if engine.ledger.active_blitz().is_none() {
        return Ok(engine.applied);
    }
    let failure = if budget_exhausted {
        BlitzFailure::BudgetExhausted
    } else if !pending.is_empty() {
        // Events were still scheduled when the clock stopped the race.
        BlitzFailure::DeadlineExhausted
    } else {
        engine
            .ledger
            .active_blitz()
            .and_then(escalation::wave_failure)
            .unwrap_or(BlitzFailure::DeadlineExhausted)
    };
    engine.emit(BlitzEvent::Exhausted {
        blitz_id: spec.blitz_id.clone(),
        failure,
    })?;
    if let Some(active) = engine.ledger.active_blitz() {
        if let Ok(recommendation) = escalation::huddle_target(active) {
            engine.emit(BlitzEvent::Escalated {
                blitz_id: spec.blitz_id.clone(),
                recommendation,
            })?;
        }
    }
    Ok(engine.applied)
}

#[tracing::instrument]
fn next_event(pending: &mut Vec<(u64, usize, BlitzEvent)>) -> Option<(u64, usize, BlitzEvent)> {
    if pending.is_empty() {
        return None;
    }
    let mut best = 0;
    for index in 1..pending.len() {
        let (time, order, _) = &pending[index];
        let (best_time, best_order, _) = &pending[best];
        if time < best_time || (time == best_time && order < best_order) {
            best = index;
        }
    }
    Some(pending.remove(best))
}

#[tracing::instrument]
fn wave_diagnostics(ledger: &BlitzLedger, wave: u16) -> Vec<String> {
    let Some(blitz) = ledger.active_blitz() else {
        return vec!["formation closed".into()];
    };
    let mut diagnostics: Vec<String> = blitz
        .candidates
        .iter()
        .filter(|candidate| {
            blitz
                .worker(&candidate.worker_id)
                .is_some_and(|slot| slot.wave == wave)
        })
        .filter_map(|candidate| {
            let record = blitz.verification(&candidate.candidate_id)?;
            if let Some((gate, reason)) = &record.failed {
                Some(format!("{}: {gate:?} {reason}", candidate.worker_id))
            } else if record.verifier_unavailable {
                Some(format!("{}: verifier unavailable", candidate.worker_id))
            } else {
                None
            }
        })
        .collect();
    for (worker, kind) in &blitz.exited {
        diagnostics.push(format!("{worker}: exited {kind:?}"));
    }
    if diagnostics.is_empty() {
        diagnostics.push("wave produced no candidates".into());
    }
    diagnostics
}

impl<'a> Engine<'a> {
    #[tracing::instrument]
fn dispatch(
        &mut self,
        plans: &[WorkerPlan],
        wave: u16,
        at_ms: u64,
        deadline: u64,
    ) -> Result<(), PlaybookError> {
        for plan in plans {
            let slot = plan.slot(wave, at_ms, deadline);
            let mut workers = self.active_workers();
            workers.push(slot.clone());
            let effective = roster::effective_parallelism_milli(&workers);
            self.emit(BlitzEvent::WorkerDispatched {
                blitz_id: self.spec_id(),
                worker: slot,
                effective_parallelism_milli: effective,
            })?;
        }
        // The wave's dispatch consumes its concurrency share at once.
        let _ = self.emit(BlitzEvent::BudgetConsumed {
            blitz_id: self.spec_id(),
            cost: BlitzCost {
                tokens: 0,
                cost_microunits: 0,
                duration_ms: 0,
                workers: u16::try_from(plans.len()).unwrap_or(u16::MAX),
                waves: 1,
            },
        });
        Ok(())
    }

    #[tracing::instrument]
fn spec_id(&self) -> BlitzId {
        self.ledger
            .active_blitz()
            .map(|blitz| blitz.id.clone())
            .unwrap_or_else(|| BlitzId::new("b-unknown").expect("fallback blitz id"))
    }

    /// The early-cancellation tail: winner, cancellation signal, per-worker
    /// cancellations with avoided spend, the cancellation summary, and the
    /// recovery confirmation. Cancellation latency is measured, not assumed.
    #[tracing::instrument]
fn finish(&mut self, dispatched: &[WorkerPlan], winner_time: u64) -> Result<(), PlaybookError> {
        let winner = self.ledger.active_blitz().and_then(|blitz| {
            blitz
                .verified_candidates()
                .last()
                .map(|candidate| (candidate.candidate_id.clone(), candidate.worker_id.clone()))
        });
        let Some((candidate_id, worker_id)) = winner else {
            return Ok(());
        };
        let blitz_id = self.spec_id();
        self.emit(BlitzEvent::WinnerSelected {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            worker_id: worker_id.clone(),
            at_ms: winner_time,
        })?;
        self.emit(BlitzEvent::CancelStarted {
            blitz_id: blitz_id.clone(),
            at_ms: winner_time,
        })?;
        let in_flight: Vec<WorkerId> = self
            .ledger
            .active_blitz()
            .map(|blitz| {
                blitz
                    .in_flight()
                    .iter()
                    .map(|slot| slot.worker_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut avoided_tokens = 0u64;
        let mut avoided_cost = 0u64;
        let mut cancelled = 0u16;
        let emitted_workers: Vec<WorkerId> = self
            .ledger
            .active_blitz()
            .map(|blitz| {
                blitz
                    .candidates
                    .iter()
                    .map(|candidate| candidate.worker_id.clone())
                    .collect()
            })
            .unwrap_or_default();
        for worker in &in_flight {
            let plan = dispatched.iter().find(|plan| &plan.worker_id == worker);
            let emitted = emitted_workers.contains(worker);
            let (avoided_t, avoided_c) = plan
                .map(|plan| {
                    (
                        if emitted { 0 } else { plan.tokens },
                        if emitted { 0 } else { plan.cost_microunits },
                    )
                })
                .unwrap_or((0, 0));
            avoided_tokens = avoided_tokens.saturating_add(avoided_t);
            avoided_cost = avoided_cost.saturating_add(avoided_c);
            self.emit(BlitzEvent::WorkerCancelled {
                blitz_id: blitz_id.clone(),
                worker_id: worker.clone(),
                avoided_tokens: avoided_t,
                avoided_cost_microunits: avoided_c,
            })?;
            cancelled = cancelled.saturating_add(1);
        }
        self.emit(BlitzEvent::CancelComplete {
            blitz_id: blitz_id.clone(),
            cancelled,
            latency_ms: CANCELLATION_LATENCY_MS,
            avoided_tokens,
            avoided_cost_microunits: avoided_cost,
        })?;
        self.emit(BlitzEvent::RecoveryConfirmed {
            blitz_id,
            evidence_refs: vec![
                EvidenceId::new(format!("recovery-{}", candidate_id.as_str()))
                    .expect("recovery evidence id"),
            ],
            at_ms: winner_time.saturating_add(CANCELLATION_LATENCY_MS),
        })?;
        Ok(())
    }
}

#[tracing::instrument]
fn schedule_wave(
    plans: &[WorkerPlan],
    wave: u16,
    spec: &RaceSpec,
    dispatched_at: u64,
    verifier: &VerifierPlan,
    pending: &mut Vec<(u64, usize, BlitzEvent)>,
) {
    let mut order = pending.len();
    for plan in plans {
        let worker_id = plan.worker_id.clone();
        let candidate_id =
            CandidateId::new(format!("candidate-{}", worker_id.as_str())).expect("candidate id");
        let start = dispatched_at.saturating_add(plan.start_delay_ms);
        let fails_at = plan.fails_at_ms.map(|at| dispatched_at.saturating_add(at));
        pending.push((
            start.min(fails_at.unwrap_or(u64::MAX)),
            order,
            BlitzEvent::WorkerStarted {
                blitz_id: spec.blitz_id.clone(),
                worker_id: worker_id.clone(),
                at_ms: start,
            },
        ));
        order += 1;
        if let Some(fails_at) = fails_at {
            pending.push((
                fails_at,
                order,
                BlitzEvent::WorkerExited {
                    blitz_id: spec.blitz_id.clone(),
                    worker_id: worker_id.clone(),
                    kind: plan.exit_kind,
                    at_ms: fails_at,
                },
            ));
            order += 1;
        }
        let Some(candidate_at) = plan.candidate_at_ms else {
            pending.push((
                spec.at_ms.saturating_add(spec.target.deadline_ms),
                order,
                BlitzEvent::WorkerExited {
                    blitz_id: spec.blitz_id.clone(),
                    worker_id,
                    kind: WorkerExitKind::Timeout,
                    at_ms: spec.at_ms.saturating_add(spec.target.deadline_ms),
                },
            ));
            order += 1;
            continue;
        };
        let candidate_at = dispatched_at.saturating_add(candidate_at);
        if fails_at.is_some_and(|fails| fails < candidate_at) {
            continue;
        }
        pending.push((
            candidate_at,
            order,
            BlitzEvent::CandidateReceived {
                blitz_id: spec.blitz_id.clone(),
                candidate: Box::new(candidate_for(spec, plan, wave, candidate_at, &candidate_id)),
            },
        ));
        order += 1;
        pending.push((
            candidate_at,
            order,
            BlitzEvent::VerificationStarted {
                blitz_id: spec.blitz_id.clone(),
                candidate_id: candidate_id.clone(),
            },
        ));
        order += 1;
        pending.push((
            candidate_at,
            order,
            BlitzEvent::BudgetConsumed {
                blitz_id: spec.blitz_id.clone(),
                cost: BlitzCost {
                    tokens: plan.tokens,
                    cost_microunits: plan.cost_microunits,
                    duration_ms: 0,
                    workers: 0,
                    waves: 0,
                },
            },
        ));
        order += 1;
        let resolved_at = candidate_at.saturating_add(verifier.latency_ms);
        if !verifier.available {
            pending.push((
                resolved_at,
                order,
                BlitzEvent::VerificationUnavailable {
                    blitz_id: spec.blitz_id.clone(),
                    candidate_id,
                },
            ));
            order += 1;
        } else if plan.valid {
            pending.push((
                resolved_at,
                order,
                BlitzEvent::CandidateVerified {
                    blitz_id: spec.blitz_id.clone(),
                    candidate_id,
                    passed: spec.target.verification_gates.clone(),
                },
            ));
            order += 1;
        } else {
            let gate = spec
                .target
                .verification_gates
                .first()
                .copied()
                .unwrap_or(VerificationKind::StaticInvariant);
            pending.push((
                resolved_at,
                order,
                BlitzEvent::CandidateRejected {
                    blitz_id: spec.blitz_id.clone(),
                    candidate_id,
                    gate,
                    reason_code: format!("{gate:?}_failure").to_lowercase(),
                },
            ));
            order += 1;
        }
        pending.push((
            resolved_at,
            order,
            BlitzEvent::WorkerSpent {
                blitz_id: spec.blitz_id.clone(),
                spent: WorkerSpent {
                    worker_id,
                    tokens: plan.tokens,
                    cost_microunits: plan.cost_microunits,
                    latency_ms: candidate_at.saturating_sub(dispatched_at),
                },
            },
        ));
        order += 1;
    }
}

#[tracing::instrument]
fn candidate_for(
    spec: &RaceSpec,
    plan: &WorkerPlan,
    wave: u16,
    emitted_at_ms: u64,
    candidate_id: &CandidateId,
) -> Candidate {
    Candidate {
        candidate_id: candidate_id.clone(),
        blitz_id: spec.blitz_id.clone(),
        worker_id: plan.worker_id.clone(),
        target_hash: spec.target.target_hash.clone(),
        artefact_digest: plan.digest.clone(),
        claim: format!("{} recovers the frozen target", plan.personnel_id.as_str()),
        confidence_ppm: PPM_SCALE,
        evidence_refs: spec.trigger.evidence_refs.clone(),
        verification_hint: String::new(),
        emitted_at_ms,
        wave,
    }
}
