//! The directive's deterministic simulation suite: BZ-T01 through BZ-T12.
//!
//! Every scenario runs the real race engine through the real fold. Nothing
//! is mocked at the rules level: if the engine schedules a refused event,
//! the race fails closed and the test fails.

use playbook_blitz::apply::{apply_blitz, BlitzEvent, BlitzView};
use playbook_blitz::domain::{
    BlitzClass, BlitzClosure, BlitzFailure, BlitzLedger, BlitzResourceMode, BlitzTrigger,
    DefensivePressure, Severity, TargetPackage, VerificationKind, WorkerExitKind,
};
use playbook_blitz::race::{run_race, RaceSpec, VerifierPlan, WorkerPlan, CANCELLATION_LATENCY_MS};
use playbook_blitz::roster::{select, WorkerCandidate};
use playbook_core::{BlitzBudget, BlitzId, EventId, PersonnelId, WorkerId};
use playbook_policy::{BlitzCorroboration, Policy};
use std::collections::BTreeSet;

fn policy() -> Policy {
    Policy::builtin()
}

fn view() -> BlitzView {
    BlitzView {
        active: true,
        remaining_tokens: 10_000_000,
        remaining_cost_microunits: 1_000_000_000,
    }
}

fn target() -> TargetPackage {
    TargetPackage {
        problem_statement: "restore the failing component".into(),
        evidence_refs: Vec::new(),
        artefact_refs: Vec::new(),
        constraints: Vec::new(),
        forbidden_actions: vec!["mutate production".into()],
        expected_output: "a verified patch".into(),
        win_condition: "compilation and unit tests pass".into(),
        verification_gates: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
        deadline_ms: 10_000,
        target_hash: String::new(),
    }
    .seal()
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
        originating_event: EventId::new("evt-bz").unwrap(),
    }
}

fn spec(budget: BlitzBudget) -> RaceSpec {
    RaceSpec {
        blitz_id: BlitzId::new("b-bz").unwrap(),
        trigger: trigger(),
        target: target(),
        pressure: pressure(),
        resource_mode: BlitzResourceMode::PowerCritical,
        corroboration: BlitzCorroboration::FirstPass,
        budget,
        class: BlitzClass::Instinctive,
        at_ms: 1_000,
    }
}

fn envelope() -> BlitzBudget {
    playbook_policy::Policy::builtin()
        .blitz
        .budget("power_critical")
}

fn plan(id: &str, provider: &str, family: &str, domains: &[&str]) -> WorkerPlan {
    WorkerPlan {
        worker_id: WorkerId::new(id).unwrap(),
        personnel_id: PersonnelId::new(id).unwrap(),
        provider: provider.into(),
        model_family: family.into(),
        failure_domains: domains.iter().map(|d| (*d).to_string()).collect(),
        start_delay_ms: 0,
        candidate_at_ms: Some(500),
        valid: true,
        tokens: 1_000,
        cost_microunits: 100_000,
        fails_at_ms: None,
        exit_kind: WorkerExitKind::Timeout,
        digest: format!("digest-{id}"),
    }
}

/// A two-slot envelope: two workers attack in wave 1 and reserves are held
/// back for the evidence-enriched second wave.
fn two_slot_envelope() -> BlitzBudget {
    BlitzBudget {
        max_tokens: 40_000,
        max_cost_microunits: 4_000_000,
        max_duration_ms: 120_000,
        max_workers: 2,
        max_waves: 3,
    }
}

/// BZ-T01 — fast winner: one worker returns a valid candidate substantially
/// earlier than peers; it is verified, accepted, and the rest cancelled.
#[test]
fn bz_t01_fast_winner_is_verified_accepted_and_the_race_cancelled() {
    let mut ledger = BlitzLedger::empty();
    let mut slow = plan("w-slow", "alpha", "family-1", &["dc-a"]);
    slow.candidate_at_ms = Some(3_000);
    let mut wrong = plan("w-wrong", "beta", "family-2", &["dc-b"]);
    wrong.valid = false;
    wrong.candidate_at_ms = Some(4_000);
    let events = run_race(
        &mut ledger,
        &spec(envelope()),
        &[plan("w-fast", "gamma", "family-3", &["dc-c"]), slow, wrong],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, BlitzEvent::WinnerSelected { .. })));
    assert!(ledger.active().is_empty());
    assert_eq!(ledger.recovered_total(), 1);
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, BlitzClosure::Recovered);
    let outcome = record.outcome.as_ref().unwrap();
    assert_eq!(outcome.raw_parallelism, 3);
    let cancellation = outcome
        .cancellation
        .clone()
        .expect("race cancelled its tail");
    assert_eq!(cancellation.cancelled, 2);
    assert_eq!(cancellation.latency_ms, CANCELLATION_LATENCY_MS);
    // The cancelled workers had not yet emitted, so their planned spend is
    // what the cancellation avoided.
    assert_eq!(cancellation.avoided_tokens, 2_000);
}

/// BZ-T02 — fast wrong answer: the earliest candidate fails verification and
/// the race continues until a slower verified candidate wins.
#[test]
fn bz_t02_fast_wrong_answer_loses_and_the_race_continues() {
    let mut ledger = BlitzLedger::empty();
    let mut fast = plan("w-fast", "alpha", "family-1", &["dc-a"]);
    fast.valid = false;
    fast.candidate_at_ms = Some(100);
    let mut slow = plan("w-slow", "beta", "family-2", &["dc-b"]);
    slow.candidate_at_ms = Some(2_000);
    let events = run_race(
        &mut ledger,
        &spec(envelope()),
        &[fast, slow],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    let winner = events.iter().find_map(|event| match event {
        BlitzEvent::WinnerSelected {
            candidate_id,
            worker_id,
            ..
        } => Some((candidate_id.as_str(), worker_id.as_str())),
        _ => None,
    });
    assert_eq!(winner, Some(("candidate-w-slow", "w-slow")));
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    // The first response is recorded, but it never won.
    assert_eq!(outcome.first_candidate_ms, Some(100));
    assert_eq!(outcome.time_to_recovery_ms, Some(2_012));
}

/// BZ-T03 — slow correct answer: several invalid candidates precede the
/// successful one, and each rejection keeps the race alive.
#[test]
fn bz_t03_invalid_candidates_precede_the_accepted_candidate() {
    let mut ledger = BlitzLedger::empty();
    let mut wrong_a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    wrong_a.valid = false;
    wrong_a.candidate_at_ms = Some(100);
    let mut wrong_b = plan("w-b", "beta", "family-2", &["dc-b"]);
    wrong_b.valid = false;
    wrong_b.candidate_at_ms = Some(200);
    let mut wrong_c = plan("w-c", "gamma", "family-3", &["dc-c"]);
    wrong_c.valid = false;
    wrong_c.candidate_at_ms = Some(300);
    let mut slow = plan("w-d", "delta", "family-4", &["dc-d"]);
    slow.candidate_at_ms = Some(2_000);
    run_race(
        &mut ledger,
        &spec(envelope()),
        &[wrong_a, wrong_b, wrong_c, slow],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, BlitzClosure::Recovered);
    let outcome = record.outcome.as_ref().unwrap();
    assert_eq!(outcome.first_candidate_ms, Some(100));
    assert_eq!(
        outcome.winner.as_ref().unwrap().worker_id,
        WorkerId::new("w-d").unwrap()
    );
}

/// BZ-T04 — provider outage: one provider dies mid-race, the formation
/// continues, and recovery still lands.
#[test]
fn bz_t04_provider_outage_does_not_invalidate_the_formation() {
    let mut ledger = BlitzLedger::empty();
    let mut dying = plan("w-dies", "flaky-provider", "family-1", &["dc-outage"]);
    dying.candidate_at_ms = Some(3_000);
    dying.fails_at_ms = Some(200);
    dying.exit_kind = WorkerExitKind::ProviderFailure;
    let mut healthy = plan("w-healthy", "solid-provider", "family-2", &["dc-b"]);
    healthy.candidate_at_ms = Some(1_000);
    run_race(
        &mut ledger,
        &spec(envelope()),
        &[dying, healthy],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Recovered);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    // The dead worker is in the scouting evidence with no verified candidate.
    let dead = outcome
        .worker_results
        .iter()
        .find(|row| row.provider == "flaky-provider")
        .unwrap();
    assert!(!dead.candidate_verified);
    assert_eq!(
        outcome.winner.as_ref().unwrap().worker_id,
        WorkerId::new("w-healthy").unwrap()
    );
}

/// BZ-T05 — budget exhaustion: no solution before the ceiling; the governor
/// terminates cleanly, no unauthorised overspend occurs, and the escalation
/// policy executes.
#[test]
fn bz_t05_budget_exhaustion_terminates_and_escalates() {
    let mut ledger = BlitzLedger::empty();
    let mut hungry = plan("w-a", "alpha", "family-1", &["dc-a"]);
    hungry.valid = false;
    hungry.tokens = 20_000;
    hungry.cost_microunits = 2_000_000;
    let mut reserve = plan("w-b", "beta", "family-2", &["dc-b"]);
    reserve.valid = false;
    reserve.candidate_at_ms = Some(6_000);
    let tight = BlitzBudget {
        max_tokens: 10_000,
        max_cost_microunits: 1_000_000,
        max_duration_ms: 120_000,
        max_workers: 1,
        max_waves: 1,
    };
    let events = run_race(
        &mut ledger,
        &spec(tight),
        &[hungry, reserve],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    // The governor terminated the race and the escalation policy executed.
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Escalated);
    assert_eq!(ledger.exhausted_total(), 1);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.failure, Some(BlitzFailure::BudgetExhausted));
    // The second worker was never dispatched: the envelope held.
    assert!(!events.iter().any(
        |event| matches!(event, BlitzEvent::WorkerDispatched { worker, .. } if worker.worker_id == WorkerId::new("w-b").unwrap())
    ));
    // The escalation policy executed: a huddle recommendation is on the log.
    assert!(events
        .iter()
        .any(|event| matches!(event, BlitzEvent::Escalated { .. })));
    assert_eq!(ledger.escalated_total(), 1);
}

/// BZ-T06 — deadline exhaustion: no solution before the deadline; the blitz
/// transitions to EXHAUSTED and escalates.
#[test]
fn bz_t06_deadline_exhaustion_transitions_and_escalates() {
    let mut ledger = BlitzLedger::empty();
    let mut late_a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    late_a.candidate_at_ms = Some(50_000);
    let mut late_b = plan("w-b", "beta", "family-2", &["dc-b"]);
    late_b.candidate_at_ms = Some(60_000);
    let events = run_race(
        &mut ledger,
        &spec(envelope()),
        &[late_a, late_b],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Escalated);
    assert_eq!(ledger.exhausted_total(), 1);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.failure, Some(BlitzFailure::DeadlineExhausted));
    assert!(outcome.winner.is_none());
    assert!(events
        .iter()
        .any(|event| matches!(event, BlitzEvent::Escalated { .. })));
}

/// BZ-T07 — correlated failure: several workers converge on the same invalid
/// fix; the correlation is recorded, effective parallelism collapses, and a
/// reserve worker takes over as the evidence-enriched second wave.
#[test]
fn bz_t07_correlated_failure_reduces_effective_parallelism() {
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-shared"]);
    a.valid = false;
    a.candidate_at_ms = Some(100);
    a.digest = "same-fix".into();
    let mut b = plan("w-b", "beta", "family-2", &["dc-shared"]);
    b.valid = false;
    b.candidate_at_ms = Some(200);
    b.digest = "same-fix".into();
    let mut reserve = plan("w-c", "gamma", "family-3", &["dc-c"]);
    reserve.candidate_at_ms = Some(500);
    let events = run_race(
        &mut ledger,
        &spec(two_slot_envelope()),
        &[a, b, reserve],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.correlated_failures, 1);
    assert_eq!(ledger.correlated_total(), 1);
    // Two raw workers hunting identical ground bought less than two
    // independent attempts.
    assert!(
        outcome.effective_parallelism_milli < u32::from(outcome.raw_parallelism) * 1_000,
        "effective {} must be below raw {}",
        outcome.effective_parallelism_milli,
        outcome.raw_parallelism
    );
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Recovered);
    // Wave 1 closed with its failure diagnostics; the reserve attacked as
    // the evidence-enriched wave 2 and won.
    assert!(events
        .iter()
        .any(|event| matches!(event, BlitzEvent::WaveClosed { wave: 1, .. })));
    assert!(events.iter().any(
        |event| matches!(event, BlitzEvent::WorkerDispatched { worker, .. } if worker.wave == 2)
    ));
    assert_eq!(
        outcome.winner.as_ref().unwrap().worker_id,
        WorkerId::new("w-c").unwrap()
    );
}

/// BZ-T08 — cancellation race: candidates pass while others are in flight;
/// exactly one authoritative winning transition occurs and no double commit
/// is possible.
#[test]
fn bz_t08_exactly_one_winner_survives_the_cancellation_race() {
    let mut ledger = BlitzLedger::empty();
    let mut first = plan("w-a", "alpha", "family-1", &["dc-a"]);
    first.candidate_at_ms = Some(100);
    let mut second = plan("w-b", "beta", "family-2", &["dc-b"]);
    second.candidate_at_ms = Some(120);
    let mut third = plan("w-c", "gamma", "family-3", &["dc-c"]);
    third.candidate_at_ms = Some(140);
    run_race(
        &mut ledger,
        &spec(envelope()),
        &[first, second, third],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, BlitzClosure::Recovered);
    let outcome = record.outcome.as_ref().unwrap();
    assert_eq!(
        outcome.winner.as_ref().unwrap().worker_id,
        WorkerId::new("w-a").unwrap()
    );
    // Both later candidates cancelled before the fold ever saw them: no
    // second verified candidate, no double commit.
    assert!(outcome.worker_results.iter().all(|row| {
        row.personnel_id == PersonnelId::new("w-a").unwrap() || !row.candidate_verified
    }));
    // And the fold refuses a second winner even when asked directly.
    let error = apply_blitz(
        &mut ledger,
        &BlitzEvent::WinnerSelected {
            blitz_id: BlitzId::new("b-bz").unwrap(),
            candidate_id: playbook_core::CandidateId::new("candidate-w-b").unwrap(),
            worker_id: WorkerId::new("w-b").unwrap(),
            at_ms: 200,
        },
        &view(),
        &policy(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        playbook_blitz::refusal("unknown blitz"),
        "the blitz is closed; nothing can commit to it"
    );
}

/// BZ-T09 — verifier failure: the verifier becomes unavailable; candidates
/// remain untrusted, the race fails closed, nothing is committed.
#[test]
fn bz_t09_verifier_failure_fails_closed() {
    let mut ledger = BlitzLedger::empty();
    let events = run_race(
        &mut ledger,
        &spec(envelope()),
        &[
            plan("w-a", "alpha", "family-1", &["dc-a"]),
            plan("w-b", "beta", "family-2", &["dc-b"]),
        ],
        &VerifierPlan {
            latency_ms: 10,
            available: false,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert!(ledger.active().is_empty());
    assert_eq!(ledger.exhausted_total(), 1);
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Escalated);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.failure, Some(BlitzFailure::VerificationFailure));
    assert!(outcome.winner.is_none());
    assert!(!events
        .iter()
        .any(|event| matches!(event, BlitzEvent::WinnerSelected { .. })));
    // The candidates themselves arrived but were never trusted.
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, BlitzEvent::VerificationUnavailable { .. }))
            .count(),
        2
    );
}

/// BZ-T10 — target mutation: material incident facts change mid-race; the
/// original blitz is superseded and a successor targets the revised problem.
/// Every candidate remains attributable to the hash it actually attempted.
#[test]
fn bz_t10_target_mutation_supersedes_without_semantic_drift() {
    let mut ledger = BlitzLedger::empty();
    let first_hash = target().target_hash;
    let id = BlitzId::new("b-bz").unwrap();
    // Open B1 by hand so it is live mid-race when the incident mutates.
    fold_open(&mut ledger, &id, &target());
    apply_blitz(
        &mut ledger,
        &BlitzEvent::WorkerDispatched {
            blitz_id: id.clone(),
            worker: playbook_blitz::domain::WorkerSlot {
                worker_id: WorkerId::new("w-a").unwrap(),
                personnel_id: PersonnelId::new("w-a").unwrap(),
                provider: "alpha".into(),
                model_family: "family-1".into(),
                failure_domains: vec!["dc-a".into()],
                wave: 1,
                dispatched_at_ms: 1_000,
                deadline_ms: 11_000,
            },
            effective_parallelism_milli: 1_000,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    // The incident changes materially mid-race: B1 is invalidated.
    apply_blitz(
        &mut ledger,
        &BlitzEvent::Superseded {
            blitz_id: id.clone(),
            successor_id: BlitzId::new("b-bz-2").unwrap(),
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Superseded);
    assert_eq!(ledger.closed()[0].target_hash, first_hash);

    // The successor targets the revised problem.
    let mut revised = target();
    revised.problem_statement = "restore the component on the rotated credentials".into();
    let revised = revised.seal().unwrap();
    let mut spec2 = spec(envelope());
    spec2.blitz_id = BlitzId::new("b-bz-2").unwrap();
    spec2.target = revised.clone();
    let events = run_race(
        &mut ledger,
        &spec2,
        &[plan("w-b", "beta", "family-2", &["dc-b"])],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    assert_eq!(ledger.closed()[1].closure, BlitzClosure::Recovered);
    assert_ne!(
        ledger.closed()[0].target_hash,
        ledger.closed()[1].target_hash
    );
    // B2's candidates are attributable to the revised hash only.
    assert!(events.iter().all(|event| match event {
        BlitzEvent::CandidateReceived { candidate, .. } => {
            candidate.target_hash == revised.target_hash
        }
        _ => true,
    }));
}

/// BZ-T10 helper: propose, authorise, and freeze a blitz by hand.
fn fold_open(ledger: &mut BlitzLedger, id: &BlitzId, target: &TargetPackage) {
    for event in [
        BlitzEvent::Proposed {
            blitz_id: id.clone(),
            trigger: trigger(),
            target: Box::new(target.clone()),
            pressure: pressure(),
            at_ms: 1_000,
        },
        BlitzEvent::Authorised {
            blitz_id: id.clone(),
            budget: envelope(),
            corroboration: BlitzCorroboration::FirstPass,
            resource_mode: BlitzResourceMode::PowerCritical,
            at_ms: 1_000,
        },
        BlitzEvent::TargetFrozen {
            blitz_id: id.clone(),
            target_hash: target.target_hash.clone(),
        },
    ] {
        apply_blitz(ledger, &event, &view(), &policy()).unwrap();
    }
}

/// BZ-T11 — power critical: local energy is scarce, so the formation prefers
/// remote providers and the tighter envelope.
#[test]
fn bz_t11_power_critical_prefers_remote_workers() {
    let policy = policy();
    let weights = policy.blitz.roster_weights;
    let diversity = policy.blitz.diversity;
    let candidate = |id: &str, provider: &str, local: bool, latency: u64| WorkerCandidate {
        personnel_id: PersonnelId::new(id).unwrap(),
        worker_id: WorkerId::new(id).unwrap(),
        provider: provider.into(),
        model_family: format!("family-{id}"),
        failure_domains: BTreeSet::from([format!("dc-{id}").to_string()]),
        estimated_latency_ms: latency,
        success_probability_ppm: 900_000,
        availability_ppm: 900_000,
        estimated_cost_microunits: 100_000,
        local,
    };
    let local_worker = candidate("w-local", "localhost", true, 400);
    let remote_worker = candidate("w-remote", "remote-provider", false, 400);
    let picked = select(
        &[local_worker, remote_worker],
        1,
        policy.blitz.min_worker_score,
        &diversity,
        &weights,
        BlitzResourceMode::PowerCritical,
    );
    assert_eq!(picked.len(), 1);
    assert!(
        !picked[0].local,
        "power-critical formations must prefer remote compute"
    );
    // The envelope is tighter under power scarcity than the normal mode.
    let power = policy.blitz.budget("power_critical");
    let normal = policy.blitz.budget("normal");
    assert!(power.max_tokens < normal.max_tokens);
    assert!(power.max_workers <= normal.max_workers);
}

/// BZ-T12 — network critical: remote connectivity deteriorates, so locally
/// viable workers receive the preference instead.
#[test]
fn bz_t12_network_critical_prefers_local_workers() {
    let policy = policy();
    let weights = policy.blitz.roster_weights;
    let diversity = policy.blitz.diversity;
    let candidate = |id: &str, provider: &str, local: bool, latency: u64| WorkerCandidate {
        personnel_id: PersonnelId::new(id).unwrap(),
        worker_id: WorkerId::new(id).unwrap(),
        provider: provider.into(),
        model_family: format!("family-{id}"),
        failure_domains: BTreeSet::from([format!("dc-{id}").to_string()]),
        estimated_latency_ms: latency,
        success_probability_ppm: 900_000,
        availability_ppm: 900_000,
        estimated_cost_microunits: 100_000,
        local,
    };
    let local_worker = candidate("w-local", "localhost", true, 400);
    let remote_worker = candidate("w-remote", "remote-provider", false, 400);
    let picked = select(
        &[local_worker, remote_worker],
        1,
        policy.blitz.min_worker_score,
        &diversity,
        &weights,
        BlitzResourceMode::NetworkCritical,
    );
    assert_eq!(picked.len(), 1);
    assert!(
        picked[0].local,
        "network-critical formations must prefer local compute"
    );
}

/// Streaming: verification begins before the race completes, not after it.
#[test]
fn verification_races_the_continuing_inference() {
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    a.valid = false;
    a.candidate_at_ms = Some(100);
    let mut b = plan("w-b", "beta", "family-2", &["dc-b"]);
    b.candidate_at_ms = Some(2_000);
    let events = run_race(
        &mut ledger,
        &spec(envelope()),
        &[a, b],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    // w-b was still running when w-a's verification started.
    let verification_started = events
        .iter()
        .position(|event| matches!(event, BlitzEvent::VerificationStarted { .. }))
        .expect("verification started");
    let still_running = events[..verification_started]
        .iter()
        .any(|event| matches!(event, BlitzEvent::WorkerStarted { worker_id, .. } if worker_id == &WorkerId::new("w-b").unwrap()));
    assert!(still_running);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, BlitzEvent::CandidateRejected { .. }))
            .count(),
        1
    );
}

/// The exhausted state machine is observable: the blitz sits in EXHAUSTED
/// until the escalation closes it — the fold refuses ESCALATED from anything
/// else, so the engine cannot skip the state.
#[test]
fn exhausted_phase_is_observable_before_the_escalation_closes() {
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    a.valid = false;
    a.candidate_at_ms = Some(100);
    let mut b = plan("w-b", "beta", "family-2", &["dc-b"]);
    b.valid = false;
    b.candidate_at_ms = Some(200);
    run_race(
        &mut ledger,
        &spec(envelope()),
        &[a, b],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy(),
    )
    .unwrap();
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, BlitzClosure::Escalated);
    assert_eq!(ledger.active_blitz().map(|blitz| blitz.phase), None);
    assert_eq!(ledger.exhausted_total(), 1);
    assert_eq!(ledger.escalated_total(), 1);
}

/// The default corroboration mode binds the fold: the policy's floor is the
/// fold's floor, not the engine's.
#[test]
fn corroboration_floor_is_policy_bound_not_engine_bound() {
    let mut policy = policy();
    policy.blitz.corroboration = BlitzCorroboration::TwoIndependentPasses;
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    a.candidate_at_ms = Some(100);
    let mut b = plan("w-b", "beta", "family-2", &["dc-b"]);
    b.candidate_at_ms = Some(2_000);
    let mut spec = spec(envelope());
    spec.corroboration = policy.blitz.corroboration;
    let events = run_race(
        &mut ledger,
        &spec,
        &[a, b],
        &VerifierPlan {
            latency_ms: 10,
            available: true,
        },
        &view(),
        &policy,
    )
    .unwrap();
    // One verified pass is not two: the race must have kept running until
    // the second candidate verified.
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(
        outcome.winner.as_ref().unwrap().worker_id,
        WorkerId::new("w-b").unwrap()
    );
    assert_eq!(outcome.first_candidate_ms, Some(100));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, BlitzEvent::CandidateVerified { .. }))
            .count(),
        2
    );
}
