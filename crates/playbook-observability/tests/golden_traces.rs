//! §9 golden traces.
//!
//! Golden traces prevent structural telemetry from silently drifting;
//! intentional schema changes require explicit fixture regeneration and
//! review. Each fixture under `fixtures/golden/` is the pretty-serialised
//! span projection of one canonical, hand-built drive journal — nine
//! scenarios covering every arm of the projection: the successful play, the
//! rejected play, the tactical transition, the substitution, the huddle,
//! the blitz fan-out, the CERS recovery, the lastdown evaluation, and the
//! sealed game contract.
//!
//! Regenerate with:
//!
//! ```text
//! cargo test -p playbook-observability --offline --test golden_traces -- \
//!   --ignored regenerate_golden_fixtures
//! ```
//!
//! The checked-in tests regenerate every scenario in-process and demand
//! byte equality with the stored fixture (the drift guard), round-trip
//! every span through the strict `SpanNode` schema, and hold every fixture
//! to single-rooted, acyclic parentage discipline.

use std::path::{Path, PathBuf};

use playbook_blitz::{
    BlitzClass, BlitzTrigger, DefensivePressure, Severity, TargetPackage, VerificationKind,
    WorkerSlot,
};
use playbook_core::{
    BarrierMode, BlitzId, CandidateId, CapabilityId, Constraint, DecisionRecord, Down, DriveBudget,
    DriveId, EvaluationBranch, EventId, Evidence, EvidenceClass, EvidenceId, EvidencePolicy,
    GameContract, GameId, GateId, HuddleBudget, HuddleId, LossCondition, LossConditionKind,
    PersonnelId, PlayExecution, PlayId, PlayKind, PlayResult, PlayStatus, RetryPolicy,
    SideEffectClass, TaskContract, TaskIntegrity, ToolFailure, ToolFailureKind, WinCondition,
    WinConditionId, WinConditionKind, WorkerId,
};
use playbook_events::DriveEvent;
use playbook_huddle::{
    DisclosureProfile, HuddleOutcome, HuddleParticipant, HuddleReport, HuddleRole, HuddleScope,
    HuddleTrigger, HuddleTriggerKind, SharedStateDelta,
};
use playbook_observability::SpanNode;

/// The nine canonical scenarios, in doctrine order.
const SCENARIOS: [&str; 9] = [
    "successful_play",
    "rejected_play",
    "tactical_transition",
    "substitution",
    "huddle",
    "blitz",
    "cers_recovery",
    "lastdown_evaluation",
    "contract_sealed",
];

/// The scenario's stable drive identity. Span ids embed it, so a fixture
/// catches any drift in naming as bytes, not conventions.
fn drive_id(name: &str) -> String {
    format!("drive-golden-{}", name.replace('_', "-"))
}

/// The golden fixture root, resolved from the crate manifest; the crate
/// carries no fixture path dependency beyond this relative join.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

/// One scenario's fixture path.
fn fixture_path(name: &str) -> PathBuf {
    fixture_dir().join(format!("{name}.json"))
}

/// The canonical account for a scenario: the span projection of its journal,
/// pretty serialised. This is the exact byte shape the fixture stores.
fn render(name: &str) -> String {
    let events = build_scenario(name).unwrap_or_else(|| panic!("unknown golden scenario {name}"));
    serde_json::to_string_pretty(&playbook_observability::spans(&drive_id(name), &events)).unwrap()
}

/// The nine canonical scenarios, matched by name. `None` names an unknown
/// scenario; the suite never invents one silently.
fn build_scenario(name: &str) -> Option<Vec<DriveEvent>> {
    match name {
        "successful_play" => Some(successful_play(&drive_id(name))),
        "rejected_play" => Some(rejected_play(&drive_id(name))),
        "tactical_transition" => Some(tactical_transition()),
        "substitution" => Some(substitution()),
        "huddle" => Some(huddle()),
        "blitz" => Some(blitz()),
        "cers_recovery" => Some(cers_recovery()),
        "lastdown_evaluation" => Some(lastdown_evaluation()),
        "contract_sealed" => Some(contract_sealed()),
        _ => None,
    }
}

/// Scenario 1 — the successful play: selection, invocation, a sealed
/// compiler result, and the evidence accepted against the gate. The
/// structural account is drive → play → tool; the result and the evidence
/// ride the journal without projecting spans of their own.
fn successful_play(drive: &str) -> Vec<DriveEvent> {
    let drive_id = DriveId::new(drive).unwrap();
    let compiler_evidence = Evidence::seal(
        EvidenceClass::CompilerResult,
        serde_json::json!({"outcome": "pass", "diagnostics": []}),
        20,
    )
    .unwrap();
    let play_id = PlayId::new("play-successful-1").unwrap();
    vec![
        DriveEvent::DriveStarted {
            drive_id: drive_id.clone(),
            task: TaskContract {
                statement: "land one verified play".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            budget: DriveBudget {
                tokens: 100_000,
                monetary_cost_micros: 1_000_000,
                wall_clock_ms: 60_000,
                tool_calls: 20,
                inference_calls: 20,
                retry_count: 3,
                context_capacity: 200_000,
                provider_quota: 100,
            },
            at_ms: 0,
        },
        DriveEvent::PlaySelected {
            decision: Box::new(DecisionRecord {
                policy_version: "golden".into(),
                input_state_hash: "state-successful-play".into(),
                classification: Down::FirstDown,
                selected_play: PlayKind::RunDeterministicTest,
                selected_operation: "compile.check".into(),
                selected_personnel: None,
                candidate_plays: vec!["compile.check".into()],
                rejection_reasons: Vec::new(),
                evidence_refs: Vec::new(),
            }),
        },
        DriveEvent::ToolInvoked {
            execution: PlayExecution {
                drive_id,
                play_id: play_id.clone(),
                play_kind: PlayKind::RunDeterministicTest,
                operation: "compile.check".into(),
                down: Down::FirstDown,
                capability_id: CapabilityId::new("elci.compiler").unwrap(),
                capability_generation: 1,
                personnel_id: PersonnelId::new("builder").unwrap(),
                inputs: serde_json::json!({"target": "golden"}),
                expected_evidence: vec![EvidenceClass::CompilerResult],
                expected_ytg_reduction_milli: 2_000,
                survey: false,
                budget_ceiling: DriveBudget {
                    tokens: 5_000,
                    ..DriveBudget::zero()
                },
                timeout_ms: 1_000,
                retry_policy: RetryPolicy::NoRetry,
                side_effect: SideEffectClass::ReadOnly,
                idempotency_key: Some("golden-successful-play-1".into()),
            },
        },
        DriveEvent::ToolSucceeded {
            result: Box::new(PlayResult {
                status: PlayStatus::Succeeded,
                play_id,
                operation: "compile.check".into(),
                output_refs: vec!["artefacts/golden-1".into()],
                evidence: vec![compiler_evidence.clone()],
                artefact_refs: Vec::new(),
                cost: DriveBudget {
                    tokens: 120,
                    monetary_cost_micros: 900,
                    ..DriveBudget::zero()
                },
                duration_ms: 250,
                diagnostics: Vec::new(),
                side_effects: Vec::new(),
                at_ms: 20,
            }),
        },
        DriveEvent::EvidenceAccepted {
            evidence: compiler_evidence,
        },
    ]
}

/// Scenario 2 — the rejected play: the invocation fails with named
/// diagnostics. The tool span is parented to the open play, so the scenario
/// selects a play first — a parentless tool span would be an orphan, and
/// the golden suite refuses orphans by construction.
fn rejected_play(drive: &str) -> Vec<DriveEvent> {
    let drive_id = DriveId::new(drive).unwrap();
    let play_id = PlayId::new("play-rejected-1").unwrap();
    vec![
        DriveEvent::PlaySelected {
            decision: Box::new(DecisionRecord {
                policy_version: "golden".into(),
                input_state_hash: "state-rejected-play".into(),
                classification: Down::ThirdDownPass,
                selected_play: PlayKind::TargetedInference,
                selected_operation: "patch.apply".into(),
                selected_personnel: None,
                candidate_plays: vec!["patch.apply".into()],
                rejection_reasons: Vec::new(),
                evidence_refs: Vec::new(),
            }),
        },
        DriveEvent::ToolInvoked {
            execution: PlayExecution {
                drive_id,
                play_id: play_id.clone(),
                play_kind: PlayKind::TargetedInference,
                operation: "patch.apply".into(),
                down: Down::ThirdDownPass,
                capability_id: CapabilityId::new("elci.patch").unwrap(),
                capability_generation: 2,
                personnel_id: PersonnelId::new("specialist").unwrap(),
                inputs: serde_json::json!({"patch": "golden-2"}),
                expected_evidence: vec![EvidenceClass::CompilerResult],
                expected_ytg_reduction_milli: 4_000,
                survey: false,
                budget_ceiling: DriveBudget {
                    tokens: 8_000,
                    ..DriveBudget::zero()
                },
                timeout_ms: 2_000,
                retry_policy: RetryPolicy::TransientOnly { limit: 1 },
                side_effect: SideEffectClass::LocalMutation,
                idempotency_key: Some("golden-rejected-play-1".into()),
            },
        },
        DriveEvent::ToolFailed {
            failure: ToolFailure {
                play_id,
                play_kind: PlayKind::TargetedInference,
                operation: "patch.apply".into(),
                capability_id: CapabilityId::new("elci.patch").unwrap(),
                personnel_id: PersonnelId::new("specialist").unwrap(),
                kind: ToolFailureKind::InvalidInput,
                diagnostics: vec![
                    "the patch refuses to apply: context drifted".into(),
                    "offending hunk: line 41".into(),
                ],
                cost: DriveBudget {
                    tokens: 400,
                    monetary_cost_micros: 3_000,
                    ..DriveBudget::zero()
                },
                at_ms: 40,
                idempotency_key: Some("golden-rejected-play-1".into()),
                side_effects: Vec::new(),
            },
        },
    ]
}

/// Scenario 3 — the tactical transition: a bunch is admitted, completes its
/// objective with measured YTG movement, then is named in an opposing
/// counter. Three spans, all children of the drive.
fn tactical_transition() -> Vec<DriveEvent> {
    vec![
        DriveEvent::TacticAdmitted {
            tactic: "bunch".into(),
            at_ms: 100,
        },
        DriveEvent::TacticCompleted {
            tactic: "bunch".into(),
            success: true,
            ytg_before_milli: 10_000,
            ytg_after_milli: 6_000,
            tokens_spent: 800,
            at_ms: 200,
        },
        DriveEvent::TacticCountered {
            tactic: "bunch".into(),
            counter: "prevent".into(),
            at_ms: 300,
        },
    ]
}

/// Scenario 4 — the substitution: a validator comes off for the named bench
/// replacement, and a rusher comes off with no qualified replacement. Both
/// branches of the projection are pinned: a missing replacement is recorded
/// as `none`, never silently dropped.
fn substitution() -> Vec<DriveEvent> {
    vec![
        DriveEvent::PlayerSubstituted {
            at_ms: 100,
            class: "validator".into(),
            out: PersonnelId::new("starter-validator").unwrap(),
            replacement: Some(PersonnelId::new("bench-validator").unwrap()),
            reasons: vec![
                "quota exhausted".into(),
                "reliability below threshold".into(),
            ],
        },
        DriveEvent::PlayerSubstituted {
            at_ms: 200,
            class: "rusher".into(),
            out: PersonnelId::new("starter-rusher").unwrap(),
            replacement: None,
            reasons: vec!["no qualified replacement on the bench".into()],
        },
    ]
}

/// Scenario 5 — the huddle: request, admission, one report, committed
/// outcome, with payload shapes following the observability unit doctrine.
/// The account links the report to the open huddle span; the committed
/// outcome stays journal-side, because closure is ledger truth, not a span.
fn huddle() -> Vec<DriveEvent> {
    let huddle_id = HuddleId::new("h-golden-1").unwrap();
    vec![
        DriveEvent::HuddleRequested {
            huddle_id: huddle_id.clone(),
            scope: HuddleScope::Position,
            trigger: HuddleTrigger {
                kind: HuddleTriggerKind::Failure,
                reason_code: "repeated_failed_play".into(),
                evidence_refs: vec![EvidenceId::new("e-golden-1").unwrap()],
                originating_event: EventId::new("evt-golden-1").unwrap(),
            },
            at_ms: 100,
            debt: 40,
            rework_avoided: 600,
        },
        DriveEvent::HuddleAdmitted {
            huddle_id: huddle_id.clone(),
            budget: HuddleBudget::zero(),
            barrier: BarrierMode::Soft,
            participants: vec![HuddleParticipant {
                personnel_id: PersonnelId::new("worker").unwrap(),
                role: HuddleRole::Contributor,
                disclosure_profile: DisclosureProfile {
                    include_gates: true,
                    include_peer_reports: true,
                    include_strategic_state: false,
                    max_evidence_refs: 8,
                },
            }],
        },
        DriveEvent::HuddleReportSubmitted {
            huddle_id: huddle_id.clone(),
            report: Box::new(HuddleReport {
                participant: PersonnelId::new("worker").unwrap(),
                verified_progress: Vec::new(),
                blockers: Vec::new(),
                changed_assumptions: Vec::new(),
                dependency_changes: Vec::new(),
                contradictions: Vec::new(),
                proposed_action: None,
                confidence_milli: 900,
            }),
        },
        DriveEvent::HuddleOutcomeCommitted {
            huddle_id: huddle_id.clone(),
            outcome: Box::new(HuddleOutcome {
                huddle_id,
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
            }),
        },
    ]
}

/// Scenario 6 — the blitz fan-out: two independently provisioned workers as
/// siblings, the first verified candidate named as winner, the loser's
/// cancellation on the record. Payload shapes follow the campaign suite's
/// blitz doctrine.
fn blitz() -> Vec<DriveEvent> {
    let blitz_id = BlitzId::new("b-golden").unwrap();
    let worker = |worker: &str, personnel: &str, provider: &str, domain: &str| WorkerSlot {
        worker_id: WorkerId::new(worker).unwrap(),
        personnel_id: PersonnelId::new(personnel).unwrap(),
        provider: provider.into(),
        model_family: "fam".into(),
        failure_domains: vec![domain.into()],
        wave: 1,
        dispatched_at_ms: 0,
        deadline_ms: 1_000,
    };
    vec![
        DriveEvent::BlitzProposed {
            blitz_id: blitz_id.clone(),
            trigger: BlitzTrigger {
                class: BlitzClass::Strategic,
                reason_code: "golden".into(),
                evidence_refs: Vec::new(),
                originating_event: EventId::new("evt-golden-blitz").unwrap(),
            },
            target: TargetPackage {
                problem_statement: "restore the failing component".into(),
                evidence_refs: Vec::new(),
                artefact_refs: Vec::new(),
                constraints: Vec::new(),
                forbidden_actions: Vec::new(),
                expected_output: "a verified patch".into(),
                win_condition: "the component passes its gates".into(),
                verification_gates: Vec::new(),
                deadline_ms: 1_000,
                target_hash: "golden-target".into(),
            },
            pressure: DefensivePressure {
                severity: Severity::Critical,
                deadline_ms: Some(1_000),
                deterioration_rate_ppm: 0,
                blast_radius_ppm: 0,
                solver_uncertainty_ppm: 0,
                containment_confidence_ppm: 0,
                recovery_confidence_ppm: 0,
                resource_decay_ppm: 0,
                expected_delay_loss: 0,
            },
            at_ms: 0,
        },
        DriveEvent::BlitzWorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker("w-golden-1", "p-golden-1", "alpha", "dc-a"),
            effective_parallelism_milli: 1_000,
        },
        DriveEvent::BlitzWorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker("w-golden-2", "p-golden-2", "beta", "dc-b"),
            effective_parallelism_milli: 2_000,
        },
        DriveEvent::BlitzCandidateVerified {
            blitz_id: blitz_id.clone(),
            candidate_id: CandidateId::new("c-golden-1").unwrap(),
            passed: vec![VerificationKind::Compilation],
        },
        DriveEvent::BlitzWinnerSelected {
            blitz_id: blitz_id.clone(),
            candidate_id: CandidateId::new("c-golden-1").unwrap(),
            worker_id: WorkerId::new("w-golden-1").unwrap(),
            at_ms: 10,
        },
        DriveEvent::BlitzWorkerCancelled {
            blitz_id,
            worker_id: WorkerId::new("w-golden-2").unwrap(),
            avoided_tokens: 900,
            avoided_cost_microunits: 9_000,
        },
    ]
}

/// Scenario 7 — the CERS recovery: a prior artefact re-enters the drive from
/// the cache. The span records the artefact identity and its source; the
/// confirmed and failed counts stay journal-side.
fn cers_recovery() -> Vec<DriveEvent> {
    vec![DriveEvent::CersRecovered {
        artefact_id: EvidenceId::new("art-golden-1").unwrap(),
        source: "prior_drive_cache".into(),
        confirmed_facts: 12,
        failed_assumptions: 2,
        reuse_hint: "resume from the recovered artefact".into(),
        at_ms: 150,
    }]
}

/// Scenario 8 — the lastdown evaluation: the evaluation branch names
/// Lastdown with its reasons on the record. The span joins the reasons with
/// a pipe; the fixture pins that rendering.
fn lastdown_evaluation() -> Vec<DriveEvent> {
    vec![DriveEvent::EvaluationDecided {
        at_ms: 400,
        branch: EvaluationBranch::Lastdown,
        reasons: vec![
            "quota runway below the punt floor".into(),
            "context capacity nearly spent".into(),
        ],
    }]
}

/// Scenario 9 — the sealed contract: a real sealed `GameContract`, hashed
/// over its canonical bytes and re-verified before journaling. The span
/// records the digest and the count of mandatory win conditions; the
/// advisory condition is deliberately not mandatory, and the fixture pins
/// that it does not inflate the count.
fn contract_sealed() -> Vec<DriveEvent> {
    let contract = GameContract {
        game_id: GameId::new("game-golden").unwrap(),
        objective: TaskContract {
            statement: "repair the parser and prove it".into(),
            workload_class: "engineering".into(),
            constraints: vec![Constraint {
                key: "offline".into(),
                value: "true".into(),
            }],
            integrity: TaskIntegrity::Sound,
        },
        win_conditions: vec![
            WinCondition {
                id: WinConditionId::new("win-build").unwrap(),
                kind: WinConditionKind::GateSatisfied {
                    gate: GateId::new("build").unwrap(),
                },
                description: "the build gate is satisfied".into(),
                mandatory: true,
            },
            WinCondition {
                id: WinConditionId::new("win-tests").unwrap(),
                kind: WinConditionKind::EvidencePresent {
                    class: EvidenceClass::TestReport,
                    minimum: 2,
                },
                description: "two sealed test reports exist".into(),
                mandatory: true,
            },
            WinCondition {
                id: WinConditionId::new("win-advisory").unwrap(),
                kind: WinConditionKind::ArtefactValidated { minimum: 1 },
                description: "a validated artefact exists".into(),
                mandatory: false,
            },
        ],
        loss_conditions: vec![
            LossCondition {
                id: WinConditionId::new("loss-budget").unwrap(),
                kind: LossConditionKind::BudgetExhausted,
                description: "the game budget was exceeded".into(),
            },
            LossCondition {
                id: WinConditionId::new("loss-deadline").unwrap(),
                kind: LossConditionKind::DeadlinePassed,
                description: "the deadline passed without a win".into(),
            },
        ],
        budget: DriveBudget {
            tokens: 100_000,
            ..DriveBudget::zero()
        },
        evidence_policy: EvidencePolicy {
            accepted_classes: vec![EvidenceClass::CompilerResult, EvidenceClass::TestReport],
        },
        deadline_ms: Some(600_000),
        predecessor_hash: None,
        contract_hash: String::new(),
    }
    .seal()
    .unwrap();
    contract.verify().unwrap();
    vec![DriveEvent::GameContractSealed {
        contract: Box::new(contract),
        at_ms: 500,
    }]
}

/// The regeneration ritual: overwrites each fixture with the freshly
/// generated account. Run explicitly; every diff it produces afterwards is
/// a schema change and must be reviewed, not waved through.
#[test]
#[ignore]
fn regenerate_golden_fixtures() {
    std::fs::create_dir_all(fixture_dir()).unwrap();
    for name in SCENARIOS {
        std::fs::write(fixture_path(name), render(name)).unwrap();
    }
}

/// The drift guard: every fixture equals the freshly generated account byte
/// for byte. Generation is asserted deterministic first (two independent
/// passes agree) so a flaky generator can never masquerade as a reviewed
/// schema change. Each span then round-trips through the strict schema.
#[test]
fn golden_fixtures_match_fresh_generation_byte_for_byte() {
    for name in SCENARIOS {
        let events =
            build_scenario(name).unwrap_or_else(|| panic!("unknown golden scenario {name}"));
        let fresh = playbook_observability::spans(&drive_id(name), &events);
        let first = serde_json::to_string_pretty(&fresh).unwrap();
        let second =
            serde_json::to_string_pretty(&playbook_observability::spans(&drive_id(name), &events))
                .unwrap();
        assert_eq!(first, second, "{name} does not generate deterministically");
        let stored = std::fs::read_to_string(fixture_path(name))
            .unwrap_or_else(|error| panic!("{name} fixture unreadable: {error}"));
        assert_eq!(
            first, stored,
            "{name} drifted from its golden fixture; regenerate and review"
        );
        // Strict deserialisation: unknown fields are refused by the schema,
        // and the parsed account equals the generated one span for span.
        let parsed: Vec<SpanNode> = serde_json::from_str(&stored)
            .unwrap_or_else(|error| panic!("{name} fixture does not parse: {error}"));
        assert_eq!(parsed, fresh, "{name} deserialises to a different account");
        // Per-span round trip: serialise, deserialise, refuse drift.
        for span in &fresh {
            let line = serde_json::to_string(span).unwrap();
            let back: SpanNode = serde_json::from_str(&line).unwrap();
            assert_eq!(
                &back, span,
                "{} lost the serialisation round trip",
                span.span_id
            );
        }
    }
}

/// Cross-fixture invariants: every golden fixture is a single-rooted,
/// acyclic trace of exactly one drive. Parentage holds in every fixture —
/// a fixture that grows an orphan or a cycle fails here before the
/// telemetry ever ships.
#[test]
fn every_golden_fixture_is_one_rooted_acyclic_trace() {
    for name in SCENARIOS {
        let stored = std::fs::read_to_string(fixture_path(name))
            .unwrap_or_else(|error| panic!("{name} fixture unreadable: {error}"));
        let trace: Vec<SpanNode> = serde_json::from_str(&stored).unwrap();
        assert_trace_discipline(&trace, &drive_id(name));
    }
}

/// Parentage, acyclicity, and single-root discipline for one trace: every
/// span's parent exists in the trace or the span is the root, no span is
/// its own parent, the sole root is the drive span, and every parent
/// precedes its child — the id scheme is positional, so a later parent
/// would be a cycle.
fn assert_trace_discipline(trace: &[SpanNode], drive: &str) {
    let ids: Vec<&str> = trace.iter().map(|span| span.span_id.as_str()).collect();
    let roots = trace
        .iter()
        .filter(|span| span.parent_span_id.is_none())
        .count();
    assert_eq!(roots, 1, "{drive}: exactly one root span");
    let root = &trace[0];
    assert_eq!(root.name, "drive", "{drive}: the root span is the drive");
    assert_eq!(root.parent_span_id, None, "{drive}: the root has no parent");
    assert_eq!(root.span_id, format!("{drive}:drive"));
    assert_eq!(root.trace_id, drive, "{drive}: the trace id is the drive");
    for (index, span) in trace.iter().enumerate() {
        assert_eq!(
            span.trace_id, drive,
            "{drive}: span {index} adopted cross-drive"
        );
        match &span.parent_span_id {
            None => assert_eq!(index, 0, "{drive}: only the first span is the root"),
            Some(parent) => {
                assert_ne!(
                    parent, &span.span_id,
                    "{drive}: span {index} is its own parent"
                );
                let parent_position = ids
                    .iter()
                    .position(|id| *id == parent.as_str())
                    .unwrap_or_else(|| {
                        panic!("{drive}: span {index} references missing parent {parent}")
                    });
                assert!(
                    parent_position < index,
                    "{drive}: span {index} is parented after its parent (cycle)"
                );
            }
        }
    }
}
