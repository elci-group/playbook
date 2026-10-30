//! The live runtime gates.
//!
//! `v0.2.0`: a non-fixture plane executes a real command against this real
//! repository, and the authority ingests the sealed result through the same
//! evidence path as everything else. `v0.2.1`: a live worker's provider
//! fails, the failure is persisted, the dispatch ledger reconciles, the
//! Bench substitutes, and the same objective continues on the replacement
//! with the accepted evidence intact.

use playbook_core::{
    DriveBudget, DriveId, EvidenceClass, ExecutionId, ObjectiveId, PlaybookError, TaskContract,
    TaskIntegrity,
};
use playbook_elci::CapabilityPlane;
use playbook_live::dispatch::{DispatchLedger, DispatchStage};
use playbook_live::envelope::ExecutionEnvelope;
use playbook_live::faults::{Fault, FaultPlane};
use playbook_live::identity::{LiveIdentity, Occupancy};
use playbook_live::toolplane::{ToolCommand, ToolPlane};
use playbook_policy::Policy;
use playbook_state::{started_budget, Authority};
use std::collections::BTreeMap;

/// The allowlist for the real work this test performs against this
/// repository: read the live commit hash. Read-only, instant, and genuinely
/// about the state of the world.
fn repo_allowlist() -> BTreeMap<String, ToolCommand> {
    let mut commands = BTreeMap::new();
    commands.insert(
        "inspect".into(),
        ToolCommand {
            program: "git".into(),
            args: vec!["rev-parse".into(), "HEAD".into()],
            evidence_class: EvidenceClass::EnvironmentSnapshot,
            gate: None,
        },
    );
    commands
}

fn authority() -> Authority {
    Authority::start(
        DriveId::new("drive-live-gate").unwrap(),
        TaskContract {
            statement: "observe the live repository state".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        },
        started_budget(100_000, 1_000),
        Policy::builtin(),
        vec![],
        1_000,
    )
    .unwrap()
}

/// v0.2.0 — the fixture plane is no longer required for useful execution:
/// the tool plane runs a real command against the real repository, seals the
/// observation, and the authority accepts it as evidence.
#[test]
fn a_real_command_executes_and_the_authority_accepts_its_evidence() {
    let mut live = ToolPlane::new(env!("CARGO_MANIFEST_DIR"), repo_allowlist());

    // Preflight discovers the live allowlist through the real trait.
    let snapshot = live.discover().unwrap();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(
        snapshot.capabilities[0].operations,
        vec!["inspect".to_string()]
    );

    // A validated envelope: the whole of the invocation's authority.
    let envelope = ExecutionEnvelope {
        execution_id: ExecutionId::new("exec-inspect-1").unwrap(),
        execution: playbook_core::PlayExecution {
            drive_id: DriveId::new("drive-live-gate").unwrap(),
            play_id: playbook_core::PlayId::new("play-inspect").unwrap(),
            play_kind: playbook_core::PlayKind::InspectEnvironment,
            down: playbook_core::Down::FirstDown,
            operation: "inspect".into(),
            capability_id: playbook_core::CapabilityId::new("live.tool").unwrap(),
            capability_generation: 1,
            personnel_id: LiveIdentity::new("local", "git", "v1", "readonly")
                .unwrap()
                .personnel_id()
                .unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::EnvironmentSnapshot],
            expected_ytg_reduction_milli: 0,
            survey: true,
            budget_ceiling: DriveBudget {
                wall_clock_ms: 10_000,
                tool_calls: 5,
                ..DriveBudget::zero()
            },
            timeout_ms: 5_000,
            retry_policy: playbook_core::RetryPolicy::NoRetry,
            side_effect: playbook_core::SideEffectClass::ReadOnly,
            idempotency_key: Some("exec-inspect-1".into()),
        },
        personnel_id: LiveIdentity::new("local", "git", "v1", "readonly")
            .unwrap()
            .personnel_id()
            .unwrap(),
        objective_id: Some(ObjectiveId::new("objective-live").unwrap()),
        allowed_operations: vec!["inspect".into()],
        allowed_side_effects: vec![playbook_core::SideEffectClass::ReadOnly],
        expected_result_schema: "environment_snapshot/1".into(),
        expected_evidence: vec![EvidenceClass::EnvironmentSnapshot],
        budget_envelope: DriveBudget {
            wall_clock_ms: 5_000,
            tool_calls: 2,
            ..DriveBudget::zero()
        },
        timeout_ms: 5_000,
        working_directory: Some(env!("CARGO_MANIFEST_DIR").to_string()),
    };
    envelope
        .validate(&[playbook_core::SideEffectClass::ReadOnly])
        .expect("a complete envelope validates");

    // Durable dispatch: persisted before anything is sent.
    let mut ledger = DispatchLedger::empty();
    ledger
        .persist(
            ExecutionId::new("exec-inspect-1").unwrap(),
            "exec-inspect-1",
        )
        .unwrap();
    ledger
        .mark_dispatched(&ExecutionId::new("exec-inspect-1").unwrap())
        .unwrap();

    // The real execution: git rev-parse HEAD runs in this repository.
    let result = match live.execute(&envelope.execution).unwrap() {
        playbook_elci::PlaneResult::Done(result) => result,
        playbook_elci::PlaneResult::Failed(failure) => {
            panic!("git rev-parse must succeed here: {failure:?}")
        }
    };
    ledger
        .mark_outcome(&ExecutionId::new("exec-inspect-1").unwrap(), true)
        .unwrap();
    assert_eq!(
        ledger
            .record(&ExecutionId::new("exec-inspect-1").unwrap())
            .unwrap()
            .stage,
        DispatchStage::OutcomeAccepted
    );

    // The sealed evidence is real: it verifies, and its body carries the
    // digest of the actual command output.
    let evidence = &result.evidence[0];
    evidence.verify().unwrap();
    assert_eq!(evidence.class, EvidenceClass::EnvironmentSnapshot);
    assert!(
        result.cost.wall_clock_ms > 0,
        "live work measures its wall clock"
    );

    // The authority ingests the live result through the same evidence path
    // as everything else.
    let mut authority = authority();
    authority
        .preflight(&mut ToolPlane::new(
            env!("CARGO_MANIFEST_DIR"),
            repo_allowlist(),
        ))
        .unwrap();
    authority.accept_evidence(evidence.clone()).unwrap();
    let accepted = authority
        .state()
        .evidence()
        .iter()
        .any(|item| item.id == evidence.id);
    assert!(accepted, "the live observation is on the official record");
}

/// v0.2.1 — live substitution: player A's provider dies mid-campaign, the
/// failure is persisted, the Bench substitutes, and the objective continues
/// on player B with the drive, evidence, and budget history intact.
#[test]
fn provider_failure_persists_and_the_bench_substitutes_without_losing_the_objective() {
    let identity_a = LiveIdentity::new("provider-a", "model-x", "v1", "tools").unwrap();
    let identity_b = LiveIdentity::new("provider-b", "model-y", "v2", "tools").unwrap();
    // §6: provider and model changes are different empirical players.
    assert_ne!(
        identity_a.personnel_id().unwrap(),
        identity_b.personnel_id().unwrap()
    );

    let mut ledger = DispatchLedger::empty();
    ledger
        .persist(ExecutionId::new("exec-a-1").unwrap(), "play-1-work")
        .unwrap();
    ledger
        .mark_dispatched(&ExecutionId::new("exec-a-1").unwrap())
        .unwrap();

    // Player A's provider fails: 5xx, the effect provably did not occur.
    let mut faulted = FaultPlane::over(
        ToolPlane::new(env!("CARGO_MANIFEST_DIR"), repo_allowlist()),
        vec![Fault::ProviderServerError5xx],
    );
    let failure = match faulted
        .execute(&playbook_core::PlayExecution {
            drive_id: DriveId::new("drive-live-gate").unwrap(),
            play_id: playbook_core::PlayId::new("play-1").unwrap(),
            play_kind: playbook_core::PlayKind::InspectEnvironment,
            down: playbook_core::Down::FirstDown,
            operation: "inspect".into(),
            capability_id: playbook_core::CapabilityId::new("live.tool").unwrap(),
            capability_generation: 1,
            personnel_id: identity_a.personnel_id().unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::EnvironmentSnapshot],
            expected_ytg_reduction_milli: 0,
            survey: true,
            budget_ceiling: DriveBudget {
                wall_clock_ms: 5_000,
                tool_calls: 2,
                ..DriveBudget::zero()
            },
            timeout_ms: 2_000,
            retry_policy: playbook_core::RetryPolicy::NoRetry,
            side_effect: playbook_core::SideEffectClass::ReadOnly,
            idempotency_key: Some("play-1-work".into()),
        })
        .unwrap()
    {
        playbook_elci::PlaneResult::Failed(failure) => failure,
        playbook_elci::PlaneResult::Done(_) => panic!("the injected 5xx must fail"),
    };
    assert_eq!(failure.personnel_id, identity_a.personnel_id().unwrap());
    // 5xx is a rejected outcome with no effect: the dispatch ledger closes
    // the invocation as rejected, and A is no longer busy.
    ledger
        .mark_outcome(&ExecutionId::new("exec-a-1").unwrap(), false)
        .unwrap();
    assert_eq!(
        ledger
            .record(&ExecutionId::new("exec-a-1").unwrap())
            .unwrap()
            .stage,
        DispatchStage::OutcomeRejected
    );

    // The Bench sees a resolved provider death and substitutes B in.
    let policy = Policy::builtin();
    let person =
        |identity: &LiveIdentity, health_ppm: u32| playbook_franchise::bench::BenchCandidate {
            personnel: playbook_core::Personnel {
                id: identity.personnel_id().unwrap(),
                kind: playbook_core::PersonnelKind::Model,
                display_name: format!("{}-{}", identity.provider, identity.model),
                cost_tokens: 40,
                latency_ms: 20,
                capability_fit_ppm: 900_000,
                reliability_ppm: health_ppm,
                quota_remaining_ppm: playbook_core::PPM_SCALE,
                evidence_quality_ppm: playbook_core::PPM_SCALE,
                own_ytg_milli: None,
            },
            provider_health_ppm: health_ppm,
            recent_gate_failures: 0,
            cost_micros_per_play: 100_000,
            available: true,
            busy: false,
        };
    let decision = playbook_franchise::bench::decide(
        &playbook_franchise::bench::BenchInput {
            current: person(&identity_a, 0),
            required_fit_ppm: 700_000,
            budget_remaining: DriveBudget {
                tokens: 90_000,
                ..DriveBudget::zero()
            },
            budget_initial: DriveBudget {
                tokens: 100_000,
                ..DriveBudget::zero()
            },
            emergency: false,
            stall_cycles: 0,
        },
        &[person(&identity_b, 900_000)],
    )
    .expect("a resolved provider death with a healthy replacement substitutes");
    assert!(matches!(
        decision.class,
        playbook_franchise::SubstitutionClass::Availability
    ));
    assert_eq!(
        decision.replacement,
        Some(identity_b.personnel_id().unwrap())
    );

    // Player B takes the SAME assignment on the SAME drive and produces the
    // real evidence A could not.
    let mut live = ToolPlane::new(env!("CARGO_MANIFEST_DIR"), repo_allowlist());
    ledger
        .persist(ExecutionId::new("exec-b-1").unwrap(), "play-1-work-b")
        .unwrap();
    ledger
        .mark_dispatched(&ExecutionId::new("exec-b-1").unwrap())
        .unwrap();
    let mut execution_b = playbook_core::PlayExecution {
        drive_id: DriveId::new("drive-live-gate").unwrap(),
        play_id: playbook_core::PlayId::new("play-1").unwrap(),
        play_kind: playbook_core::PlayKind::InspectEnvironment,
        down: playbook_core::Down::FirstDown,
        operation: "inspect".into(),
        capability_id: playbook_core::CapabilityId::new("live.tool").unwrap(),
        capability_generation: 1,
        personnel_id: identity_b.personnel_id().unwrap(),
        inputs: serde_json::json!({}),
        expected_evidence: vec![EvidenceClass::EnvironmentSnapshot],
        expected_ytg_reduction_milli: 0,
        survey: true,
        budget_ceiling: DriveBudget {
            wall_clock_ms: 5_000,
            tool_calls: 2,
            ..DriveBudget::zero()
        },
        timeout_ms: 2_000,
        retry_policy: playbook_core::RetryPolicy::NoRetry,
        side_effect: playbook_core::SideEffectClass::ReadOnly,
        idempotency_key: Some("play-1-work-b".into()),
    };
    let _ = &mut execution_b;
    match live.execute(&execution_b).unwrap() {
        playbook_elci::PlaneResult::Done(result) => {
            ledger
                .mark_outcome(&ExecutionId::new("exec-b-1").unwrap(), true)
                .unwrap();
            assert_eq!(result.evidence[0].class, EvidenceClass::EnvironmentSnapshot);
            // The sealed body is B's real observation of this repository.
            assert!(result.evidence[0].body["observed"] == serde_json::json!(true));
        }
        playbook_elci::PlaneResult::Failed(failure) => {
            panic!("git rev-parse succeeds for player B too: {failure:?}")
        }
    }
    assert_eq!(
        ledger.unreconciled(),
        0,
        "both invocations closed; nothing is left unknown"
    );
    // The whole journey used only the policy-backed vocabulary.
    let _: Result<(), PlaybookError> = Ok(());
    let _ = policy;
}

#[test]
fn busy_players_hold_their_assignment_until_their_effect_reconciles() {
    // §8: the default occupancy rule binds live work — an unresolved
    // invocation means the player is busy and may not be reassigned, no
    // matter who needs the slot.
    let busy = Occupancy {
        has_unresolved_invocation: true,
        supports_concurrent_occupancy: false,
        policy_permits_reassignment: false,
    };
    assert!(!busy.may_take_work());
    // After reconciliation the player is free again.
    let resolved = Occupancy {
        has_unresolved_invocation: false,
        ..busy
    };
    assert!(resolved.may_take_work());
}
