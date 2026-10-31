//! The concurrent-personnel verification (§19, §22, §40-7/8).
//!
//! Concurrency is representable only when identities stay distinct and
//! history is never rewritten. Every scenario here hand-builds a drive
//! event log and folds it through the real reducer — never a mock — and
//! every test ends with the audit/resimulate equality: the same journal
//! folds to the same state no matter which mode reads it.
//!
//! The pins:
//!
//! * disjoint domains do not collide — two identities journaling on
//!   separate domains leave two distinct records, not one merged one;
//! * a huddle during divergence reconciles and journals — the committed
//!   outcome carries its resolved contradiction on the record;
//! * the huddle is a coordination node, not an evidence authority — a
//!   committed huddle moves no gate satisfaction and declares no gates;
//! * an ownership exchange preserves history — the admission and the
//!   counter both stay on the ledger, and a decision computed before the
//!   handoff is a stale owner and is refused;
//! * parallel workers cannot collide on journaled identities — two
//!   dispatches with distinct worker ids fold cleanly into one formation.

use playbook_blitz::{
    effective_parallelism_milli, BlitzClass, BlitzResourceMode, BlitzTrigger, DefensivePressure,
    Severity, TargetPackage, VerificationKind, WorkerSlot,
};
use playbook_core::{
    BarrierMode, BlitzBudget, BlitzId, DriveBudget, DriveId, EventId, Evidence, EvidenceClass,
    EvidenceId, Gate, GateId, HuddleBudget, HuddleId, PersonnelId, TaskContract, TaskIntegrity,
};
use playbook_events::DriveEvent;
use playbook_huddle::{
    AssumptionDelta, Contradiction, ContradictionId, ContradictionResolution,
    ContradictionSeverity, DisclosureProfile, HuddleClosure, HuddleOutcome, HuddleParticipant,
    HuddleReport, HuddleRole, HuddleScope, HuddleTrigger, HuddleTriggerKind, ResolutionState,
    SharedStateDelta,
};
use playbook_policy::{BlitzCorroboration, Policy};
use playbook_state::{fold, ApplyMode, TacticRecordKind};

fn started(drive: &str) -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: DriveId::new(drive).unwrap(),
        task: TaskContract {
            statement: "hold one objective across concurrent personnel".into(),
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
    }
}

fn replay(events: &[DriveEvent]) {
    let policy = Policy::builtin();
    let audit = fold(events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(
        &fold(events, &policy, ApplyMode::Resimulate).unwrap(),
        &audit,
        "the journal diverges between audit and resimulate"
    );
}

fn contributor(id: &str) -> HuddleParticipant {
    HuddleParticipant {
        personnel_id: PersonnelId::new(id).unwrap(),
        role: HuddleRole::Contributor,
        disclosure_profile: DisclosureProfile {
            include_gates: true,
            include_peer_reports: true,
            include_strategic_state: false,
            max_evidence_refs: 8,
        },
    }
}

fn huddle_budget() -> HuddleBudget {
    HuddleBudget {
        max_tokens: 2_000,
        max_cost_microunits: 200_000,
        max_duration_ms: 5_000,
        max_rounds: 2,
        max_participants: 3,
    }
}

fn report(participant: &str, current: &str, evidence: Option<&EvidenceId>) -> HuddleReport {
    HuddleReport {
        participant: PersonnelId::new(participant).unwrap(),
        verified_progress: evidence.map(|id| vec![id.clone()]).unwrap_or_default(),
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

/// The diverging claim pair the huddle must reconcile.
fn divergence(claims: usize) -> Contradiction {
    Contradiction {
        id: ContradictionId::new("con-diverge").unwrap(),
        subject: "interface".into(),
        claims: (0..claims)
            .map(|index| playbook_huddle::ClaimRef {
                personnel_id: PersonnelId::new(format!("worker-{}", index + 1)).unwrap(),
                statement: format!("v{index}"),
                evidence_ref: None,
            })
            .collect(),
        severity: ContradictionSeverity::High,
    }
}

fn committed_outcome(huddle_id: &HuddleId) -> HuddleOutcome {
    HuddleOutcome {
        huddle_id: huddle_id.clone(),
        reconciled_state: SharedStateDelta {
            verified_evidence: Vec::new(),
            gate_snapshots: Vec::new(),
            assumptions: Vec::new(),
        },
        resolved_contradictions: vec![ContradictionResolution {
            contradiction_id: ContradictionId::new("con-diverge").unwrap(),
            state: ResolutionState::ResolvedByAuthority,
            evidence_ref: None,
            authority: Some("offensive_coordinator".into()),
        }],
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

/// One full huddle over diverging personnel: request, admission, two
/// reports, the detected contradiction, and the committed resolution.
/// `evidence` is the observation the reports cite; without one the reports
/// cite nothing, and the fold must still refuse to manufacture progress.
/// The caller journals the observation itself before this sequence.
fn diverged_huddle(drive: &str, evidence: Option<&EvidenceId>) -> Vec<DriveEvent> {
    let huddle_id = HuddleId::new("h-diverge").unwrap();
    let cited = evidence.map(|id| vec![id.clone()]).unwrap_or_default();
    let mut events = vec![started(drive)];
    events.push(DriveEvent::HuddleRequested {
        huddle_id: huddle_id.clone(),
        scope: HuddleScope::Position,
        trigger: HuddleTrigger {
            kind: HuddleTriggerKind::Failure,
            reason_code: "divergence_between_concurrent_personnel".into(),
            evidence_refs: cited.clone(),
            originating_event: EventId::new("evt-diverge").unwrap(),
        },
        at_ms: 2_000,
        debt: 0,
        rework_avoided: 600,
    });
    events.push(DriveEvent::HuddleAdmitted {
        huddle_id: huddle_id.clone(),
        budget: huddle_budget(),
        barrier: BarrierMode::Soft,
        participants: vec![contributor("worker-1"), contributor("worker-2")],
    });
    events.push(DriveEvent::HuddleOpened {
        huddle_id: huddle_id.clone(),
    });
    for author in ["worker-1", "worker-2"] {
        events.push(DriveEvent::HuddleReportSubmitted {
            huddle_id: huddle_id.clone(),
            report: Box::new(report(author, "v1", evidence)),
        });
    }
    events.push(DriveEvent::HuddleContradictionDetected {
        huddle_id: huddle_id.clone(),
        contradiction: divergence(2),
    });
    events.push(DriveEvent::HuddleOutcomeCommitted {
        huddle_id: huddle_id.clone(),
        outcome: Box::new(committed_outcome(&huddle_id)),
    });
    events
}

/// §19: two distinct identities journaling on disjoint domains fold
/// without collision — the substitutions and the tactical memory keep both
/// records distinct instead of collapsing them into one.
#[test]
fn concurrent_personnel_on_disjoint_domains_fold_without_collision() {
    let events = vec![
        started("drive-concurrency-disjoint"),
        DriveEvent::PlayerSubstituted {
            at_ms: 1_000,
            class: "tactical".into(),
            out: PersonnelId::new("primary-a").unwrap(),
            replacement: Some(PersonnelId::new("relief-a").unwrap()),
            reasons: vec!["disjoint_domain_a".into()],
        },
        DriveEvent::PlayerSubstituted {
            at_ms: 1_100,
            class: "tactical".into(),
            out: PersonnelId::new("primary-b").unwrap(),
            replacement: Some(PersonnelId::new("relief-b").unwrap()),
            reasons: vec!["disjoint_domain_b".into()],
        },
        DriveEvent::TacticAdmitted {
            tactic: "domain_a".into(),
            at_ms: 1_200,
        },
        DriveEvent::TacticAdmitted {
            tactic: "domain_b".into(),
            at_ms: 1_300,
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let substitutions = state.substitutions();
    assert_eq!(substitutions.len(), 2);
    assert_ne!(substitutions[0].out, substitutions[1].out);
    assert_ne!(
        substitutions[0].replacement, substitutions[1].replacement,
        "disjoint domains keep distinct replacements"
    );
    assert!(substitutions
        .iter()
        .all(|record| record.class == "tactical"));
    let ledger = state.tactics();
    assert_eq!(ledger.admitted_total(), 2);
    let tactics: Vec<&str> = ledger.history().iter().map(|r| r.tactic.as_str()).collect();
    assert!(tactics.contains(&"domain_a") && tactics.contains(&"domain_b"));
    assert!(ledger
        .history()
        .iter()
        .all(|record| matches!(record.kind, TacticRecordKind::Admitted)));
    replay(&events);
}

/// §22: a huddle opened mid-divergence reconciles and journals — exactly
/// one committed huddle, and the resolved contradiction is on the closed
/// record, not erased by it.
#[test]
fn a_huddle_during_divergence_reconciles_and_journals() {
    let observation = Evidence::seal(
        EvidenceClass::EnvironmentSnapshot,
        serde_json::json!({"host": "lab", "observed": 1_000}),
        1_000,
    )
    .unwrap();
    let evidence_id = observation.id.clone();
    let mut events = diverged_huddle("drive-concurrency-huddle", Some(&evidence_id));
    events.insert(
        1,
        DriveEvent::EvidenceAccepted {
            evidence: observation,
        },
    );
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.huddles();
    assert_eq!(ledger.committed_total(), 1);
    let record = &ledger.closed()[0];
    assert_eq!(record.closure, HuddleClosure::Committed);
    let outcome = record
        .outcome
        .as_ref()
        .expect("a commit carries its outcome");
    assert_eq!(outcome.resolved_contradictions.len(), 1);
    let resolution = &outcome.resolved_contradictions[0];
    assert_eq!(
        resolution.contradiction_id,
        ContradictionId::new("con-diverge").unwrap()
    );
    assert_eq!(resolution.state, ResolutionState::ResolvedByAuthority);
    assert!(events.iter().any(|event| matches!(
        event,
        DriveEvent::HuddleContradictionDetected { contradiction, .. }
            if contradiction.id == ContradictionId::new("con-diverge").unwrap()
    )));
    replay(&events);
}

/// §40-7: the huddle is a coordination node, not an evidence authority.
/// Two reads of the same doctrine: on a field with no gates the committed
/// huddle declares none, and on a field with a declared gate its
/// satisfaction stays 0 through the commit.
#[test]
fn the_huddle_is_a_coordination_node_not_evidence_authority() {
    // No GateDeclared, no EvidenceAccepted: the fold must not manufacture
    // a gate out of a coordination outcome.
    let gateless = diverged_huddle("drive-concurrency-gateless", None);
    let state = fold(&gateless, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.huddles().committed_total(), 1);
    assert!(
        state.gates().is_empty(),
        "a committed huddle declares no gates"
    );
    replay(&gateless);

    // A declared gate stays unsatisfied: consensus never satisfies evidence.
    let mut gated = diverged_huddle("drive-concurrency-gate", None);
    gated.insert(
        1,
        DriveEvent::GateDeclared {
            gate: Gate::declare(
                GateId::new("shared_objective").unwrap(),
                "the objective the huddle reconciles toward",
                1_000_000,
                Vec::new(),
                Vec::new(),
                false,
            )
            .unwrap(),
        },
    );
    let state = fold(&gated, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.huddles().committed_total(), 1);
    assert_eq!(state.gates().len(), 1);
    assert_eq!(
        state.gates()[0].satisfaction_ppm,
        0,
        "the huddle moved no gate satisfaction"
    );
    assert_eq!(state.gate_reopens(), 0);
    replay(&gated);
}

/// §40-8: an ownership exchange preserves history. The admission of the
/// owned domain and the counter that hands it off are both still on the
/// ledger — history is appended, never rewritten — and a handoff decision
/// computed against pre-handoff state is a stale owner, refused.
#[test]
fn an_ownership_exchange_preserves_history() {
    let events = vec![
        started("drive-concurrency-exchange"),
        DriveEvent::TacticAdmitted {
            tactic: "domain_x".into(),
            at_ms: 1_000,
        },
        DriveEvent::TacticCountered {
            tactic: "domain_x".into(),
            counter: "scrape_exchange".into(),
            at_ms: 2_000,
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.tactics();
    assert_eq!(ledger.admitted_total(), 1);
    assert_eq!(ledger.countered_total(), 1);
    let history = ledger.history();
    assert_eq!(history.len(), 2, "the handoff does not rewrite the call");
    assert_eq!(history[0].tactic, "domain_x");
    assert!(matches!(history[0].kind, TacticRecordKind::Admitted));
    assert_eq!(history[1].tactic, "domain_x");
    assert!(matches!(
        &history[1].kind,
        TacticRecordKind::Countered { counter }
            if counter == "scrape_exchange"
    ));
    replay(&events);

    // The exchange doctrine draws the staleness boundary itself: a
    // decision computed before the handoff (1_000 < the 2_000 handoff
    // state) is a stale owner; computed at the handoff it is valid.
    match playbook_tactics::exchange::reject_stale(2_000, 1_000) {
        Err(playbook_core::PlaybookError::Inadmissible(authority, code)) => {
            assert_eq!(authority, "tactics");
            assert_eq!(code, "exchange_stale_owner");
        }
        other => panic!("a pre-handoff decision is a stale owner, got {other:?}"),
    }
    assert!(playbook_tactics::exchange::reject_stale(2_000, 2_000).is_ok());
}

/// §19: two parallel workers cannot collide on journaled identities. Two
/// dispatches with distinct worker ids and distinct personnel fold cleanly
/// into one defensive formation — the ledger holds both, keyed by id.
#[test]
fn parallel_workers_cannot_collide_on_journaled_identities() {
    let target = TargetPackage {
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
    .unwrap();
    let slot = |id: &str, domain: &str, dispatched_at_ms: u64| WorkerSlot {
        worker_id: playbook_core::WorkerId::new(id).unwrap(),
        personnel_id: PersonnelId::new(format!("personnel-{id}")).unwrap(),
        provider: "alpha".into(),
        model_family: "f1".into(),
        failure_domains: vec![domain.into()],
        wave: 1,
        dispatched_at_ms,
        deadline_ms: 10_000,
    };
    let worker_w1 = slot("worker-w1", "dc-1", 1_100);
    let worker_w2 = slot("worker-w2", "dc-2", 1_200);
    let blitz_id = BlitzId::new("bz-concurrent").unwrap();
    let events = vec![
        started("drive-concurrency-blitz"),
        DriveEvent::BlitzProposed {
            blitz_id: blitz_id.clone(),
            trigger: BlitzTrigger {
                class: BlitzClass::Instinctive,
                reason_code: "power_critical".into(),
                evidence_refs: Vec::new(),
                originating_event: EventId::new("evt-blitz").unwrap(),
            },
            target: target.clone(),
            pressure: DefensivePressure {
                severity: Severity::Critical,
                deadline_ms: Some(10_000),
                deterioration_rate_ppm: 0,
                blast_radius_ppm: 900,
                solver_uncertainty_ppm: 700,
                containment_confidence_ppm: 100,
                recovery_confidence_ppm: 800,
                resource_decay_ppm: 400,
                expected_delay_loss: 50,
            },
            at_ms: 1_000,
        },
        DriveEvent::BlitzAuthorised {
            blitz_id: blitz_id.clone(),
            budget: BlitzBudget {
                max_tokens: 40_000,
                max_cost_microunits: 4_000_000,
                max_duration_ms: 120_000,
                max_workers: 2,
                max_waves: 3,
            },
            corroboration: BlitzCorroboration::FirstPass,
            resource_mode: BlitzResourceMode::Normal,
            at_ms: 1_000,
        },
        DriveEvent::BlitzTargetFrozen {
            blitz_id: blitz_id.clone(),
            target_hash: target.target_hash.clone(),
        },
        DriveEvent::BlitzWorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker_w1.clone(),
            effective_parallelism_milli: effective_parallelism_milli(std::slice::from_ref(
                &worker_w1,
            )),
        },
        DriveEvent::BlitzWorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker_w2.clone(),
            effective_parallelism_milli: effective_parallelism_milli(&[
                worker_w1.clone(),
                worker_w2.clone(),
            ]),
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let ledger = state.blitzes();
    assert_eq!(ledger.proposed_total(), 1);
    assert_eq!(ledger.authorised_total(), 1);
    assert_eq!(ledger.rejected_total(), 0);
    let blitz = ledger.active_blitz().expect("the formation is live");
    assert_eq!(blitz.workers.len(), 2);
    assert_ne!(blitz.workers[0].worker_id, blitz.workers[1].worker_id);
    assert_ne!(blitz.workers[0].personnel_id, blitz.workers[1].personnel_id);
    assert!(blitz.worker(&worker_w1.worker_id).is_some());
    assert!(blitz.worker(&worker_w2.worker_id).is_some());
    replay(&events);
}
