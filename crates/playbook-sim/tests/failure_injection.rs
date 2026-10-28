use playbook_sim::{
    open_drive, open_with, quota_pressure, roster, started_budget, task_contract, ArtefactId,
    ArtefactRef, Authority, CapabilityPlane, Down, DriveBudget, DriveEvent, DriveId, DriveStatus,
    Evidence, EvidenceClass, FailureClass, FixturePlane, Gate, PersonnelKind, PlayDefinition,
    PlayKind, PlaybookError, Policy, SideEffectClass, TaskIntegrity, ToolFailureKind,
};
use serde_json::json;

fn gate(id: &str, class: EvidenceClass) -> Gate {
    playbook_sim::gate_one(id, class)
}

fn env() -> Evidence {
    Evidence::seal(
        EvidenceClass::EnvironmentSnapshot,
        json!({"host": "lab"}),
        10,
    )
    .unwrap()
}

fn compiler(success: bool) -> Evidence {
    Evidence::seal(
        EvidenceClass::CompilerResult,
        json!({
            "gate_id": "implementation",
            "success": success,
            "command": "cargo check",
            "repository_hash": "abc"
        }),
        20,
    )
    .unwrap()
}

fn prepare(plane: FixturePlane, gates: Vec<Gate>) -> (Authority, FixturePlane) {
    open_drive(plane, gates).unwrap()
}

#[test]
fn rate_limit_asks_for_different_personnel() {
    let mut plane = FixturePlane::ready();
    plane.push_success(
        "inspect",
        env(),
        DriveBudget {
            tokens: 20,
            ..DriveBudget::zero()
        },
    );
    plane.push_failure("compile", ToolFailureKind::RateLimit);
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(drive.state().down(), Down::SecondDownLoss);
    assert_eq!(
        drive.state().failure_class(),
        Some(FailureClass::ToolFailure)
    );
    assert_eq!(
        drive.state().last_failure_kind(),
        Some(ToolFailureKind::RateLimit)
    );
    let ytg = drive.state().ytg_milli();
    drive.accept_evidence(narrative()).unwrap();
    assert_eq!(drive.state().ytg_milli(), ytg);
    let err = drive.try_operation("compile", &mut plane, 300).unwrap_err();
    assert!(matches!(err, PlaybookError::Inadmissible(_, _)));
    assert!(err.to_string().contains("rate limit"));
}

#[test]
fn authorization_failure_stops_repetition() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_failure("compile", ToolFailureKind::Authorization);
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    drive.accept_evidence(narrative()).unwrap();
    let err = drive.try_operation("compile", &mut plane, 300).unwrap_err();
    assert!(matches!(err, PlaybookError::AuthorizationStop(_)));
}

#[test]
fn timeout_and_tool_bug_leave_yards_unchanged() {
    for kind in [ToolFailureKind::Timeout, ToolFailureKind::ToolBug] {
        let mut plane = FixturePlane::ready();
        plane.push_success("inspect", env(), tiny());
        plane.push_failure("compile", kind);
        let (mut drive, mut plane) = prepare(
            plane,
            vec![gate("implementation", EvidenceClass::CompilerResult)],
        );
        let ytg = drive.state().ytg_milli();
        drive.step(&mut plane, 100).unwrap();
        drive.step(&mut plane, 200).unwrap();
        assert_eq!(drive.state().ytg_milli(), ytg);
        assert_eq!(drive.state().down(), Down::SecondDownLoss);
        assert_eq!(
            drive.state().failure_class(),
            Some(FailureClass::ToolFailure)
        );
    }
}

#[test]
fn malformed_and_invalid_evidence_do_not_move_ytg() {
    let (mut drive, _) = prepare(
        FixturePlane::ready(),
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    let ytg = drive.state().ytg_milli();
    let malformed = Evidence::seal(
        EvidenceClass::TestReport,
        json!({"gate_id": "unit_tests"}),
        1,
    )
    .unwrap();
    assert!(matches!(
        drive.accept_evidence(malformed).unwrap_err(),
        PlaybookError::Evidence(_)
    ));
    assert_eq!(drive.state().ytg_milli(), ytg);
    let mut tampered = compiler(true);
    tampered.id = playbook_sim::EvidenceId::new("not-the-digest").unwrap();
    assert_eq!(
        drive.accept_evidence(tampered).unwrap_err(),
        PlaybookError::DigestMismatch
    );
    assert_eq!(drive.state().ytg_milli(), ytg);
}

#[test]
fn compiler_failure_is_validation_loss() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_success("compile", compiler(false), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    let ytg = drive.state().ytg_milli();
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(drive.state().ytg_milli(), ytg);
    assert_eq!(drive.state().down(), Down::SecondDownLoss);
    assert_eq!(
        drive.state().failure_class(),
        Some(FailureClass::ValidationFailure)
    );
    assert!(!drive.claim_complete(300).unwrap());
}

#[test]
fn unresolved_mutation_blocks_retry_until_reconcile() {
    let mut policy = Policy::builtin();
    policy
        .allowed_side_effects
        .push(SideEffectClass::RepositoryMutation);
    policy.plays.push(PlayDefinition {
        kind: PlayKind::ExploitProvenTool,
        name: "commit".into(),
        personnel_kind: PersonnelKind::DeterministicProcedure,
        operation: "commit".into(),
        capability_id: "elci.cargo".into(),
        target_gate: None,
        side_effect: SideEffectClass::RepositoryMutation,
        expected_ytg_reduction_milli: 0,
        marginal_cost: tiny(),
        reliability_ppm: 900_000,
        survey: false,
        evidence_class: EvidenceClass::RepositoryMutation,
    });
    let mut plane = FixturePlane::ready();
    plane.push_unresolved("inspect", env(), tiny());
    plane.push_success(
        "commit",
        Evidence::seal(
            EvidenceClass::RepositoryMutation,
            json!({"confirmed": true}),
            30,
        )
        .unwrap(),
        tiny(),
    );
    let (mut drive, mut plane) = open_with(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
        policy,
    )
    .unwrap();
    drive.step(&mut plane, 100).unwrap();
    let err = drive.try_operation("commit", &mut plane, 200).unwrap_err();
    assert!(matches!(err, PlaybookError::Reconcile(_)));
    let key = drive
        .log()
        .iter()
        .find_map(|event| match event {
            DriveEvent::ToolInvoked { execution } => execution.idempotency_key.clone(),
            _ => None,
        })
        .unwrap();
    drive.reconcile(&key, true).unwrap();
    drive.try_operation("commit", &mut plane, 300).unwrap();
}

#[test]
fn environment_failure_is_its_own_class() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_failure("compile", ToolFailureKind::Environment);
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(
        drive.state().failure_class(),
        Some(FailureClass::EnvironmentFailure)
    );
    assert_eq!(drive.state().down(), Down::SecondDownLoss);
}

#[test]
fn unsupported_is_capability_absence_and_a_missing_class_is_play_action() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(
        drive.state().last_failure_kind(),
        Some(ToolFailureKind::Unsupported)
    );
    assert_eq!(
        drive.state().failure_class(),
        Some(FailureClass::CapabilityAbsence)
    );
    assert_eq!(drive.state().down(), Down::SecondDownLoss);

    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_success("compile", compiler(true), tiny());
    let gates = vec![
        playbook_sim::weighted_gate(
            "implementation",
            500_000,
            &[],
            EvidenceClass::CompilerResult,
        ),
        playbook_sim::weighted_gate(
            "windows",
            500_000,
            &["implementation"],
            EvidenceClass::Benchmark,
        ),
    ];
    let (mut drive, mut plane) = prepare(plane, gates);
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(drive.state().down(), Down::ThirdDownPlayAction);
    assert!(drive.state().requirement().is_some());
    assert!(drive.state().ytg_milli() > 0);
}

#[test]
fn duplicate_delivery_rolls_back_but_stale_capability_preserves_open_invocation() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.try_operation("inspect", &mut plane, 100).unwrap();
    let before = drive.state().clone();
    let err = drive.try_operation("inspect", &mut plane, 110).unwrap_err();
    assert!(matches!(err, PlaybookError::DuplicateEffect(_)));
    assert_eq!(drive.state(), &before);

    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    plane.discover().unwrap();
    let before = drive.state().clone();
    assert_eq!(
        drive.step(&mut plane, 100).unwrap_err(),
        PlaybookError::StaleCapability
    );
    assert_ne!(drive.state(), &before);
    assert!(drive.log().iter().any(|event| matches!(
        event,
        DriveEvent::ToolOutcomeRejected {
            result: None,
            failure: None,
            reason,
        } if reason.contains("capability snapshot is stale")
    )));
    let retry = drive.step(&mut plane, 101).unwrap_err();
    assert!(matches!(retry, PlaybookError::Reconcile(_)));
}

#[test]
fn budget_exhaustion_is_rejected_and_quota_pressure_reaches_fourth() {
    let (mut drive, _) = prepare(
        FixturePlane::ready(),
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    let before = drive.state().clone();
    let err = drive
        .pressure(
            DriveBudget {
                tokens: 50_000,
                ..DriveBudget::zero()
            },
            50,
        )
        .unwrap_err();
    assert_eq!(err, PlaybookError::BudgetExhausted);
    assert_eq!(drive.state(), &before);

    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.pressure(quota_pressure(1_000), 200).unwrap();
    assert_eq!(drive.state().down(), Down::FourthDown);
    assert!(drive.state().disposition().is_some());
}

#[test]
fn artefact_rejection_reopens_the_gate() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_success("compile", compiler(true), tiny());
    let (mut drive, mut plane) = prepare(
        plane,
        vec![gate("implementation", EvidenceClass::CompilerResult)],
    );
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert_eq!(drive.state().ytg_milli(), 0);
    let evidence_id = drive
        .log()
        .iter()
        .rev()
        .find_map(|event| match event {
            DriveEvent::ToolSucceeded { result } => {
                result.evidence.first().map(|item| item.id.clone())
            }
            _ => None,
        })
        .unwrap();
    let artefact_id = ArtefactId::new("impl-artefact").unwrap();
    drive
        .produce_artefact(ArtefactRef {
            id: artefact_id.clone(),
            digest: "abc".into(),
            validated: false,
            rejected: false,
            evidence_id: None,
        })
        .unwrap();
    drive
        .validate_artefact(artefact_id.clone(), evidence_id.clone())
        .unwrap();
    drive.reject_artefact(artefact_id, evidence_id).unwrap();
    assert!(drive.state().ytg_milli() > 0);
    assert_eq!(
        drive.state().failure_class(),
        Some(FailureClass::TaskRegression)
    );
    assert_eq!(drive.state().down(), Down::SecondDownLoss);
}

#[test]
fn field_goal_scores_with_yards_remaining_and_unsafe_work_is_abandoned() {
    let mut plane = FixturePlane::ready();
    plane.push_success("inspect", env(), tiny());
    plane.push_success("compile", compiler(true), tiny());
    let gates = vec![
        playbook_sim::weighted_gate(
            "implementation",
            400_000,
            &[],
            EvidenceClass::CompilerResult,
        ),
        playbook_sim::weighted_gate(
            "benchmark",
            600_000,
            &["implementation"],
            EvidenceClass::Benchmark,
        ),
    ];
    let (mut drive, mut plane) = prepare(plane, gates);
    drive.step(&mut plane, 100).unwrap();
    drive.step(&mut plane, 200).unwrap();
    assert!(drive.state().ytg_milli() > 0);
    drive
        .select_field_goal(playbook_sim::GateId::new("implementation").unwrap())
        .unwrap();
    assert_eq!(drive.state().status(), DriveStatus::FieldGoalScored);
    assert!(drive.state().ytg_milli() > 0);
    assert!(drive.state().ytg_milli() > 0);

    let mut unsafe_drive = Authority::start(
        DriveId::new("unsafe-drive").unwrap(),
        task_contract(TaskIntegrity::Unsafe),
        started_budget(100, 100),
        Policy::builtin(),
        roster(),
        1,
    )
    .unwrap();
    unsafe_drive.abandon("unsafe task").unwrap();
    assert_eq!(unsafe_drive.state().status(), DriveStatus::Abandoned);

    let mut sound = Authority::start(
        DriveId::new("sound-drive").unwrap(),
        task_contract(TaskIntegrity::Sound),
        started_budget(100, 100),
        Policy::builtin(),
        roster(),
        1,
    )
    .unwrap();
    assert!(!sound.claim_complete(2).unwrap());
    assert_eq!(sound.state().status(), DriveStatus::Active);
    sound
        .declare_gate(gate("implementation", EvidenceClass::CompilerResult))
        .unwrap();
    assert_eq!(
        sound.abandon("no doctrine").unwrap_err(),
        PlaybookError::DispositionMismatch
    );
    assert_eq!(sound.state().status(), DriveStatus::Active);
}

fn narrative() -> Evidence {
    Evidence::seal(
        EvidenceClass::NarrativeClaim,
        json!({"text": "implemented successfully"}),
        5,
    )
    .unwrap()
}

fn tiny() -> DriveBudget {
    DriveBudget {
        tokens: 1,
        monetary_cost_micros: 100,
        tool_calls: 1,
        ..DriveBudget::zero()
    }
}
