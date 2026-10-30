//! Contract, evaluation, bench, CERS, and TT integration tests: the
//! journal-level proofs that a sealed contract is the only authority on
//! victory, and that the new branches replay.

use playbook_core::{
    Evidence, EvidenceClass, GameContract, Gate, GateId, PlaybookError, TinkerReport, TtDirective,
    WinCondition, WinConditionId, WinConditionKind,
};
use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode};

fn gate(id: &str) -> Gate {
    Gate::declare(
        GateId::new(id).unwrap(),
        format!("{id} holds"),
        1_000_000,
        Vec::new(),
        vec![EvidenceClass::CompilerResult],
        true,
    )
    .unwrap()
}

fn started() -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: playbook_core::DriveId::new("drive-contract").unwrap(),
        task: playbook_core::TaskContract {
            statement: "repair the parser".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        budget: playbook_core::DriveBudget {
            tokens: 100_000,
            monetary_cost_micros: 10_000_000,
            ..playbook_core::DriveBudget::zero()
        },
        at_ms: 0,
    }
}

fn contract(gate_id: &str, id: &str) -> GameContract {
    GameContract {
        game_id: playbook_core::GameId::new(id).unwrap(),
        objective: playbook_core::TaskContract {
            statement: "repair the parser".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        win_conditions: vec![WinCondition {
            id: WinConditionId::new("win-build").unwrap(),
            kind: WinConditionKind::GateSatisfied {
                gate: GateId::new(gate_id).unwrap(),
            },
            description: "the build gate holds".into(),
            mandatory: true,
        }],
        loss_conditions: Vec::new(),
        budget: playbook_core::DriveBudget {
            tokens: 100_000,
            ..playbook_core::DriveBudget::zero()
        },
        evidence_policy: playbook_core::EvidencePolicy {
            accepted_classes: vec![EvidenceClass::CompilerResult],
        },
        deadline_ms: None,
        predecessor_hash: None,
        contract_hash: String::new(),
    }
    .seal()
    .unwrap()
}

fn compiler_evidence(at_ms: u64) -> Evidence {
    Evidence::seal(
        EvidenceClass::CompilerResult,
        serde_json::json!({"success": true, "gate_id": "build"}),
        at_ms,
    )
    .unwrap()
}

fn completed(at_ms: u64) -> DriveEvent {
    DriveEvent::DriveCompleted { at_ms }
}

#[test]
fn a_win_is_only_verified_through_the_sealed_contract() {
    let events = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(contract("build", "game-1")),
            at_ms: 100,
        },
        DriveEvent::EvidenceAccepted {
            evidence: compiler_evidence(200),
        },
        completed(300),
    ];
    let policy = Policy::builtin();
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(state.status(), playbook_core::DriveStatus::Completed);
    assert!(state.game_contract().is_some());
    assert_eq!(
        fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        state,
        "contract-gated completion must replay"
    );
}

#[test]
fn a_contract_victory_is_refused_while_a_mandatory_condition_is_unmet() {
    let events = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(contract("build", "game-1")),
            at_ms: 100,
        },
        // The claim arrives before the evidence: no win.
        completed(150),
    ];
    let error = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap_err();
    assert_eq!(error, PlaybookError::Incomplete);
}

#[test]
fn completion_without_evidence_at_all_is_refused_under_a_contract() {
    let events = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(contract("build", "game-1")),
            at_ms: 100,
        },
        completed(300),
    ];
    assert_eq!(
        fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Incomplete
    );
}

#[test]
fn an_amendment_must_name_its_predecessor() {
    let original = contract("build", "game-1");
    let amended = original.clone().amend(playbook_core::EvidencePolicy {
        accepted_classes: vec![EvidenceClass::CompilerResult, EvidenceClass::TestReport],
    });
    let orphan = contract("build", "game-1");
    let events = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(original),
            at_ms: 100,
        },
        // A contract with no predecessor cannot follow a seated one.
        DriveEvent::GameContractAmended {
            contract: Box::new(orphan),
            at_ms: 150,
        },
    ];
    assert!(matches!(
        fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Inadmissible(_, _)
    ));
    let lawful = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(contract("build", "game-1")),
            at_ms: 100,
        },
        DriveEvent::GameContractAmended {
            contract: Box::new(amended),
            at_ms: 150,
        },
        DriveEvent::EvidenceAccepted {
            evidence: compiler_evidence(200),
        },
        completed(300),
    ];
    let policy = Policy::builtin();
    let state = fold(&lawful, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(state.status(), playbook_core::DriveStatus::Completed);
    assert_eq!(
        fold(&lawful, &policy, ApplyMode::Resimulate).unwrap(),
        state
    );
}

#[test]
fn evaluation_branches_are_journalled_with_reasons() {
    use playbook_core::EvaluationBranch;
    let events = vec![
        started(),
        DriveEvent::EvaluationDecided {
            at_ms: 100,
            branch: EvaluationBranch::Lastdown,
            reasons: vec!["cycle_failure".into()],
        },
        DriveEvent::EvaluationDecided {
            at_ms: 200,
            branch: EvaluationBranch::CersRecovery,
            reasons: vec!["material_failure_tool_failure".into()],
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    let evaluation = state.last_evaluation().unwrap();
    assert_eq!(evaluation.branch, EvaluationBranch::CersRecovery);
    assert_eq!(evaluation.reasons, vec!["material_failure_tool_failure"]);
    // Reason-less decisions are refused at fold time.
    let refusal = vec![
        started(),
        DriveEvent::EvaluationDecided {
            at_ms: 100,
            branch: EvaluationBranch::Advance,
            reasons: Vec::new(),
        },
    ];
    assert!(matches!(
        fold(&refusal, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Inadmissible(_, _)
    ));
}

#[test]
fn substitutions_are_journalled_by_class() {
    let events = vec![
        started(),
        DriveEvent::PlayerSubstituted {
            at_ms: 100,
            class: "performance".into(),
            out: playbook_core::PersonnelId::new("model-a").unwrap(),
            replacement: Some(playbook_core::PersonnelId::new("model-b").unwrap()),
            reasons: vec!["three_recent_gate_failures".into()],
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.substitutions().len(), 1);
    assert_eq!(state.substitutions()[0].class, "performance");
    let refusal = vec![
        started(),
        DriveEvent::PlayerSubstituted {
            at_ms: 100,
            class: "whim".into(),
            out: playbook_core::PersonnelId::new("model-a").unwrap(),
            replacement: None,
            reasons: vec!["because".into()],
        },
    ];
    assert!(matches!(
        fold(&refusal, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Inadmissible(_, _)
    ));
}

#[test]
fn cers_artefacts_are_journalled_and_counted() {
    let events = vec![
        started(),
        DriveEvent::CersRecovered {
            artefact_id: playbook_core::EvidenceId::new("cers-1").unwrap(),
            source: "failed_play".into(),
            confirmed_facts: 3,
            failed_assumptions: 1,
            reuse_hint: "operation:compile".into(),
            at_ms: 100,
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.cers_artefacts().len(), 1);
    assert_eq!(
        state.cers_artefacts()[0].confirmed_facts + state.cers_artefacts()[0].failed_assumptions,
        4
    );
    let empty = vec![
        started(),
        DriveEvent::CersRecovered {
            artefact_id: playbook_core::EvidenceId::new("cers-empty").unwrap(),
            source: "failed_play".into(),
            confirmed_facts: 0,
            failed_assumptions: 0,
            reuse_hint: "operation:compile".into(),
            at_ms: 100,
        },
    ];
    assert!(matches!(
        fold(&empty, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Evidence(_)
    ));
}

#[test]
fn the_tinker_never_rewrites_the_specification() {
    let directive = TtDirective {
        id: playbook_core::DirectiveId::new("dir-1").unwrap(),
        thinker: playbook_core::PersonnelId::new("thinker").unwrap(),
        tinker: playbook_core::PersonnelId::new("tinker").unwrap(),
        specification: "implement the parser per the sealed interface".into(),
        specification_hash: String::new(),
        expected_evidence: vec![EvidenceClass::CompilerResult],
    }
    .seal()
    .unwrap();
    let conforming = TinkerReport {
        directive_id: directive.id.clone(),
        specification_hash: directive.specification_hash.clone(),
        conformance: true,
        deviation: None,
        evidence_refs: Vec::new(),
    };
    let rewritten = TinkerReport {
        specification_hash: "a-different-specification".into(),
        ..conforming.clone()
    };
    let events = vec![
        started(),
        DriveEvent::DirectiveSealed {
            directive: directive.clone(),
            at_ms: 100,
        },
        DriveEvent::DirectiveExecuted {
            report: rewritten,
            at_ms: 200,
        },
    ];
    // The rewritten specification is a replay divergence: control returns
    // to Playbook, the journal refuses.
    assert!(matches!(
        fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::ReplayDivergence(_)
    ));
    let lawful = vec![
        started(),
        DriveEvent::DirectiveSealed {
            directive,
            at_ms: 100,
        },
        DriveEvent::DirectiveExecuted {
            report: conforming,
            at_ms: 200,
        },
    ];
    let state = fold(&lawful, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.tinker_discipline(), (1, 0));
    assert!(state.directive().is_some());
}

#[test]
fn the_swarm_of_new_arms_replays_deterministically() {
    let directive = TtDirective {
        id: playbook_core::DirectiveId::new("dir-2").unwrap(),
        thinker: playbook_core::PersonnelId::new("thinker").unwrap(),
        tinker: playbook_core::PersonnelId::new("tinker").unwrap(),
        specification: "seal the build".into(),
        specification_hash: String::new(),
        expected_evidence: Vec::new(),
    }
    .seal()
    .unwrap();
    let directive_snapshot = directive.clone();
    let events = vec![
        started(),
        DriveEvent::GateDeclared {
            gate: gate("build"),
        },
        DriveEvent::GameContractSealed {
            contract: Box::new(contract("build", "game-2")),
            at_ms: 10,
        },
        DriveEvent::EvaluationDecided {
            at_ms: 20,
            branch: playbook_core::EvaluationBranch::Advance,
            reasons: vec!["measured_progress".into()],
        },
        DriveEvent::DirectiveSealed {
            directive,
            at_ms: 30,
        },
        DriveEvent::DirectiveExecuted {
            report: TinkerReport {
                directive_id: playbook_core::DirectiveId::new("dir-2").unwrap(),
                // Non-conformance is reported against the SEALED hash: the
                // tinker names its deviation, it never rewrites the spec.
                specification_hash: directive_snapshot.specification_hash.clone(),
                conformance: false,
                deviation: Some("invalid premise in the specification".into()),
                evidence_refs: Vec::new(),
            },
            at_ms: 40,
        },
        DriveEvent::CersRecovered {
            artefact_id: playbook_core::EvidenceId::new("cers-2").unwrap(),
            source: "failed_blitz".into(),
            confirmed_facts: 1,
            failed_assumptions: 2,
            reuse_hint: "blitz:invalid_candidates".into(),
            at_ms: 50,
        },
        DriveEvent::EvidenceAccepted {
            evidence: compiler_evidence(60),
        },
        completed(70),
    ];
    let policy = Policy::builtin();
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(
        fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        state
    );
    // The premise invalidation restarted the possession and was counted.
    assert_eq!(state.tinker_discipline(), (0, 1));
    assert_eq!(state.cers_artefacts().len(), 1);
}
