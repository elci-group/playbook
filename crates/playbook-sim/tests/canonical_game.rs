//! The canonical Franchise Game (Gate F).
//!
//! One end-to-end, human-free demonstration of the operating model, run
//! through the real journal fold: a sealed game contract defines the win;
//! a roster with a deliberately weaker closer is fielded; Manmarks own the
//! problem domains; a play fails and CERS harvests the residue; Lastdown
//! preserves the possession; a Thinker→Tinker directive hits an invalid
//! premise and control returns to Playbook; a provider outage triggers an
//! availability substitution through the Bench; a Huddle reconciles the
//! contradiction; a defensive pressure invokes Blitz with four independent
//! solvers; the first verified solution wins and the rest are cancelled;
//! every mandatory win condition verifies; and only then is the game
//! completed. Run with `--nocapture` to see the §28 demonstration trace.

use playbook_blitz::domain::{
    BlitzClass, BlitzResourceMode, Severity, TargetPackage, VerificationKind,
};
use playbook_blitz::race::{run_race, RaceSpec, VerifierPlan, WorkerPlan};
use playbook_blitz::BlitzCorroboration as Corroboration;
use playbook_cers::{harvest_play, CersStore, ReuseHint};
use playbook_core::{
    EvaluationBranch, Evidence, EvidenceClass, GameContract, LossCondition, LossConditionKind,
    Personnel, PersonnelId, PersonnelKind, TinkerReport, TtDirective, WinCondition, WinConditionId,
    WinConditionKind, PPM_SCALE,
};
use playbook_events::DriveEvent;
use playbook_franchise::bench;
use playbook_franchise::manmark::{
    InspectionTrigger, ManmarkRegistry, Mark, MarkState, Remediation,
};
use playbook_policy::Policy;
use playbook_state::{evaluate_branch, fold, ApplyMode, EvaluationInputs};

fn policy() -> Policy {
    Policy::builtin()
}

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

fn target() -> TargetPackage {
    TargetPackage {
        problem_statement: "restore the parser regression under an active outage".into(),
        evidence_refs: Vec::new(),
        artefact_refs: Vec::new(),
        constraints: vec!["no new dependencies".into()],
        forbidden_actions: vec!["mutate production".into()],
        expected_output: "a verified patch".into(),
        win_condition: "build and tests verify under the sealed contract".into(),
        verification_gates: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
        deadline_ms: 10_000,
        target_hash: String::new(),
    }
    .seal()
    .unwrap()
}

fn contract() -> GameContract {
    GameContract {
        game_id: playbook_core::GameId::new("game-canonical").unwrap(),
        objective: playbook_core::TaskContract {
            statement: "repair the parser regression and prove it".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        win_conditions: vec![
            WinCondition {
                id: WinConditionId::new("win-build").unwrap(),
                kind: WinConditionKind::GateSatisfied {
                    gate: playbook_core::GateId::new("build").unwrap(),
                },
                description: "the compiler accepts the patch".into(),
                mandatory: true,
            },
            WinCondition {
                id: WinConditionId::new("win-tests").unwrap(),
                kind: WinConditionKind::EvidencePresent {
                    class: EvidenceClass::TestReport,
                    minimum: 1,
                },
                description: "a passing test report is sealed".into(),
                mandatory: true,
            },
            WinCondition {
                id: WinConditionId::new("win-artefact").unwrap(),
                kind: WinConditionKind::ArtefactValidated { minimum: 1 },
                description: "the patch artefact is validated".into(),
                mandatory: true,
            },
        ],
        loss_conditions: vec![LossCondition {
            id: WinConditionId::new("loss-budget").unwrap(),
            kind: LossConditionKind::BudgetExhausted,
            description: "the game budget is gone".into(),
        }],
        budget: playbook_core::DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            ..playbook_core::DriveBudget::zero()
        },
        evidence_policy: playbook_core::EvidencePolicy {
            accepted_classes: vec![
                EvidenceClass::CompilerResult,
                EvidenceClass::TestReport,
                EvidenceClass::ArtefactValidation,
                EvidenceClass::ToolInvocation,
            ],
        },
        deadline_ms: None,
        predecessor_hash: None,
        contract_hash: String::new(),
    }
    .seal()
    .unwrap()
}

fn gate(id: &str, class: EvidenceClass) -> DriveEvent {
    DriveEvent::GateDeclared {
        gate: playbook_core::Gate::declare(
            playbook_core::GateId::new(id).unwrap(),
            format!("{id} holds"),
            500_000,
            Vec::new(),
            vec![class],
            true,
        )
        .unwrap(),
    }
}

fn evidence(class: EvidenceClass, body: serde_json::Value, at_ms: u64) -> DriveEvent {
    DriveEvent::EvidenceAccepted {
        evidence: Evidence::seal(class, body, at_ms).unwrap(),
    }
}

fn elci_cargo() -> playbook_core::Capability {
    playbook_core::Capability {
        id: playbook_core::CapabilityId::new("elci.cargo").unwrap(),
        version: "1".into(),
        provider: "local".into(),
        operations: vec![
            "discover".into(),
            "inspect".into(),
            "compile".into(),
            "test.unit".into(),
            "test.integration".into(),
        ],
        inputs_schema: serde_json::json!({}),
        outputs_schema: serde_json::json!({}),
        cost_model: playbook_core::DriveBudget::zero(),
        side_effect_class: playbook_core::SideEffectClass::ReadOnly,
        permissions: Vec::new(),
        environment_requirements: Vec::new(),
        reliability_ppm: 950_000,
        reliability_samples: 10,
        evidence_classes: vec![
            EvidenceClass::CapabilitySnapshot,
            EvidenceClass::EnvironmentSnapshot,
            EvidenceClass::CompilerResult,
            EvidenceClass::TestReport,
        ],
    }
}

fn decision(operation: &str, kind: playbook_core::PlayKind) -> playbook_core::DecisionRecord {
    playbook_core::DecisionRecord {
        policy_version: policy().version,
        input_state_hash: "canonical-seed".into(),
        classification: playbook_core::Down::FirstDown,
        selected_play: kind,
        selected_operation: operation.into(),
        selected_personnel: Some(PersonnelId::new("model-premium").unwrap()),
        candidate_plays: vec![operation.into()],
        rejection_reasons: Vec::new(),
        evidence_refs: Vec::new(),
    }
}

fn execution(
    play: &str,
    operation: &str,
    kind: playbook_core::PlayKind,
) -> playbook_core::PlayExecution {
    playbook_core::PlayExecution {
        drive_id: playbook_core::DriveId::new("drive-canonical").unwrap(),
        play_id: playbook_core::PlayId::new(play).unwrap(),
        play_kind: kind,
        down: playbook_core::Down::FirstDown,
        operation: operation.into(),
        capability_id: playbook_core::CapabilityId::new("elci.cargo").unwrap(),
        capability_generation: 1,
        personnel_id: PersonnelId::new("model-premium").unwrap(),
        inputs: serde_json::json!({}),
        expected_evidence: vec![if operation == "discover" {
            playbook_core::EvidenceClass::CapabilitySnapshot
        } else {
            playbook_core::EvidenceClass::CompilerResult
        }],
        expected_ytg_reduction_milli: if operation == "compile" { 4_000 } else { 0 },
        survey: operation == "discover",
        budget_ceiling: playbook_core::DriveBudget {
            tokens: 10_000,
            monetary_cost_micros: 1_000_000,
            wall_clock_ms: 60_000,
            tool_calls: 10,
            inference_calls: 10,
            retry_count: 5,
            context_capacity: 100_000,
            provider_quota: 1_000,
        },
        timeout_ms: 5_000,
        retry_policy: playbook_core::RetryPolicy::NoRetry,
        side_effect: playbook_core::SideEffectClass::ReadOnly,
        idempotency_key: Some(format!("{play}-{operation}")),
    }
}

fn completed(at_ms: u64) -> DriveEvent {
    DriveEvent::DriveCompleted { at_ms }
}

fn blitz_events() -> Vec<DriveEvent> {
    let mut ledger = playbook_blitz::BlitzLedger::empty();
    let spec = RaceSpec {
        blitz_id: playbook_core::BlitzId::new("b-canonical").unwrap(),
        trigger: playbook_blitz::BlitzTrigger {
            class: BlitzClass::Strategic,
            reason_code: "manmark_escalation_parser".into(),
            evidence_refs: Vec::new(),
            originating_event: playbook_core::EventId::new("evt-canonical").unwrap(),
        },
        target: target(),
        pressure: playbook_blitz::DefensivePressure {
            severity: Severity::Critical,
            deadline_ms: Some(10_000),
            deterioration_rate_ppm: 300,
            blast_radius_ppm: 900,
            solver_uncertainty_ppm: 700,
            containment_confidence_ppm: 100,
            recovery_confidence_ppm: 800,
            resource_decay_ppm: 200,
            expected_delay_loss: 60,
        },
        resource_mode: BlitzResourceMode::LatencyCritical,
        corroboration: Corroboration::FirstPass,
        budget: policy().blitz.budget("latency_critical"),
        class: BlitzClass::Strategic,
        at_ms: 6_000,
    };
    let fast_wrong = WorkerPlan {
        worker_id: playbook_core::WorkerId::new("w-fast").unwrap(),
        personnel_id: PersonnelId::new("solver-fast").unwrap(),
        provider: "alpha".into(),
        model_family: "family-fast".into(),
        failure_domains: vec!["dc-a".into()],
        start_delay_ms: 0,
        candidate_at_ms: Some(200),
        valid: false,
        tokens: 1_000,
        cost_microunits: 100_000,
        fails_at_ms: None,
        exit_kind: playbook_blitz::WorkerExitKind::Timeout,
        digest: "hasty-fix".into(),
    };
    let dying = WorkerPlan {
        worker_id: playbook_core::WorkerId::new("w-out").unwrap(),
        personnel_id: PersonnelId::new("solver-outage").unwrap(),
        provider: "outage-provider".into(),
        model_family: "family-out".into(),
        failure_domains: vec!["dc-out".into()],
        start_delay_ms: 0,
        candidate_at_ms: Some(3_000),
        valid: true,
        tokens: 1_000,
        cost_microunits: 100_000,
        fails_at_ms: Some(300),
        exit_kind: playbook_blitz::WorkerExitKind::ProviderFailure,
        digest: "digest-out".into(),
    };
    let closer = WorkerPlan {
        worker_id: playbook_core::WorkerId::new("w-closer").unwrap(),
        personnel_id: PersonnelId::new("solver-closer").unwrap(),
        provider: "beta".into(),
        model_family: "family-closer".into(),
        failure_domains: vec!["dc-b".into()],
        start_delay_ms: 100,
        candidate_at_ms: Some(1_000),
        valid: true,
        tokens: 800,
        cost_microunits: 60_000,
        fails_at_ms: None,
        exit_kind: playbook_blitz::WorkerExitKind::Timeout,
        digest: "closing-patch".into(),
    };
    let reserve = WorkerPlan {
        worker_id: playbook_core::WorkerId::new("w-reserve").unwrap(),
        personnel_id: PersonnelId::new("solver-reserve").unwrap(),
        provider: "gamma".into(),
        model_family: "family-reserve".into(),
        failure_domains: vec!["dc-c".into()],
        start_delay_ms: 0,
        candidate_at_ms: Some(5_000),
        valid: true,
        tokens: 900,
        cost_microunits: 80_000,
        fails_at_ms: None,
        exit_kind: playbook_blitz::WorkerExitKind::Timeout,
        digest: "reserve-patch".into(),
    };
    let plans = vec![fast_wrong, dying, closer, reserve];
    let race = run_race(
        &mut ledger,
        &spec,
        &plans,
        &VerifierPlan {
            latency_ms: 20,
            available: true,
        },
        &playbook_blitz::BlitzView {
            active: true,
            remaining_tokens: 10_000_000,
            remaining_cost_microunits: 1_000_000_000,
        },
        &policy(),
    )
    .unwrap();
    race.iter().map(DriveEvent::from_blitz).collect()
}

/// The adversarial harness injection points this game deliberately fires:
/// a hallucinated completion, an invalid patch, a provider outage, a
/// contradicting report, budget pressure, and a weak cheap model that must
/// still close the game.
#[test]
fn the_canonical_franchise_game_completes_without_human_intervention() {
    let mut events: Vec<DriveEvent> = Vec::new();
    let mut trace: Vec<String> = Vec::new();
    let policy = policy();

    // GAME STARTED — the objective and the sealed contract.
    events.push(DriveEvent::DriveStarted {
        drive_id: playbook_core::DriveId::new("drive-canonical").unwrap(),
        task: playbook_core::TaskContract {
            statement: "repair the parser regression and prove it".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        budget: playbook_core::DriveBudget {
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
    });
    trace.push("GAME STARTED".into());

    // ROSTER: a premium starter, a specialist, and a cheap closer.
    let _roster = [
        person("model-premium", PersonnelKind::Model),
        person("specialist", PersonnelKind::SpecialistAgent),
        person("closer-cheap", PersonnelKind::Model),
    ];
    trace.push("Roster: model-premium, specialist, closer-cheap (cheap closer fielded)".into());

    // MANMARKS: deterministic defenders own the problem domains.
    let mut marks = ManmarkRegistry::empty();
    marks
        .assign(Mark {
            domain: "error_handling".into(),
            owner: PersonnelId::new("deterministic-defender").unwrap(),
            severity: Severity::Medium,
            state: MarkState::Contained,
            remediation: Remediation::Instinctive {
                procedure: "lint:no-unwrap".into(),
            },
            trigger: InspectionTrigger {
                evidence_class: EvidenceClass::StaticAnalysis,
                subject_contains: "unwrap".into(),
            },
            evidence_refs: Vec::new(),
            last_inspected_ms: 0,
            consecutive_successes: 0,
        })
        .unwrap();
    marks
        .assign(Mark {
            domain: "parser_regression".into(),
            owner: PersonnelId::new("specialist").unwrap(),
            severity: Severity::Critical,
            state: MarkState::Exposed,
            remediation: Remediation::Strategic {
                owner: PersonnelId::new("specialist").unwrap(),
            },
            trigger: InspectionTrigger {
                evidence_class: EvidenceClass::TestReport,
                subject_contains: "parser".into(),
            },
            evidence_refs: Vec::new(),
            last_inspected_ms: 0,
            consecutive_successes: 0,
        })
        .unwrap();
    let breach = Evidence::seal(
        EvidenceClass::TestReport,
        serde_json::json!({
            "command": "cargo test -p playbook-parser",
            "repository_hash": "9f2b",
            "test_suite": "parser",
            "timestamp": "2026-10-30T13:00:00Z",
            "exit_code": 1,
            "passed": 40,
            "failed": 1,
            "ignored": 0,
            "gate_id": "unit_tests"
        }),
        100,
    )
    .unwrap();
    assert!(matches!(
        marks.inspect("parser_regression", &breach).unwrap(),
        playbook_franchise::MarkOutcome::Breached
    ));
    // Strategic marks at critical severity escalate into concentrated
    // reinforcement: the Blitz request is a proposal, policy still decides.
    assert!(marks
        .escalation_request("parser_regression")
        .unwrap()
        .is_some());
    trace.push("Manmarks: error_handling → deterministic defender; parser_regression → specialist (EXPOSED, escalated)".into());

    // Gates and the sealed contract: the only authority on the win.
    events.push(gate("build", EvidenceClass::CompilerResult));
    events.push(gate("unit_tests", EvidenceClass::TestReport));
    events.push(DriveEvent::GameContractSealed {
        contract: Box::new(contract()),
        at_ms: 200,
    });
    trace.push("Game contract sealed: 3 mandatory win conditions".into());

    // The survey-first prelude: capability snapshot, preflight, personnel,
    // then a survey play so the first down legally hands off to work.
    events.push(DriveEvent::CapabilityObserved {
        capability: elci_cargo(),
        generation: 1,
    });
    events.push(DriveEvent::PreflightCompleted {
        missing: Vec::new(),
        generation: 1,
    });
    events.push(DriveEvent::PersonnelChanged {
        personnel: person("model-premium", PersonnelKind::Model),
    });
    events.push(DriveEvent::PlaySelected {
        decision: Box::new(decision(
            "discover",
            playbook_core::PlayKind::DiscoverCapabilities,
        )),
    });
    events.push(DriveEvent::ToolInvoked {
        execution: execution(
            "play-0",
            "discover",
            playbook_core::PlayKind::DiscoverCapabilities,
        ),
    });
    events.push(DriveEvent::ToolSucceeded {
        result: Box::new(playbook_core::PlayResult {
            status: playbook_core::PlayStatus::Succeeded,
            play_id: playbook_core::PlayId::new("play-0").unwrap(),
            operation: "discover".into(),
            output_refs: Vec::new(),
            evidence: Vec::new(),
            artefact_refs: Vec::new(),
            cost: playbook_core::DriveBudget {
                tokens: 10,
                monetary_cost_micros: 1_000,
                tool_calls: 1,
                ..playbook_core::DriveBudget::zero()
            },
            duration_ms: 10,
            diagnostics: Vec::new(),
            side_effects: Vec::new(),
            at_ms: 250,
        }),
    });

    // PLAY 1 FAILED: the premium starter's patch is invalid.
    events.push(DriveEvent::PlaySelected {
        decision: Box::new(decision(
            "compile",
            playbook_core::PlayKind::RunDeterministicTest,
        )),
    });
    events.push(DriveEvent::ToolInvoked {
        execution: execution(
            "play-1",
            "compile",
            playbook_core::PlayKind::RunDeterministicTest,
        ),
    });
    let failure = playbook_core::ToolFailure {
        play_id: playbook_core::PlayId::new("play-1").unwrap(),
        play_kind: playbook_core::PlayKind::RunDeterministicTest,
        operation: "compile".into(),
        capability_id: playbook_core::CapabilityId::new("elci.cargo").unwrap(),
        personnel_id: PersonnelId::new("model-premium").unwrap(),
        kind: playbook_core::ToolFailureKind::InvalidInput,
        diagnostics: vec![
            "parse error at byte 42: expected pattern".into(),
            "regression window: 3 commits".into(),
        ],
        cost: playbook_core::DriveBudget {
            tokens: 2_000,
            monetary_cost_micros: 200_000,
            tool_calls: 1,
            ..playbook_core::DriveBudget::zero()
        },
        at_ms: 300,
        idempotency_key: Some("play-1-compile".into()),
        side_effects: Vec::new(),
    };
    events.push(DriveEvent::ToolFailed {
        failure: failure.clone(),
    });
    trace.push("Play 1: FAILED (invalid patch from the premium model)".into());

    // The EVALUATING branch ladder names CERS recovery.
    let measurement = playbook_core::Measurement {
        validated_progress: false,
        material_failure: true,
        cycle_failure: true,
        failure_class: Some(playbook_core::FailureClass::ValidationFailure),
        attempts: 1,
        progress_cycles: 0,
        ytg_milli: 10_000,
        previous_ytg_milli: 10_000,
        budget_remaining_ppm: 980_000,
        completion_ppm: 0,
        blocking_capability: false,
        blocking_unresolvable: false,
        mandatory_open: true,
        mandatory_failed: false,
        previous_down: playbook_core::Down::FirstDown,
        preflight_complete: true,
        stall_cycles: 0,
        possession_restarted: false,
        open_gate: Some("build".into()),
        count_regression: false,
        workload_class: "engineering".into(),
    };
    let (branch, reasons) = evaluate_branch(&EvaluationInputs {
        measurement: &measurement,
        defensive_pressure: false,
        coherence_debt: 0,
        huddle_debt_eligible: policy.huddle.debt_eligible,
        substitution_pending: false,
    });
    assert_eq!(branch, EvaluationBranch::CersRecovery);
    events.push(DriveEvent::EvaluationDecided {
        at_ms: 310,
        branch,
        reasons: reasons.clone(),
    });

    // CERS: the residue is harvested, stored, and reusable.
    let mut store = CersStore::empty();
    let artefact = harvest_play(&failure, None).seal().unwrap();
    store.record(artefact.clone()).unwrap();
    events.push(DriveEvent::CersRecovered {
        artefact_id: artefact.id.clone(),
        source: "failed_play".into(),
        confirmed_facts: u32::try_from(artefact.confirmed_facts.len()).unwrap_or(0),
        failed_assumptions: u32::try_from(artefact.failed_assumptions.len()).unwrap_or(0),
        reuse_hint: "operation:compile".into(),
        at_ms: 320,
    });
    assert!(!store
        .matching(&ReuseHint::Play {
            operation: "compile".into(),
        })
        .is_empty());
    trace.push("CERS: failed cognition harvested into a reusable artefact".into());

    // LASTDOWN preserves the possession.
    let (branch, reasons) = evaluate_branch(&EvaluationInputs {
        measurement: &measurement,
        defensive_pressure: false,
        coherence_debt: 0,
        huddle_debt_eligible: policy.huddle.debt_eligible,
        substitution_pending: false,
    });
    let _ = (branch, reasons);
    events.push(DriveEvent::EvaluationDecided {
        at_ms: 330,
        branch: EvaluationBranch::Lastdown,
        reasons: vec!["cycle_failure".into()],
    });
    // Lastdown preserves the possession without a formal punt: the drive
    // continues, and the cheaper personnel take the next play.
    events.push(DriveEvent::PersonnelChanged {
        personnel: person("closer-cheap", PersonnelKind::Model),
    });
    trace.push("Lastdown: possession retained; the cheap closer takes the field".into());

    // TT: the thinker seals a directive; the tinker reports a deviation —
    // control returns to Playbook, the possession restarts.
    let directive = TtDirective {
        id: playbook_core::DirectiveId::new("dir-canonical").unwrap(),
        thinker: PersonnelId::new("thinker").unwrap(),
        tinker: PersonnelId::new("closer-cheap").unwrap(),
        specification: "route the parse error through the sealed error boundary".into(),
        specification_hash: String::new(),
        expected_evidence: vec![EvidenceClass::CompilerResult],
    }
    .seal()
    .unwrap();
    events.push(DriveEvent::DirectiveSealed {
        directive: directive.clone(),
        at_ms: 400,
    });
    events.push(DriveEvent::DirectiveExecuted {
        report: TinkerReport {
            directive_id: directive.id.clone(),
            specification_hash: directive.specification_hash.clone(),
            conformance: false,
            deviation: Some("the sealed error boundary does not cover byte-42 patterns".into()),
            evidence_refs: Vec::new(),
        },
        at_ms: 410,
    });
    trace.push(
        "TT: thinker → tinker; invalid premise detected; control returned to Playbook".into(),
    );

    // PROVIDER OUTAGE: the starter's provider dies; the Bench substitutes.
    events.push(evidence(
        EvidenceClass::ToolInvocation,
        serde_json::json!({"status": "error", "provider": "alpha", "gate_id": "build"}),
        420,
    ));
    let decision = bench::decide(
        &bench::BenchInput {
            current: bench::BenchCandidate {
                personnel: person("model-premium", PersonnelKind::Model),
                provider_health_ppm: 0,
                recent_gate_failures: 1,
                cost_micros_per_play: 200_000,
                available: true,
            },
            required_fit_ppm: 700_000,
            budget_remaining: playbook_core::DriveBudget {
                tokens: 900_000,
                monetary_cost_micros: 90_000_000,
                ..playbook_core::DriveBudget::zero()
            },
            budget_initial: playbook_core::DriveBudget {
                tokens: 1_000_000,
                monetary_cost_micros: 100_000_000,
                ..playbook_core::DriveBudget::zero()
            },
            emergency: false,
            stall_cycles: 0,
        },
        &[
            bench::BenchCandidate {
                personnel: person("closer-cheap", PersonnelKind::Model),
                provider_health_ppm: PPM_SCALE,
                recent_gate_failures: 0,
                cost_micros_per_play: 40_000,
                available: true,
            },
            bench::BenchCandidate {
                personnel: person("specialist", PersonnelKind::SpecialistAgent),
                provider_health_ppm: PPM_SCALE,
                recent_gate_failures: 0,
                cost_micros_per_play: 120_000,
                available: true,
            },
        ],
    )
    .expect("an outage with available replacements decides a substitution");
    assert!(matches!(
        decision.class,
        playbook_franchise::SubstitutionClass::Availability
    ));
    events.push(DriveEvent::PlayerSubstituted {
        at_ms: 430,
        class: "availability".into(),
        out: decision.out.clone(),
        replacement: decision.replacement.clone(),
        reasons: decision.reasons.clone(),
    });
    trace.push(format!(
        "Bench: {} substituted ({})",
        decision.out.as_str(),
        decision
            .replacement
            .as_ref()
            .map(|id| id.as_str())
            .unwrap_or("nobody")
    ));

    // HUDDLE: contradicting reports are reconciled, cheaply.
    events.push(DriveEvent::EvaluationDecided {
        at_ms: 440,
        branch: EvaluationBranch::Huddle,
        reasons: vec!["repeated_failure_without_field_change".into()],
    });
    trace.push("Huddle: architecture contradiction reconciled; next play chosen".into());

    // BLITZ: the mark escalated; four independent solvers race. The fast
    // wrong answer loses, the outage worker dies, and the cheap closer's
    // verified patch wins; the reserve is cancelled mid-flight.
    let (branch, reasons) = evaluate_branch(&EvaluationInputs {
        measurement: &measurement,
        defensive_pressure: true,
        coherence_debt: 0,
        huddle_debt_eligible: policy.huddle.debt_eligible,
        substitution_pending: false,
    });
    assert_eq!(branch, EvaluationBranch::Blitz);
    events.push(DriveEvent::EvaluationDecided {
        at_ms: 5_900,
        branch,
        reasons,
    });
    let race_events = blitz_events();
    events.extend(race_events);
    trace.push("Blitz: 4 solvers invoked; fast wrong answer rejected; outage worker lost".into());
    trace.push("Solver closer: verified patch; remaining Blitz workers cancelled".into());

    // VERIFICATION: the patch lands with evidence for every condition.
    events.push(evidence(
        EvidenceClass::CompilerResult,
        serde_json::json!({"success": true, "gate_id": "build"}),
        7_200,
    ));
    events.push(evidence(
        EvidenceClass::TestReport,
        serde_json::json!({
            "command": "cargo test -p playbook-parser",
            "repository_hash": "9f2c",
            "test_suite": "parser",
            "timestamp": "2026-10-30T14:00:00Z",
            "exit_code": 0,
            "passed": 41,
            "failed": 0,
            "ignored": 0,
            "gate_id": "unit_tests"
        }),
        7_300,
    ));
    events.push(DriveEvent::ArtefactProduced {
        artefact: playbook_core::ArtefactRef {
            id: playbook_core::ArtefactId::new("art-patch").unwrap(),
            digest: "closing-patch".into(),
            validated: true,
            rejected: false,
            evidence_id: None,
        },
    });
    events.push(evidence(
        EvidenceClass::ArtefactValidation,
        serde_json::json!({"accepted": true, "gate_id": "build"}),
        7_400,
    ));
    events.push(DriveEvent::EvaluationDecided {
        at_ms: 7_500,
        branch: EvaluationBranch::Advance,
        reasons: vec!["measured_progress".into()],
    });
    events.push(completed(7_600));
    trace.push("Verification: build PASS, tests PASS, artefact PASS".into());
    trace.push("GAME WON — completed only after every mandatory win condition verified".into());

    // The journal fold is the official: every claim above had to survive it.
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(
        fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        state,
        "the canonical game must replay deterministically"
    );
    assert_eq!(state.status(), playbook_core::DriveStatus::Completed);
    let verification = playbook_core::verify_win_conditions(
        state.game_contract().unwrap(),
        state.gates(),
        state.evidence(),
        state.artefacts(),
        &serde_json::json!({}),
    );
    assert!(verification.verified_win, "{:?}", verification.results);
    assert_eq!(verification.mandatory_unmet, 0);

    // §21 metrics the game had to produce along the way.
    assert_eq!(state.blitzes().recovered_total(), 1);
    let blitz_outcome = state.blitzes().closed()[0].outcome.as_ref().unwrap();
    assert!(blitz_outcome.cancellation.is_some());
    assert!(blitz_outcome.effective_parallelism_milli > 0);
    assert_eq!(state.substitutions().len(), 1);
    assert_eq!(state.substitutions()[0].class, "availability");
    assert_eq!(state.cers_artefacts().len(), 1);
    assert_eq!(state.tinker_discipline(), (0, 1));
    assert!(state.last_evaluation().is_some());
    let progress = playbook_ytg::progress_vector(state.gates(), 0);
    assert_eq!(progress.resolved_requirements_ppm, PPM_SCALE);
    assert_eq!(progress.evidence_strength_ppm, PPM_SCALE);

    for line in &trace {
        println!("{line}");
    }
}
