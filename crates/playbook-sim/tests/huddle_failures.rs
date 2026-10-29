//! §26 failure-injection coverage for the huddle subsystem.
//!
//! Every scenario drives the single-writer authority, keeps the journal as
//! the source of truth, and ends with an audit/resimulate fold determinism
//! assertion over that journal.

use playbook_core::{
    DriveId, Evidence, EvidenceClass, EvidenceId, Gate, GateId, HuddleId, Personnel, PersonnelId,
    PersonnelKind, PlaybookError, TaskContract, TaskIntegrity, PPM_SCALE,
};
use playbook_huddle::{
    AssumptionDelta, ContradictionSeverity, HuddleClosure, HuddleCost, HuddleOutcome, HuddlePhase,
    HuddleReport, HuddleScope, HuddleTriggerKind, ParticipantCandidate, SharedStateDelta,
};
use playbook_policy::Policy;
use playbook_sim::{apply, fold, started_budget, ApplyMode, Authority, DriveEvent};
use playbook_state::{HuddleAdmission, HuddleRequest};
use serde_json::{json, Value};

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

fn roster() -> Vec<Personnel> {
    vec![
        person("worker-a", PersonnelKind::DeterministicProcedure),
        person("worker-b", PersonnelKind::DeterministicProcedure),
        person("model-a", PersonnelKind::Model),
    ]
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
        started_budget(100_000, 1_000),
        Policy::builtin(),
        roster(),
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

fn huddle_value() -> playbook_huddle::HuddleValue {
    playbook_huddle::HuddleValue {
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
        json!({"host": "lab", "observed": at_ms}),
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

fn open_position_huddle(authority: &mut Authority) -> HuddleId {
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
    huddle_id
}

fn refuses_with(error: PlaybookError, why: &str) {
    assert!(
        matches!(error, PlaybookError::Inadmissible(_, ref message) if message.contains(why)),
        "expected a refusal containing `{why}`, got {error:?}"
    );
}

fn journal_agrees_with_fold(authority: &Authority) {
    let policy = authority.policy().clone();
    let audit = fold(authority.log(), &policy, ApplyMode::Audit).unwrap();
    assert_eq!(&audit, authority.state());
    assert_eq!(
        &fold(authority.log(), &policy, ApplyMode::Resimulate).unwrap(),
        &audit
    );
}

#[test]
fn contradictory_reports_become_objects_and_force_escalation() {
    let mut authority = authority("drive-huddle-failure-contradiction");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
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
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].claims.len(), 3);
    assert_eq!(detected[0].severity, ContradictionSeverity::High);
    assert!(authority.log().iter().any(|event| matches!(
        event,
        DriveEvent::HuddleContradictionDetected { contradiction, .. }
            if contradiction.severity == ContradictionSeverity::High
    )));

    let child = authority
        .escalate_huddle(&huddle_id, false, "unresolved_interface_contradiction")
        .unwrap();
    {
        let ledger = authority.state().huddles();
        assert_eq!(ledger.escalated_total(), 1);
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::Escalated);
        let child_huddle = ledger.active_by_id(&child).unwrap();
        assert_eq!(child_huddle.scope, HuddleScope::Unit);
        assert_eq!(child_huddle.phase, HuddlePhase::Requested);
    }
    journal_agrees_with_fold(&authority);
}

#[test]
fn duplicate_report_submission_is_refused() {
    let mut authority = authority("drive-huddle-failure-duplicate");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    refuses_with(
        authority
            .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
            .unwrap_err(),
        "duplicate report",
    );
    assert_eq!(
        authority
            .state()
            .huddles()
            .active_by_id(&huddle_id)
            .unwrap()
            .reports
            .len(),
        1
    );
    journal_agrees_with_fold(&authority);
}

#[test]
fn tampered_report_is_refused_on_replay() {
    let mut authority = authority("drive-huddle-failure-tamper");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v2", &evidence))
        .unwrap();
    journal_agrees_with_fold(&authority);
    let log = authority.log().to_vec();
    let policy = Policy::builtin();

    let mut json: Value = serde_json::from_str(&serde_json::to_string(&log).unwrap()).unwrap();
    let mut rewritten = 0;
    for event in json.as_array_mut().unwrap() {
        if event["type"] == "huddle_report_submitted" {
            event["report"]["confidence_milli"] = json!(20_000);
            rewritten += 1;
        }
    }
    assert_eq!(rewritten, 2);
    let tampered: Vec<DriveEvent> = serde_json::from_value(json).unwrap();
    refuses_with(
        fold(&tampered, &policy, ApplyMode::Audit).unwrap_err(),
        "confidence_milli out of range",
    );

    let mut json: Value = serde_json::from_str(&serde_json::to_string(&log).unwrap()).unwrap();
    for event in json.as_array_mut().unwrap() {
        if event["type"] == "huddle_report_submitted" {
            event["report"]["verified_progress"][0] = json!("evidence-fabricated");
        }
    }
    let tampered: Vec<DriveEvent> = serde_json::from_value(json).unwrap();
    refuses_with(
        fold(&tampered, &policy, ApplyMode::Audit).unwrap_err(),
        "unknown evidence reference",
    );
    assert_eq!(
        fold(&log, &policy, ApplyMode::Audit).unwrap(),
        fold(&log, &policy, ApplyMode::Resimulate).unwrap()
    );
}

#[test]
fn crash_after_open_and_after_reports_replay_identically() {
    let mut authority = authority("drive-huddle-failure-crash");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
    let after_open = authority.log().to_vec();
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
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v1", &evidence))
        .unwrap();
    let after_reports = authority.log().to_vec();
    assert!(after_open.len() < after_reports.len());
    let policy = Policy::builtin();
    let state_full = fold(&after_reports, &policy, ApplyMode::Audit).unwrap();

    for (prefix, reports, spent_tokens) in [
        (&after_open, 0usize, 0u64),
        (&after_reports, 2usize, 100u64),
    ] {
        let resumed = fold(prefix, &policy, ApplyMode::Audit).unwrap();
        assert_eq!(&resumed, &fold(prefix, &policy, ApplyMode::Audit).unwrap());
        assert_eq!(
            &fold(prefix, &policy, ApplyMode::Resimulate).unwrap(),
            &resumed
        );
        let active = resumed.huddles().active_by_id(&huddle_id).unwrap();
        assert_eq!(active.phase, HuddlePhase::CollectReports);
        assert_eq!(active.participants.len(), 2);
        assert_eq!(active.reports.len(), reports);
        assert_eq!(active.spent.tokens, spent_tokens);
        let mut replayed = resumed.clone();
        for event in &after_reports[prefix.len()..] {
            replayed = apply(Some(&replayed), event, &policy, ApplyMode::Audit).unwrap();
        }
        assert_eq!(&replayed, &state_full);
    }

    let recovered = Authority::resume(after_reports.clone(), policy.clone(), roster()).unwrap();
    assert_eq!(
        recovered.state(),
        &fold(&after_reports, &policy, ApplyMode::Audit).unwrap()
    );
    journal_agrees_with_fold(&authority);
}

#[test]
fn budget_exhaustion_mid_collection_forces_synthesis_and_hands_off() {
    let mut authority = authority("drive-huddle-failure-budget");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
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
                tokens: 1_000,
                cost_microunits: 100_000,
                duration_ms: 2_500,
                rounds: 0,
            },
        )
        .unwrap();
    authority
        .consume_huddle_budget(
            &huddle_id,
            HuddleCost {
                tokens: 1_000,
                cost_microunits: 100_000,
                duration_ms: 2_500,
                rounds: 1,
            },
        )
        .unwrap();
    assert!(
        authority
            .state()
            .huddles()
            .active_by_id(&huddle_id)
            .unwrap()
            .exhausted
    );
    refuses_with(
        authority
            .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
            .unwrap_err(),
        "budget exhausted; synthesis only",
    );
    assert_eq!(
        authority
            .consume_huddle_budget(
                &huddle_id,
                HuddleCost {
                    tokens: 1,
                    cost_microunits: 0,
                    duration_ms: 0,
                    rounds: 0,
                },
            )
            .unwrap_err(),
        PlaybookError::BudgetExhausted
    );

    authority.force_huddle_synthesis(&huddle_id).unwrap();
    let ledger = authority.state().huddles();
    assert!(ledger.active().is_empty());
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::ForcedSynthesis);
    let outcome = ledger.closed()[0].outcome.as_ref().unwrap();
    assert_eq!(outcome.unresolved_contradictions.len(), 1);
    assert_eq!(outcome.escalated_to, Some(HuddleScope::Unit));
    assert!(ledger
        .conditions()
        .iter()
        .any(|condition| condition.reason_code == "post_synthesis_review"));
    journal_agrees_with_fold(&authority);
}

/// A child huddle inherits no justification: `escalation_target` demands a
/// blocking contradiction or coordinator-band debt of its own, and manual
/// authority only unlocks the Franchise boundary, never justification.
#[test]
fn recursive_escalation_without_new_justification_is_refused() {
    let mut authority = authority("drive-huddle-failure-recursive");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
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
    assert_eq!(detected[0].severity, ContradictionSeverity::High);
    let child = authority
        .escalate_huddle(&huddle_id, false, "unresolved_interface_contradiction")
        .unwrap();
    let ledger = authority.state().huddles();
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::Escalated);
    let child_huddle = ledger.active_by_id(&child).unwrap();
    assert_eq!(child_huddle.scope, HuddleScope::Unit);
    assert_eq!(child_huddle.phase, HuddlePhase::Requested);
    assert!(child_huddle.contradictions.is_empty());
    assert!(child_huddle.reports.is_empty());

    for manual in [false, true] {
        refuses_with(
            authority
                .escalate_huddle(&child, manual, "unresolved_interface_contradiction")
                .unwrap_err(),
            "escalation lacks scope justification",
        );
    }
    assert!(authority.state().huddles().active_by_id(&child).is_some());
    journal_agrees_with_fold(&authority);
}

#[test]
fn bench_review_never_benches_without_a_committed_recommendation() {
    let mut authority = authority("drive-huddle-failure-bench");
    let evidence = observation(&mut authority, 1_500);
    let huddle_id = open_position_huddle(&mut authority);
    authority
        .submit_huddle_report(&huddle_id, report("worker-a", "v1", &evidence))
        .unwrap();
    authority
        .submit_huddle_report(&huddle_id, report("worker-b", "v1", &evidence))
        .unwrap();
    authority
        .commit_huddle_outcome(&huddle_id, empty_outcome(&huddle_id))
        .unwrap();
    assert_eq!(authority.bench_from_review().unwrap(), 0);
    let ledger = authority.state().huddles();
    assert_eq!(ledger.closed()[0].closure, HuddleClosure::Committed);
    assert!(authority.state().benched().is_empty());
    assert!(ledger.substitutions().is_empty());
    assert!(!authority
        .log()
        .iter()
        .any(|event| matches!(event, DriveEvent::BenchDecision { .. })));
    journal_agrees_with_fold(&authority);
}
