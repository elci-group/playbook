//! Scenario fixtures for the blitz subsystem.
//!
//! The transcripts under `fixtures/blitz/` are full drive logs built through
//! the real race engine and the real drive fold, so every event in them is
//! one the fold accepted. Regenerate with:
//!
//! ```text
//! cargo test -p playbook-blitz --offline --test fixtures -- --ignored regenerate_blitz_fixtures
//! ```
//!
//! The checked-in test folds each fixture and asserts its scenario marker,
//! which is what keeps the fixtures honest.

use playbook_blitz::apply::{apply_blitz, BlitzEvent, BlitzView};
use playbook_blitz::domain::{
    BlitzClass, BlitzClosure, BlitzFailure, BlitzLedger, BlitzResourceMode, BlitzTrigger,
    DefensivePressure, Severity, TargetPackage, VerificationKind, WorkerExitKind,
};
use playbook_blitz::race::{run_race, RaceSpec, VerifierPlan, WorkerPlan};
use playbook_blitz::roster::{select, WorkerCandidate};
use playbook_core::{
    BlitzId, DriveBudget, DriveId, EventId, PersonnelId, TaskContract, TaskIntegrity, WorkerId,
};
use playbook_events::DriveEvent;
use playbook_policy::{BlitzCorroboration, Policy};
use playbook_state::{fold, ApplyMode};
use std::collections::BTreeSet;

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
        originating_event: EventId::new("evt-fixture").unwrap(),
    }
}

fn spec(id: &str) -> RaceSpec {
    RaceSpec {
        blitz_id: BlitzId::new(id).unwrap(),
        trigger: trigger(),
        target: target(),
        pressure: pressure(),
        resource_mode: BlitzResourceMode::PowerCritical,
        corroboration: BlitzCorroboration::FirstPass,
        budget: policy().blitz.budget("power_critical"),
        class: BlitzClass::Instinctive,
        at_ms: 1_000,
    }
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

fn verifier() -> VerifierPlan {
    VerifierPlan {
        latency_ms: 10,
        available: true,
    }
}

fn drive_events(blitz_events: &[BlitzEvent]) -> Vec<DriveEvent> {
    let mut events = vec![DriveEvent::DriveStarted {
        drive_id: DriveId::new("drive-blitz-fixture").unwrap(),
        task: TaskContract {
            statement: "recover under defensive pressure".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        },
        budget: DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            wall_clock_ms: 600_000,
            tool_calls: 1_000,
            inference_calls: 1_000,
            retry_count: 100,
            context_capacity: 1_000_000,
            provider_quota: 10_000,
        },
        at_ms: 0,
    }];
    events.extend(blitz_events.iter().map(to_drive_event));
    events
}

/// The inverse of the drive fold's blitz projection: the engine's events
/// become journal events one-to-one.
fn to_drive_event(event: &BlitzEvent) -> DriveEvent {
    match event {
        BlitzEvent::Proposed {
            blitz_id,
            trigger,
            target,
            pressure,
            at_ms,
        } => DriveEvent::BlitzProposed {
            blitz_id: blitz_id.clone(),
            trigger: trigger.clone(),
            target: (**target).clone(),
            pressure: pressure.clone(),
            at_ms: *at_ms,
        },
        BlitzEvent::Authorised {
            blitz_id,
            budget,
            corroboration,
            resource_mode,
            at_ms,
        } => DriveEvent::BlitzAuthorised {
            blitz_id: blitz_id.clone(),
            budget: *budget,
            corroboration: *corroboration,
            resource_mode: *resource_mode,
            at_ms: *at_ms,
        },
        BlitzEvent::Rejected {
            blitz_id,
            reason_code,
        } => DriveEvent::BlitzRejected {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
        BlitzEvent::TargetFrozen {
            blitz_id,
            target_hash,
        } => DriveEvent::BlitzTargetFrozen {
            blitz_id: blitz_id.clone(),
            target_hash: target_hash.clone(),
        },
        BlitzEvent::WorkerDispatched {
            blitz_id,
            worker,
            effective_parallelism_milli,
        } => DriveEvent::BlitzWorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker.clone(),
            effective_parallelism_milli: *effective_parallelism_milli,
        },
        BlitzEvent::WorkerStarted {
            blitz_id,
            worker_id,
            at_ms,
        } => DriveEvent::BlitzWorkerStarted {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            at_ms: *at_ms,
        },
        BlitzEvent::WorkerExited {
            blitz_id,
            worker_id,
            kind,
            at_ms,
        } => DriveEvent::BlitzWorkerExited {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            kind: *kind,
            at_ms: *at_ms,
        },
        BlitzEvent::WorkerSpent { blitz_id, spent } => DriveEvent::BlitzWorkerSpent {
            blitz_id: blitz_id.clone(),
            spent: spent.clone(),
        },
        BlitzEvent::WaveClosed {
            blitz_id,
            wave,
            diagnostics,
        } => DriveEvent::BlitzWaveClosed {
            blitz_id: blitz_id.clone(),
            wave: *wave,
            diagnostics: diagnostics.clone(),
        },
        BlitzEvent::CandidateReceived {
            blitz_id,
            candidate,
        } => DriveEvent::BlitzCandidateReceived {
            blitz_id: blitz_id.clone(),
            candidate: candidate.clone(),
        },
        BlitzEvent::VerificationStarted {
            blitz_id,
            candidate_id,
        } => DriveEvent::BlitzVerificationStarted {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
        },
        BlitzEvent::CandidateRejected {
            blitz_id,
            candidate_id,
            gate,
            reason_code,
        } => DriveEvent::BlitzCandidateRejected {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            gate: *gate,
            reason_code: reason_code.clone(),
        },
        BlitzEvent::CandidateVerified {
            blitz_id,
            candidate_id,
            passed,
        } => DriveEvent::BlitzCandidateVerified {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            passed: passed.clone(),
        },
        BlitzEvent::VerificationUnavailable {
            blitz_id,
            candidate_id,
        } => DriveEvent::BlitzVerificationUnavailable {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
        },
        BlitzEvent::HumanConfirmed {
            blitz_id,
            candidate_id,
            authority,
        } => DriveEvent::BlitzHumanConfirmed {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            authority: authority.clone(),
        },
        BlitzEvent::WinnerSelected {
            blitz_id,
            candidate_id,
            worker_id,
            at_ms,
        } => DriveEvent::BlitzWinnerSelected {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            worker_id: worker_id.clone(),
            at_ms: *at_ms,
        },
        BlitzEvent::CancelStarted { blitz_id, at_ms } => DriveEvent::BlitzCancelStarted {
            blitz_id: blitz_id.clone(),
            at_ms: *at_ms,
        },
        BlitzEvent::WorkerCancelled {
            blitz_id,
            worker_id,
            avoided_tokens,
            avoided_cost_microunits,
        } => DriveEvent::BlitzWorkerCancelled {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            avoided_tokens: *avoided_tokens,
            avoided_cost_microunits: *avoided_cost_microunits,
        },
        BlitzEvent::CancelComplete {
            blitz_id,
            cancelled,
            latency_ms,
            avoided_tokens,
            avoided_cost_microunits,
        } => DriveEvent::BlitzCancelComplete {
            blitz_id: blitz_id.clone(),
            cancelled: *cancelled,
            latency_ms: *latency_ms,
            avoided_tokens: *avoided_tokens,
            avoided_cost_microunits: *avoided_cost_microunits,
        },
        BlitzEvent::RecoveryConfirmed {
            blitz_id,
            evidence_refs,
            at_ms,
        } => DriveEvent::BlitzRecoveryConfirmed {
            blitz_id: blitz_id.clone(),
            evidence_refs: evidence_refs.clone(),
            at_ms: *at_ms,
        },
        BlitzEvent::BudgetConsumed { blitz_id, cost } => DriveEvent::BlitzBudgetConsumed {
            blitz_id: blitz_id.clone(),
            cost: *cost,
        },
        BlitzEvent::Exhausted { blitz_id, failure } => DriveEvent::BlitzExhausted {
            blitz_id: blitz_id.clone(),
            failure: *failure,
        },
        BlitzEvent::Escalated {
            blitz_id,
            recommendation,
        } => DriveEvent::BlitzEscalated {
            blitz_id: blitz_id.clone(),
            recommendation: recommendation.clone(),
        },
        BlitzEvent::Superseded {
            blitz_id,
            successor_id,
        } => DriveEvent::BlitzSuperseded {
            blitz_id: blitz_id.clone(),
            successor_id: successor_id.clone(),
        },
        BlitzEvent::Failed {
            blitz_id,
            reason_code,
        } => DriveEvent::BlitzFailed {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
        BlitzEvent::Aborted {
            blitz_id,
            reason_code,
        } => DriveEvent::BlitzAborted {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
    }
}

fn fast_winner() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut slow = plan("w-slow", "alpha", "family-1", &["dc-a"]);
    slow.candidate_at_ms = Some(3_000);
    let mut wrong = plan("w-wrong", "beta", "family-2", &["dc-b"]);
    wrong.valid = false;
    wrong.candidate_at_ms = Some(4_000);
    run_race(
        &mut ledger,
        &spec("b-1"),
        &[plan("w-fast", "gamma", "family-3", &["dc-c"]), slow, wrong],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
    .into_iter()
    .collect()
}

fn fast_wrong_answer() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut fast = plan("w-fast", "alpha", "family-1", &["dc-a"]);
    fast.valid = false;
    fast.candidate_at_ms = Some(100);
    let mut slow = plan("w-slow", "beta", "family-2", &["dc-b"]);
    slow.candidate_at_ms = Some(2_000);
    run_race(
        &mut ledger,
        &spec("b-1"),
        &[fast, slow],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn slow_correct() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-a"]);
    a.valid = false;
    a.candidate_at_ms = Some(100);
    let mut b = plan("w-b", "beta", "family-2", &["dc-b"]);
    b.valid = false;
    b.candidate_at_ms = Some(200);
    let mut c = plan("w-c", "gamma", "family-3", &["dc-c"]);
    c.valid = false;
    c.candidate_at_ms = Some(300);
    let mut d = plan("w-d", "delta", "family-4", &["dc-d"]);
    d.candidate_at_ms = Some(2_000);
    run_race(
        &mut ledger,
        &spec("b-1"),
        &[a, b, c, d],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn provider_outage() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut dying = plan("w-dies", "flaky-provider", "family-1", &["dc-outage"]);
    dying.candidate_at_ms = Some(3_000);
    dying.fails_at_ms = Some(200);
    dying.exit_kind = WorkerExitKind::ProviderFailure;
    let mut healthy = plan("w-healthy", "solid-provider", "family-2", &["dc-b"]);
    healthy.candidate_at_ms = Some(1_000);
    run_race(
        &mut ledger,
        &spec("b-1"),
        &[dying, healthy],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn budget_exhausted() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut hungry = plan("w-hungry", "alpha", "family-1", &["dc-a"]);
    hungry.valid = false;
    hungry.tokens = 20_000;
    hungry.cost_microunits = 2_000_000;
    let reserve = plan("w-reserve", "beta", "family-2", &["dc-b"]);
    let mut tight = spec("b-1");
    tight.budget = playbook_core::BlitzBudget {
        max_tokens: 10_000,
        max_cost_microunits: 1_000_000,
        max_duration_ms: 120_000,
        max_workers: 1,
        max_waves: 1,
    };
    run_race(
        &mut ledger,
        &tight,
        &[hungry, reserve],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn deadline_exhausted() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut late_a = plan("w-late-a", "alpha", "family-1", &["dc-a"]);
    late_a.candidate_at_ms = Some(50_000);
    let mut late_b = plan("w-late-b", "beta", "family-2", &["dc-b"]);
    late_b.candidate_at_ms = Some(60_000);
    run_race(
        &mut ledger,
        &spec("b-1"),
        &[late_a, late_b],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn correlated_failure() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let mut a = plan("w-a", "alpha", "family-1", &["dc-shared"]);
    a.valid = false;
    a.candidate_at_ms = Some(100);
    a.digest = "same-fix".into();
    let mut b = plan("w-b", "beta", "family-2", &["dc-shared"]);
    b.valid = false;
    b.candidate_at_ms = Some(200);
    b.digest = "same-fix".into();
    let reserve = plan("w-reserve", "gamma", "family-3", &["dc-c"]);
    let mut two_slot = spec("b-1");
    two_slot.budget = playbook_core::BlitzBudget {
        max_tokens: 40_000,
        max_cost_microunits: 4_000_000,
        max_duration_ms: 120_000,
        max_workers: 2,
        max_waves: 3,
    };
    run_race(
        &mut ledger,
        &two_slot,
        &[a, b, reserve],
        &verifier(),
        &view(),
        &policy(),
    )
    .unwrap()
}

fn verifier_fail_closed() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    run_race(
        &mut ledger,
        &spec("b-1"),
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
    .unwrap()
}

fn target_drift() -> Vec<BlitzEvent> {
    let mut ledger = BlitzLedger::empty();
    let id = BlitzId::new("b-1").unwrap();
    let mut events: Vec<BlitzEvent> = [
        BlitzEvent::Proposed {
            blitz_id: id.clone(),
            trigger: trigger(),
            target: Box::new(target()),
            pressure: pressure(),
            at_ms: 1_000,
        },
        BlitzEvent::Authorised {
            blitz_id: id.clone(),
            budget: policy().blitz.budget("power_critical"),
            corroboration: BlitzCorroboration::FirstPass,
            resource_mode: BlitzResourceMode::PowerCritical,
            at_ms: 1_000,
        },
        BlitzEvent::TargetFrozen {
            blitz_id: id.clone(),
            target_hash: target().target_hash,
        },
        BlitzEvent::Superseded {
            blitz_id: id.clone(),
            successor_id: BlitzId::new("b-2").unwrap(),
        },
    ]
    .into_iter()
    .collect();
    for event in &events {
        apply_blitz(&mut ledger, event, &view(), &policy()).unwrap();
    }
    let mut revised = target();
    revised.problem_statement = "restore the component on the rotated credentials".into();
    let mut spec2 = spec("b-2");
    spec2.target = revised.seal().unwrap();
    events.extend(
        run_race(
            &mut ledger,
            &spec2,
            &[plan("w-b", "beta", "family-2", &["dc-b"])],
            &verifier(),
            &view(),
            &policy(),
        )
        .unwrap(),
    );
    events
}

/// The resource-mode fixtures: the front-office offers a mixed local/remote
/// roster, the mode selects the formation, and the race runs with it.
fn resource_mode_formation(mode: BlitzResourceMode) -> (Vec<WorkerCandidate>, Vec<BlitzEvent>) {
    let blitz_policy = policy();
    let candidate = |id: &str, provider: &str, local: bool| WorkerCandidate {
        personnel_id: PersonnelId::new(id).unwrap(),
        worker_id: WorkerId::new(id).unwrap(),
        provider: provider.into(),
        model_family: format!("family-{id}"),
        failure_domains: BTreeSet::from([format!("dc-{id}").to_string()]),
        estimated_latency_ms: 400,
        success_probability_ppm: 900_000,
        availability_ppm: 900_000,
        estimated_cost_microunits: 100_000,
        local,
    };
    let roster = vec![
        candidate("w-local-a", "localhost-a", true),
        candidate("w-local-b", "localhost-b", true),
        candidate("w-remote-a", "remote-a", false),
        candidate("w-remote-b", "remote-b", false),
    ];
    let picked = select(
        &roster,
        2,
        blitz_policy.blitz.min_worker_score,
        &blitz_policy.blitz.diversity,
        &blitz_policy.blitz.roster_weights,
        mode,
    );
    assert_eq!(picked.len(), 2);
    let plans: Vec<WorkerPlan> = picked
        .iter()
        .map(|worker| {
            plan(
                worker.worker_id.as_str(),
                &worker.provider,
                &worker.model_family,
                &[],
            )
        })
        .collect();
    let mut ledger = BlitzLedger::empty();
    let mut race = spec("b-1");
    race.resource_mode = mode;
    let events = run_race(&mut ledger, &race, &plans, &verifier(), &view(), &policy()).unwrap();
    (picked, events)
}

fn power_critical() -> (Vec<WorkerCandidate>, Vec<BlitzEvent>) {
    resource_mode_formation(BlitzResourceMode::PowerCritical)
}

fn network_critical() -> (Vec<WorkerCandidate>, Vec<BlitzEvent>) {
    resource_mode_formation(BlitzResourceMode::NetworkCritical)
}

fn write_fixture(name: &str, events: &[DriveEvent]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/blitz");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(name), serde_json::to_string(events).unwrap()).unwrap();
}

fn load_fixture(name: &str) -> Vec<DriveEvent> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../fixtures/blitz/{name}"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn fixture_events() -> Vec<(&'static str, Vec<DriveEvent>)> {
    vec![
        ("fast_winner.json", drive_events(&fast_winner())),
        ("fast_wrong_answer.json", drive_events(&fast_wrong_answer())),
        ("slow_correct.json", drive_events(&slow_correct())),
        ("provider_outage.json", drive_events(&provider_outage())),
        ("budget_exhausted.json", drive_events(&budget_exhausted())),
        (
            "deadline_exhausted.json",
            drive_events(&deadline_exhausted()),
        ),
        (
            "correlated_failure.json",
            drive_events(&correlated_failure()),
        ),
        (
            "verifier_fail_closed.json",
            drive_events(&verifier_fail_closed()),
        ),
        ("target_drift.json", drive_events(&target_drift())),
        ("power_critical.json", drive_events(&power_critical().1)),
        ("network_critical.json", drive_events(&network_critical().1)),
    ]
}

#[test]
#[ignore]
fn regenerate_blitz_fixtures() {
    for (name, events) in fixture_events() {
        write_fixture(name, &events);
    }
}

#[test]
fn every_blitz_fixture_folds_and_replays() {
    let policy = Policy::builtin();
    for (name, expected) in fixture_events() {
        let events = load_fixture(name);
        assert_eq!(events, expected, "{name} drifted from the generator");
        let state = fold(&events, &policy, ApplyMode::Audit)
            .unwrap_or_else(|error| panic!("{name} does not fold: {error}"));
        assert_eq!(
            &fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
            &state,
            "{name} diverges between audit and resimulate"
        );
    }
}

#[test]
fn fast_winner_fixture_recovers_with_cancellations() {
    let events = load_fixture("fast_winner.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.blitzes();
    assert_eq!(ledger.recovered_total(), 1);
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, BlitzClosure::Recovered);
    let outcome = record.outcome.as_ref().unwrap();
    assert!(outcome.cancellation.is_some());
    assert!(outcome.effective_parallelism_milli > 0);
}

#[test]
fn fast_wrong_answer_fixture_records_the_first_response_without_victory() {
    let state = fold(
        &load_fixture("fast_wrong_answer.json"),
        &Policy::builtin(),
        ApplyMode::Audit,
    )
    .unwrap();
    let outcome = state.blitzes().closed()[0].outcome.as_ref().unwrap();
    assert!(outcome.first_candidate_ms.is_some());
    assert!(outcome.winner.is_some());
    assert_ne!(outcome.first_candidate_ms, outcome.time_to_verified_ms);
}

#[test]
fn exhausted_fixtures_carry_their_failure_classes() {
    let policy = Policy::builtin();
    let budget = fold(
        &load_fixture("budget_exhausted.json"),
        &policy,
        ApplyMode::Audit,
    )
    .unwrap();
    assert_eq!(
        budget.blitzes().closed()[0]
            .outcome
            .as_ref()
            .unwrap()
            .failure,
        Some(BlitzFailure::BudgetExhausted)
    );
    let deadline = fold(
        &load_fixture("deadline_exhausted.json"),
        &policy,
        ApplyMode::Audit,
    )
    .unwrap();
    assert_eq!(
        deadline.blitzes().closed()[0]
            .outcome
            .as_ref()
            .unwrap()
            .failure,
        Some(BlitzFailure::DeadlineExhausted)
    );
    let verifier = fold(
        &load_fixture("verifier_fail_closed.json"),
        &policy,
        ApplyMode::Audit,
    )
    .unwrap();
    assert_eq!(
        verifier.blitzes().closed()[0]
            .outcome
            .as_ref()
            .unwrap()
            .failure,
        Some(BlitzFailure::VerificationFailure)
    );
    for name in [
        "budget_exhausted.json",
        "deadline_exhausted.json",
        "verifier_fail_closed.json",
    ] {
        let state = fold(&load_fixture(name), &policy, ApplyMode::Audit).unwrap();
        assert_eq!(
            state.blitzes().escalated_total(),
            1,
            "{name} must have executed its escalation policy"
        );
    }
}

#[test]
fn correlated_fixture_records_the_correlation_and_the_second_wave() {
    let state = fold(
        &load_fixture("correlated_failure.json"),
        &Policy::builtin(),
        ApplyMode::Audit,
    )
    .unwrap();
    let ledger = state.blitzes();
    assert_eq!(ledger.correlated_total(), 1);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert!(
        outcome.effective_parallelism_milli < u32::from(outcome.raw_parallelism) * 1_000,
        "correlated raw parallelism must exceed its effective value"
    );
}

#[test]
fn target_drift_fixture_supersedes_and_recovers_on_the_new_hash() {
    let state = fold(
        &load_fixture("target_drift.json"),
        &Policy::builtin(),
        ApplyMode::Audit,
    )
    .unwrap();
    let ledger = state.blitzes();
    assert_eq!(ledger.closed()[0].closure, BlitzClosure::Superseded);
    assert_eq!(ledger.closed()[1].closure, BlitzClosure::Recovered);
    assert_ne!(
        ledger.closed()[0].target_hash,
        ledger.closed()[1].target_hash
    );
}

#[test]
fn resource_mode_fixtures_show_the_formation_bias() {
    let policy = Policy::builtin();
    let (power_picked, _) = power_critical();
    let (network_picked, _) = network_critical();
    assert!(!power_picked.is_empty());
    assert!(!network_picked.is_empty());
    // Power scarcity prefers remote compute; network scarcity prefers local.
    assert!(
        power_picked.iter().all(|worker| !worker.local),
        "power-critical formation must favour remote workers"
    );
    assert!(
        network_picked.iter().all(|worker| worker.local),
        "network-critical formation must favour local workers"
    );
    let power_state = fold(
        &load_fixture("power_critical.json"),
        &policy,
        ApplyMode::Audit,
    )
    .unwrap();
    assert_eq!(power_state.blitzes().recovered_total(), 1);
}
