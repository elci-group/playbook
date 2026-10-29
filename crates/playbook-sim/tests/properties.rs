use std::path::Path;

use playbook_sim::{
    authorize, classify_measurement, fold, gate_one, leaks_competitive_metadata, open_drive,
    run_mvp, try_tov_normalise, ApplyMode, Down, DriveBudget, DriveEvent, Evidence, EvidenceClass,
    FixturePlane, GateId, Measurement, PlayStatus, PlaybookError, Policy, SideEffectClass,
    PPM_SCALE,
};
use serde_json::{json, Value};

#[test]
fn invariants_hold_on_the_engine() {
    let report = run_mvp().unwrap();
    assert!(report.replay_match);
    assert!(!report.package_leaks);
    let mut tampered = report.events.clone();
    tampered
        .iter_mut()
        .find_map(|event| match event {
            DriveEvent::DownChanged { reasons, .. } => Some(reasons),
            _ => None,
        })
        .unwrap()
        .push("tampered".into());
    assert!(matches!(
        fold(&tampered, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::ReplayDivergence(_)
    ));

    let mut forged = report.events.clone();
    if let Some(DriveEvent::ToolSucceeded { result }) = forged
        .iter_mut()
        .find(|event| matches!(event, DriveEvent::ToolSucceeded { .. }))
    {
        result.status = PlayStatus::Failed;
    }
    assert!(matches!(
        fold(&forged, &Policy::builtin(), ApplyMode::Audit).unwrap_err(),
        PlaybookError::Inadmissible(_, _)
    ));

    let policy = Policy::builtin();
    let measurement = sample_measurement();
    let first = classify_measurement(&measurement, &policy);
    assert_eq!(first, classify_measurement(&measurement, &policy));
    assert_eq!(first.down, Down::FirstDown);

    assert_eq!(
        authorize(SideEffectClass::Deployment, &policy.allowed_side_effects).unwrap_err(),
        PlaybookError::SideEffect("Deployment".into())
    );
    let low_budget_ppm = 10_000u32;
    assert!(low_budget_ppm < policy.rush_budget_fraction_ppm);
    assert!(authorize(SideEffectClass::Deployment, &policy.allowed_side_effects).is_err());

    let bytes = serde_json::to_vec(report.final_state.package().unwrap()).unwrap();
    assert!(!leaks_competitive_metadata(&bytes));
    assert!(report.events.iter().any(|event| matches!(
        event,
        DriveEvent::FourthDownDeclared {
            disposition: playbook_sim::Disposition::Punt,
            ..
        }
    )));
}

#[test]
fn fuzz_rejects_weightless_claims_and_illegal_budget_moves() {
    let (mut drive, _) = open_drive(
        FixturePlane::ready(),
        vec![gate_one("implementation", EvidenceClass::CompilerResult)],
    )
    .unwrap();
    let ytg = drive.state().ytg_milli();
    let mut rng = XorShift(0xA5A5_1234_F00D_u64);
    for index in 0..50 {
        let evidence = Evidence::seal(
            EvidenceClass::NarrativeClaim,
            json!({"text": format!("claim-{}", rng.next())}),
            index,
        )
        .unwrap();
        drive.accept_evidence(evidence).unwrap();
        assert_eq!(drive.state().ytg_milli(), ytg);
        let before = drive.state().clone();
        assert_eq!(
            drive
                .waive(GateId::new("implementation").unwrap(), "", "because")
                .unwrap_err(),
            PlaybookError::WaiverAuthority
        );
        assert_eq!(drive.state(), &before);
        let err = drive
            .pressure(
                DriveBudget {
                    tokens: 1_001 + (rng.next() % 50),
                    ..DriveBudget::zero()
                },
                index,
            )
            .unwrap_err();
        assert_eq!(err, PlaybookError::BudgetExhausted);
        assert_eq!(drive.state().ytg_milli(), ytg);
    }
}

#[test]
fn a_missing_tov_adapter_stays_isolated() {
    let outcome = try_tov_normalise(
        Path::new("tov/adapters/missing.tov"),
        &json!({
            "command": "cargo test",
            "repository_hash": "abc",
            "test_suite": "unit",
            "exit_code": 0,
            "passed": 4,
            "failed": 0,
            "ignored": 0,
            "timestamp": "1",
            "gate_id": "unit_tests"
        }),
    )
    .unwrap();
    assert!(outcome.isolated);
    assert!(outcome.accepted);
    assert_eq!(outcome.source, "rust");
}

#[test]
fn inconsistent_nonzero_test_exit_reopens_an_already_satisfied_gate() {
    let (mut drive, _) = open_drive(
        FixturePlane::ready(),
        vec![gate_one("unit_tests", EvidenceClass::TestReport)],
    )
    .unwrap();
    let passing_evidence = Evidence::seal(
        EvidenceClass::TestReport,
        json!({
            "command": "cargo test",
            "repository_hash": "abc",
            "test_suite": "unit",
            "exit_code": 0,
            "passed": 1,
            "failed": 0,
            "ignored": 0,
            "timestamp": "13",
            "gate_id": "unit_tests"
        }),
        13,
    )
    .unwrap();
    drive.accept_evidence(passing_evidence).unwrap();
    assert_eq!(
        drive.state().gates()[0].state,
        playbook_core::GateState::Satisfied
    );

    let inconsistent = Evidence::seal(
        EvidenceClass::TestReport,
        json!({
            "command": "cargo test",
            "repository_hash": "abc",
            "test_suite": "unit",
            "exit_code": 1,
            "passed": 4,
            "failed": 0,
            "ignored": 0,
            "timestamp": "14",
            "gate_id": "unit_tests"
        }),
        14,
    )
    .unwrap();
    drive.accept_evidence(inconsistent).unwrap();
    assert_eq!(
        drive.state().gates()[0].state,
        playbook_core::GateState::Failed
    );
    assert_eq!(drive.state().gates()[0].satisfaction_ppm, 0);
    assert!(!drive.state().gates()[0].is_terminal());
}

#[test]
fn partial_test_failure_preserves_progress_and_reopens_satisfied_gate() {
    let (mut drive, _) = open_drive(
        FixturePlane::ready(),
        vec![gate_one("unit_tests", EvidenceClass::TestReport)],
    )
    .unwrap();
    let partial_failure = |passed: u64, failed: u64, exit_code: i64, timestamp: &str| {
        Evidence::seal(
            EvidenceClass::TestReport,
            json!({
                "command": "cargo test",
                "repository_hash": "abc",
                "test_suite": "unit",
                "exit_code": exit_code,
                "passed": passed,
                "failed": failed,
                "ignored": 0,
                "timestamp": timestamp,
                "gate_id": "unit_tests"
            }),
            timestamp.parse::<u64>().unwrap(),
        )
        .unwrap()
    };

    drive
        .accept_evidence(partial_failure(3, 1, 1, "21"))
        .unwrap();
    assert_eq!(drive.state().gates()[0].satisfaction_ppm, 750_000);
    assert_eq!(
        drive.state().gates()[0].state,
        playbook_core::GateState::InProgress
    );

    drive
        .accept_evidence(partial_failure(4, 0, 0, "22"))
        .unwrap();
    assert_eq!(
        drive.state().gates()[0].state,
        playbook_core::GateState::Satisfied
    );

    drive
        .accept_evidence(partial_failure(3, 1, 1, "23"))
        .unwrap();
    assert_eq!(drive.state().gates()[0].satisfaction_ppm, 750_000);
    assert_eq!(
        drive.state().gates()[0].state,
        playbook_core::GateState::InProgress
    );
    assert!(!drive.state().gates()[0].is_terminal());
}

#[test]
fn schemas_name_the_contract_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let report = run_mvp().unwrap();
    let policy = serde_json::to_value(Policy::builtin()).unwrap();
    let evidence = report
        .events
        .iter()
        .find_map(|event| match event {
            DriveEvent::EvidenceAccepted { evidence } => {
                Some(serde_json::to_value(evidence).unwrap())
            }
            _ => None,
        })
        .unwrap();
    let capability = report
        .events
        .iter()
        .find_map(|event| match event {
            DriveEvent::CapabilityObserved { capability, .. } => {
                Some(serde_json::to_value(capability).unwrap())
            }
            _ => None,
        })
        .unwrap();
    let execution = report
        .events
        .iter()
        .find_map(|event| match event {
            DriveEvent::ToolInvoked { execution } => Some(serde_json::to_value(execution).unwrap()),
            _ => None,
        })
        .unwrap();
    let continuation = serde_json::to_value(report.final_state.package().unwrap()).unwrap();
    let mut prior_table = playbook_core::HistoricalPriorTable::empty();
    prior_table.record(&playbook_core::PersonnelId::new("agent1").unwrap(), 8_000);
    let prior_bundle = serde_json::to_value(playbook_core::PriorBundle {
        schema_version: 1,
        priors: prior_table,
    })
    .unwrap();
    let huddle_request = serde_json::to_value(playbook_huddle::HuddleRequestBody {
        scope: playbook_huddle::HuddleScope::Position,
        trigger_kind: playbook_huddle::HuddleTriggerKind::CoherenceDebt,
        reason_code: "coherence_debt".into(),
        evidence_refs: Vec::new(),
        manual: false,
        value: playbook_huddle::HuddleValue {
            predicted_rework_avoided: 600,
            expected_failure_reduction: 300,
            expected_coordination_gain: 200,
            expected_budget_preservation: 100,
            inference_cost: 100,
            latency_cost: 50,
            context_cost: 50,
            opportunity_cost: 0,
        },
        debt: 400,
        rework_avoided: 600,
    })
    .unwrap();
    let huddle_report = serde_json::to_value(playbook_huddle::HuddleReport {
        participant: playbook_core::PersonnelId::new("agent1").unwrap(),
        verified_progress: vec![playbook_core::EvidenceId::new("ev-1").unwrap()],
        blockers: vec![playbook_huddle::Blocker {
            subject: "implementation".into(),
            reason: "compiler unavailable".into(),
            evidence_ref: None,
        }],
        changed_assumptions: vec![playbook_huddle::AssumptionDelta {
            subject: "workload".into(),
            previous: "engineering".into(),
            current: "research".into(),
            evidence_ref: Some(playbook_core::EvidenceId::new("ev-2").unwrap()),
        }],
        dependency_changes: vec![playbook_huddle::DependencyDelta {
            dependency: "crates/playbook-state".into(),
            change: "fold extended".into(),
            evidence_ref: None,
        }],
        contradictions: vec![playbook_huddle::Contradiction {
            id: playbook_huddle::ContradictionId::new("c-1").unwrap(),
            subject: "budget reserve".into(),
            claims: vec![playbook_huddle::ClaimRef {
                personnel_id: playbook_core::PersonnelId::new("agent1").unwrap(),
                statement: "reserve exhausted".into(),
                evidence_ref: None,
            }],
            severity: playbook_huddle::ContradictionSeverity::High,
        }],
        proposed_action: Some(playbook_huddle::PlayProposal {
            operation: "test.unit".into(),
            rationale: "re-verify the reopened gate".into(),
            target_gate: Some(playbook_core::GateId::new("unit_tests").unwrap()),
        }),
        confidence_milli: 800,
    })
    .unwrap();
    let huddle_outcome = serde_json::to_value(playbook_huddle::HuddleOutcome {
        huddle_id: playbook_core::HuddleId::new("huddle-1").unwrap(),
        reconciled_state: playbook_huddle::SharedStateDelta {
            verified_evidence: vec![playbook_core::EvidenceId::new("ev-1").unwrap()],
            gate_snapshots: vec![playbook_huddle::GateSnapshot {
                gate_id: playbook_core::GateId::new("unit_tests").unwrap(),
                weight_ppm: 500_000,
                satisfaction_ppm: 1_000_000,
                state: playbook_core::GateState::Satisfied,
            }],
            assumptions: Vec::new(),
        },
        resolved_contradictions: vec![playbook_huddle::ContradictionResolution {
            contradiction_id: playbook_huddle::ContradictionId::new("c-1").unwrap(),
            state: playbook_huddle::ResolutionState::ResolvedByEvidence,
            evidence_ref: Some(playbook_core::EvidenceId::new("ev-1").unwrap()),
            authority: None,
        }],
        unresolved_contradictions: Vec::new(),
        assignment_changes: vec![playbook_huddle::AssignmentChange {
            personnel_id: playbook_core::PersonnelId::new("agent2").unwrap(),
            from: None,
            to: "test.unit".into(),
        }],
        strategy_change: Some(playbook_huddle::StrategyChange {
            subject: "rush tendency".into(),
            from: "balanced".into(),
            to: "pass_heavy".into(),
            justification: "reserve stays above the rush line".into(),
        }),
        personnel_changes: vec![playbook_huddle::PersonnelChange {
            personnel_id: playbook_core::PersonnelId::new("agent3").unwrap(),
            action: playbook_huddle::PersonnelAction::Rest,
            reason: "context exhausted".into(),
        }],
        revised_ytg_milli: Some(3_000),
        debt_after: Some(320),
        next_play: None,
        next_huddle_conditions: vec![playbook_huddle::HuddleCondition {
            kind: playbook_huddle::HuddleTriggerKind::Failure,
            reason_code: "gate_regression".into(),
        }],
        escalated_to: None,
    })
    .unwrap();
    let coherence_state = serde_json::to_value(playbook_huddle::CoherenceState {
        debt: 450,
        band: playbook_huddle::DebtBand::HuddleEligible,
        active_huddles: 1,
        closed_huddles: 2,
        unresolved_contradictions: 0,
    })
    .unwrap();

    for (file, record) in [
        ("policy.json", policy),
        ("evidence.json", evidence),
        ("capability.json", capability),
        ("continuation.json", continuation),
        ("play_execution.json", execution.clone()),
        ("prior_table.json", prior_bundle),
        ("huddle_request.json", huddle_request),
        ("huddle_report.json", huddle_report.clone()),
        ("huddle_outcome.json", huddle_outcome),
        ("coherence_state.json", coherence_state),
    ] {
        let schema: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(file)).unwrap()).unwrap();
        assert_schema_subset(&schema, &record, file);
    }

    // The exact serialized execution includes every deny-unknown-fields member,
    // including the nullable idempotency key.
    assert!(execution["idempotency_key"].is_null() || execution["idempotency_key"].is_string());
    let mut missing = execution.clone();
    missing.as_object_mut().unwrap().remove("personnel_id");
    let execution_schema: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("play_execution.json")).unwrap())
            .unwrap();
    let mut nullable_key = execution.clone();
    nullable_key["idempotency_key"] = Value::Null;
    assert_schema_subset(
        &execution_schema,
        &nullable_key,
        "play_execution.json nullable key",
    );
    assert!(validate_schema_subset(&execution_schema, &missing).is_err());
    let mut extra = execution.clone();
    extra["unrecognized"] = json!(true);
    assert!(validate_schema_subset(&execution_schema, &extra).is_err());
    let mut wrong_type = execution;
    wrong_type["capability_generation"] = json!("one");
    assert!(validate_schema_subset(&execution_schema, &wrong_type).is_err());

    let prior_schema: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("prior_table.json")).unwrap())
            .unwrap();
    let mut negative_count = serde_json::to_value(playbook_core::PriorBundle {
        schema_version: 1,
        priors: {
            let mut table = playbook_core::HistoricalPriorTable::empty();
            table.record(&playbook_core::PersonnelId::new("agent1").unwrap(), 8_000);
            table
        },
    })
    .unwrap();
    negative_count["priors"]["rows"]["agent1"]["samples"] = json!(-1);
    assert!(validate_schema_subset(&prior_schema, &negative_count).is_err());

    let huddle_report_schema: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("huddle_report.json")).unwrap())
            .unwrap();
    let mut missing_blockers = huddle_report.clone();
    missing_blockers.as_object_mut().unwrap().remove("blockers");
    assert!(validate_schema_subset(&huddle_report_schema, &missing_blockers).is_err());
}

// These contracts use only this small JSON Schema subset. Keeping the test
// validator local avoids a runtime or development dependency on a validator
// crate while still checking serialized fixtures recursively.
fn assert_schema_subset(schema: &Value, record: &Value, name: &str) {
    if let Err(message) = validate_schema_subset(schema, record) {
        panic!("{name} serialized record does not match schema: {message}");
    }
}

fn validate_schema_subset(schema: &Value, value: &Value) -> Result<(), String> {
    const SUPPORTED: &[&str] = &[
        "$schema",
        "$id",
        "title",
        "description",
        "type",
        "const",
        "enum",
        "required",
        "properties",
        "additionalProperties",
        "items",
        "oneOf",
        "minimum",
    ];
    if let Some(object) = schema.as_object() {
        if let Some(keyword) = object.keys().find(|key| !SUPPORTED.contains(&key.as_str())) {
            return Err(format!("unsupported schema keyword `{keyword}`"));
        }
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = variants
            .iter()
            .filter(|variant| validate_schema_subset(variant, value).is_ok())
            .count();
        if matches != 1 {
            return Err(format!("value {value} matches {matches} oneOf variants"));
        }
    }
    if let Some(expected) = schema.get("type") {
        let valid = match expected {
            Value::String(kind) => matches_json_type(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| matches_json_type(kind, value)),
            _ => return Err("schema `type` must be a string or array".into()),
        };
        if !valid {
            return Err(format!("value {value} has the wrong JSON type"));
        }
    }
    if let Some(expected) = schema.get("const") {
        if expected != value {
            return Err(format!("value {value} does not match const {expected}"));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return Err(format!("value {value} is outside enum {allowed:?}"));
        }
    }
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64) {
        let Some(actual) = value.as_f64() else {
            return Err(format!("value {value} is not numeric for minimum"));
        };
        if actual < minimum {
            return Err(format!("value {actual} is below minimum {minimum}"));
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("missing required property `{key}`"));
                }
            }
        }
        let properties = schema.get("properties").and_then(Value::as_object);
        for (key, child) in object {
            if let Some(property_schema) = properties.and_then(|props| props.get(key)) {
                validate_schema_subset(property_schema, child)
                    .map_err(|error| format!("property `{key}`: {error}"))?;
            } else if let Some(additional) = schema.get("additionalProperties") {
                match additional {
                    Value::Bool(false) => return Err(format!("unexpected property `{key}`")),
                    Value::Object(_) => validate_schema_subset(additional, child)
                        .map_err(|error| format!("additional property `{key}`: {error}"))?,
                    _ => {}
                }
            }
        }
    }
    if let Some(items) = schema.get("items") {
        if let Some(array) = value.as_array() {
            for (index, item) in array.iter().enumerate() {
                validate_schema_subset(items, item)
                    .map_err(|error| format!("item {index}: {error}"))?;
            }
        }
    }
    Ok(())
}

fn matches_json_type(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn sample_measurement() -> Measurement {
    Measurement {
        validated_progress: false,
        material_failure: false,
        cycle_failure: false,
        failure_class: None,
        attempts: 0,
        progress_cycles: 0,
        ytg_milli: 10_000,
        previous_ytg_milli: 10_000,
        budget_remaining_ppm: PPM_SCALE,
        completion_ppm: 0,
        blocking_capability: false,
        blocking_unresolvable: false,
        mandatory_open: true,
        mandatory_failed: false,
        previous_down: Down::FirstDown,
        preflight_complete: true,
        stall_cycles: 0,
        possession_restarted: false,
        open_gate: Some("implementation".into()),
        count_regression: false,
        workload_class: "engineering".into(),
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}
