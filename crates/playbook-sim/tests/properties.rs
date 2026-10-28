use std::path::Path;

use playbook_sim::{
    authorize, classify_measurement, fold, gate_one, leaks_competitive_metadata, open_drive,
    run_mvp, try_tov_normalise, ApplyMode, Down, DriveBudget, DriveEvent, Evidence, EvidenceClass,
    FixturePlane, GateId, Measurement, PlayStatus, PlaybookError, Policy, SideEffectClass,
    PPM_SCALE,
};
use serde_json::json;

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
fn schemas_name_the_contract_fields() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    for (file, keys) in [
        (
            "policy.json",
            &["version", "plays", "rush_budget_fraction_ppm"][..],
        ),
        (
            "evidence.json",
            &["id", "class", "body", "observed_at_ms"][..],
        ),
        (
            "capability.json",
            &["id", "operations", "evidence_classes", "side_effect_class"][..],
        ),
        (
            "continuation.json",
            &["magic", "objective_state", "evidence_refs", "budget_state"][..],
        ),
        (
            "play_execution.json",
            &[
                "drive_id",
                "play_id",
                "operation",
                "budget_ceiling",
                "idempotency_key",
            ][..],
        ),
    ] {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        for key in keys {
            assert!(text.contains(&format!("\"{key}\"")), "{file} missing {key}");
        }
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
