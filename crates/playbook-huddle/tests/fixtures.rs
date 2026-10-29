//! Scenario fixtures for the huddle subsystem.
//!
//! The five transcripts under `fixtures/huddles/` are full drive logs built
//! through the authority, so every event in them is one the fold accepted.
//! Regenerate with:
//!
//! ```text
//! cargo test -p playbook-huddle --offline -- --ignored regenerate_huddle_fixtures
//! ```
//!
//! The checked-in test folds each fixture and asserts its scenario marker,
//! which is what keeps the fixtures honest.

use playbook_core::{
    DriveBudget, DriveId, EventId, Evidence, EvidenceClass, EvidenceId, Gate, GateId, HuddleId,
    Personnel, PersonnelId, PersonnelKind, PlaybookError, TaskContract, TaskIntegrity, PPM_SCALE,
};
use playbook_events::DriveEvent;
use playbook_huddle::{
    admission, detect_contradictions, AssignmentChange, AssumptionDelta, HuddleClosure, HuddleCost,
    HuddleOutcome, HuddleReport, HuddleScope, HuddleTriggerKind, HuddleValue, ParticipantCandidate,
    SharedStateDelta,
};
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode, Authority, HuddleAdmission, HuddleRequest};

fn person(id: &str, kind: PersonnelKind) -> Personnel {
    Personnel {
        id: PersonnelId::new(id).unwrap(),
        kind,
        display_name: id.into(),
        cost_tokens: 40,
        latency_ms: 20,
        capability_fit_ppm: PPM_SCALE,
        reliability_ppm: 900_000,
        quota_remaining_ppm: PPM_SCALE,
        evidence_quality_ppm: PPM_SCALE,
        own_ytg_milli: None,
    }
}

fn authority(drive: &str) -> Authority {
    let mut authority = Authority::start(
        DriveId::new(drive).unwrap(),
        TaskContract {
            statement: "reconcile the huddle scenario".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        },
        playbook_state::started_budget(100_000, 1_000),
        Policy::builtin(),
        vec![
            person("worker-a", PersonnelKind::DeterministicProcedure),
            person("worker-b", PersonnelKind::DeterministicProcedure),
            person("model-a", PersonnelKind::Model),
        ],
        1_000,
    )
    .unwrap();
    authority
        .declare_gate(
            Gate::declare(
                GateId::new("implementation").unwrap(),
                "compiler accepts the implementation",
                1_000_000,
                Vec::new(),
                vec![EvidenceClass::CompilerResult],
                true,
            )
            .unwrap(),
        )
        .unwrap();
    authority
}

fn candidate(id: &str, relevance: u32) -> ParticipantCandidate {
    ParticipantCandidate {
        personnel_id: PersonnelId::new(id).unwrap(),
        dependency_overlap: relevance,
        contradiction_relevance: relevance,
        ownership_weight: relevance,
        downstream_impact: relevance,
        specialist_value: relevance,
        context_cost: 10,
    }
}

fn huddle_value() -> HuddleValue {
    HuddleValue {
        predicted_rework_avoided: 600,
        expected_failure_reduction: 300,
        expected_coordination_gain: 200,
        expected_budget_preservation: 100,
        inference_cost: 100,
        latency_cost: 50,
        context_cost: 50,
        opportunity_cost: 0,
    }
}

fn request(scope: HuddleScope, kind: HuddleTriggerKind, reason: &str) -> HuddleRequest {
    HuddleRequest {
        scope,
        trigger_kind: kind,
        reason_code: reason.into(),
        evidence_refs: Vec::new(),
        manual: false,
        value: huddle_value(),
        candidates: vec![
            candidate("worker-a", 900),
            candidate("worker-b", 900),
            candidate("model-a", 100),
        ],
        debt: 0,
        rework_avoided: 600,
    }
}

fn observation(authority: &mut Authority, at_ms: u64) -> EvidenceId {
    let evidence = Evidence::seal(
        EvidenceClass::EnvironmentSnapshot,
        serde_json::json!({"host": "fixture", "observed": at_ms}),
        at_ms,
    )
    .unwrap();
    let id = evidence.id.clone();
    authority.accept_evidence(evidence).unwrap();
    id
}

fn report(participant: &str, current: &str, evidence: &EvidenceId) -> HuddleReport {
    HuddleReport {
        participant: PersonnelId::new(participant).unwrap(),
        verified_progress: vec![evidence.clone()],
        blockers: Vec::new(),
        changed_assumptions: vec![AssumptionDelta {
            subject: "interface".into(),
            previous: "v1".into(),
            current: current.into(),
            evidence_ref: None,
        }],
        dependency_changes: Vec::new(),
        contradictions: Vec::new(),
        proposed_action: None,
        confidence_milli: 9_000,
    }
}

fn empty_outcome(huddle_id: &HuddleId) -> HuddleOutcome {
    HuddleOutcome {
        huddle_id: huddle_id.clone(),
        reconciled_state: SharedStateDelta {
            verified_evidence: Vec::new(),
            gate_snapshots: Vec::new(),
            assumptions: Vec::new(),
        },
        resolved_contradictions: Vec::new(),
        unresolved_contradictions: Vec::new(),
        assignment_changes: Vec::new(),
        strategy_change: None,
        personnel_changes: Vec::new(),
        revised_ytg_milli: None,
        debt_after: None,
        next_play: None,
        next_huddle_conditions: Vec::new(),
        escalated_to: None,
    }
}

/// A failure-triggered position huddle reconciles cleanly: agreement, one
/// assignment change, a next-play recommendation, committed closure.
fn position_success() -> Vec<DriveEvent> {
    let mut authority = authority("drive-huddle-position-success");
    let evidence = observation(&mut authority, 1_500);
    let HuddleAdmission::Open(huddle_id) = authority
        .open_huddle(
            request(
                HuddleScope::Position,
                HuddleTriggerKind::Failure,
                "repeated_failed_play",
            ),
            2_000,
        )
        .unwrap()
    else {
        panic!("scenario assumes admission")
    };
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v1", &evidence))
        .unwrap();
    authority
        .consume_huddle_budget(
            &huddle_id,
            HuddleCost {
                tokens: 100,
                cost_microunits: 10_000,
                duration_ms: 120,
                rounds: 0,
            },
        )
        .unwrap();
    let mut outcome = empty_outcome(&huddle_id);
    outcome.reconciled_state.verified_evidence = vec![evidence];
    outcome.assignment_changes = vec![AssignmentChange {
        personnel_id: PersonnelId::new("worker-b").unwrap(),
        from: Some("compile".into()),
        to: "test.unit".into(),
    }];
    outcome.next_play = Some(playbook_huddle::PlayProposal {
        operation: "test.unit".into(),
        rationale: "unit coverage is the open dependency".into(),
        target_gate: Some(GateId::new("unit_tests").unwrap()),
    });
    outcome.revised_ytg_milli = Some(authority.state().ytg_milli());
    authority
        .commit_huddle_outcome(&huddle_id, outcome)
        .unwrap();
    authority.log().to_vec()
}

/// A repeat request on an unchanged field is a refused huddle storm.
fn position_failure() -> Vec<DriveEvent> {
    let mut authority = authority("drive-huddle-position-failure");
    let evidence = observation(&mut authority, 1_500);
    let HuddleAdmission::Open(huddle_id) = authority
        .open_huddle(
            request(
                HuddleScope::Position,
                HuddleTriggerKind::Failure,
                "repeated_failed_play",
            ),
            2_000,
        )
        .unwrap()
    else {
        panic!("scenario assumes admission")
    };
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v1", &evidence))
        .unwrap();
    authority
        .commit_huddle_outcome(&huddle_id, empty_outcome(&huddle_id))
        .unwrap();
    let refused = authority
        .open_huddle(
            request(
                HuddleScope::Position,
                HuddleTriggerKind::Failure,
                "repeated_failed_play",
            ),
            3_000,
        )
        .unwrap();
    let HuddleAdmission::Refused { reason_code, .. } = refused else {
        panic!("scenario assumes a storm refusal")
    };
    assert_eq!(reason_code, "unchanged_field");
    authority.log().to_vec()
}

/// Contradicting reports become an explicit High-severity contradiction;
/// the huddle escalates to unit scope and the child huddle is left open.
fn unit_contradiction() -> Vec<DriveEvent> {
    let mut authority = authority("drive-huddle-unit-contradiction");
    let evidence = observation(&mut authority, 1_500);
    let HuddleAdmission::Open(huddle_id) = authority
        .open_huddle(
            request(
                HuddleScope::Position,
                HuddleTriggerKind::Failure,
                "repeated_failed_play",
            ),
            2_000,
        )
        .unwrap()
    else {
        panic!("scenario assumes admission")
    };
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v2", &evidence))
        .unwrap();
    authority
        .invite_to_huddle(&huddle_id, &PersonnelId::new("model-a").unwrap())
        .unwrap();
    let detected = authority
        .submit_huddle_report(&huddle_id, report("model-a", "v3", &evidence))
        .unwrap();
    assert_eq!(
        detected[0].severity,
        playbook_huddle::ContradictionSeverity::High
    );
    // The escalation is a new admission event: the child waits in the
    // requested phase until the coordinator admits it.
    let _child = authority
        .escalate_huddle(&huddle_id, false, "unresolved_interface_contradiction")
        .unwrap();
    authority.log().to_vec()
}

/// A coordinator-scope strategic huddle commits a justified strategy
/// change; the tactical authority journals it like any other event.
fn coordinator_fourth_down() -> Vec<DriveEvent> {
    let mut authority = authority("drive-huddle-coordinator");
    let evidence = observation(&mut authority, 1_500);
    let huddle_request = HuddleRequest {
        scope: HuddleScope::Coordinator,
        trigger_kind: HuddleTriggerKind::Strategic,
        reason_code: "fourth_down_disposition".into(),
        evidence_refs: Vec::new(),
        manual: false,
        value: huddle_value(),
        candidates: vec![
            candidate("worker-a", 700),
            candidate("worker-b", 700),
            candidate("model-a", 100),
        ],
        debt: 0,
        rework_avoided: 600,
    };
    let HuddleAdmission::Open(huddle_id) = authority.open_huddle(huddle_request, 2_000).unwrap()
    else {
        panic!("scenario assumes admission")
    };
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v1", &evidence))
        .unwrap();
    let mut outcome = empty_outcome(&huddle_id);
    outcome.reconciled_state.verified_evidence = vec![evidence];
    outcome.strategy_change = Some(playbook_huddle::StrategyChange {
        subject: "possession".into(),
        from: "rush the implementation gate".into(),
        to: "extract the unit-test field goal first".into(),
        justification: "banked unit coverage dominates the rush upside".into(),
    });
    outcome.revised_ytg_milli = Some(authority.state().ytg_milli());
    authority
        .commit_huddle_outcome(&huddle_id, outcome)
        .unwrap();
    authority.log().to_vec()
}

/// Budget exhaustion terminates discussion and forces synthesis from the
/// accumulated reports; the blocking remainder is handed to unit scope.
fn budget_exhausted() -> Vec<DriveEvent> {
    let mut authority = authority("drive-huddle-budget-exhausted");
    let evidence = observation(&mut authority, 1_500);
    let HuddleAdmission::Open(huddle_id) = authority
        .open_huddle(
            request(
                HuddleScope::Position,
                HuddleTriggerKind::Failure,
                "repeated_failed_play",
            ),
            2_000,
        )
        .unwrap()
    else {
        panic!("scenario assumes admission")
    };
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v2", &evidence))
        .unwrap();
    authority
        .consume_huddle_budget(
            &huddle_id,
            HuddleCost {
                tokens: 2_000,
                cost_microunits: 200_000,
                duration_ms: 5_000,
                rounds: 1,
            },
        )
        .unwrap();
    authority.force_huddle_synthesis(&huddle_id).unwrap();
    authority.log().to_vec()
}

fn write_fixture(name: &str, events: &[DriveEvent]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/huddles");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(name), serde_json::to_string(events).unwrap()).unwrap();
}

fn load_fixture(name: &str) -> Vec<DriveEvent> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../fixtures/huddles/{name}"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn fixture_events() -> Vec<(&'static str, Vec<DriveEvent>)> {
    vec![
        ("position_success.json", position_success()),
        ("position_failure.json", position_failure()),
        ("unit_contradiction.json", unit_contradiction()),
        ("coordinator_fourth_down.json", coordinator_fourth_down()),
        ("budget_exhausted.json", budget_exhausted()),
    ]
}

#[test]
#[ignore]
fn regenerate_huddle_fixtures() {
    for (name, events) in fixture_events() {
        write_fixture(name, &events);
    }
}

#[test]
fn every_huddle_fixture_folds_and_replays() {
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
fn position_success_commits_an_assignment() {
    let events = load_fixture("position_success.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.committed_total(), 1);
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::Committed);
    assert!(ledger.assignments().contains_key("worker-b"));
    assert_eq!(ledger.proposals().len(), 1);
    assert!(state.gates()[0].satisfaction_ppm == 0);
}

#[test]
fn position_failure_records_the_refused_storm() {
    let events = load_fixture("position_failure.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.committed_total(), 1);
    assert_eq!(ledger.rejected_total(), 1);
    assert_eq!(ledger.closed()[1].closure, HuddleClosure::Rejected);
    assert_eq!(ledger.closed()[1].reason_code, "unchanged_field");
}

#[test]
fn unit_contradiction_leaves_an_active_child_huddle() {
    let events = load_fixture("unit_contradiction.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.escalated_total(), 1);
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::Escalated);
    let child = &ledger.active()[0];
    assert_eq!(child.scope, HuddleScope::Unit);
    assert_eq!(child.phase, playbook_huddle::HuddlePhase::Requested);
    // The contradiction itself lives on the closed parent's journal trail;
    // the child starts with a clean slate.
    assert!(events.iter().any(|event| matches!(
        event,
        DriveEvent::HuddleContradictionDetected { contradiction, .. }
            if contradiction.severity == playbook_huddle::ContradictionSeverity::High
    )));
}

#[test]
fn coordinator_scenario_commits_a_justified_strategy_change() {
    let events = load_fixture("coordinator_fourth_down.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.closed()[0].scope, HuddleScope::Coordinator);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    let change = outcome.strategy_change.as_ref().unwrap();
    assert_eq!(change.subject, "possession");
    assert!(!change.justification.is_empty());
}

#[test]
fn budget_exhaustion_forces_synthesis_and_escalates() {
    let events = load_fixture("budget_exhausted.json");
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::ForcedSynthesis);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.unresolved_contradictions.len(), 1);
    assert_eq!(outcome.escalated_to, Some(HuddleScope::Unit));
    assert!(ledger
        .conditions()
        .iter()
        .any(|condition| condition.reason_code == "post_synthesis_review"));
}

#[test]
fn admission_decides_the_same_way_for_every_fixture_trigger() {
    // Determinism spot check: identical inputs, identical decision.
    let book = playbook_huddle::HuddleLedger::empty();
    let trigger = playbook_huddle::HuddleTrigger {
        kind: HuddleTriggerKind::Failure,
        reason_code: "repeated_failed_play".into(),
        evidence_refs: Vec::new(),
        originating_event: EventId::new("event-0").unwrap(),
    };
    let inputs = admission::AdmissionInputs {
        scope: HuddleScope::Position,
        trigger: &trigger,
        debt: 0,
        field_fingerprint: "field",
        ledger: &book,
        value: huddle_value(),
        remaining_drive_budget: DriveBudget {
            tokens: 100_000,
            monetary_cost_micros: 1_000_000,
            ..DriveBudget::zero()
        },
        manual: false,
    };
    let policy = Policy::builtin();
    let first = admission::decide(&inputs, &policy.huddle).unwrap();
    assert_eq!(first, admission::decide(&inputs, &policy.huddle).unwrap());
    assert!(matches!(first, admission::AdmissionDecision::Admit { .. }));
    let _ = detect_contradictions(&[]);
    let _: Result<(), PlaybookError> = Ok(());
}
