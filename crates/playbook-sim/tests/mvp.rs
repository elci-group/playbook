use playbook_sim::{compare_policies, run_mvp, Disposition, Down, DriveStatus, PersonnelKind};
use playbook_sim::{fold, ApplyMode, HistoricalPriorTable, Policy};

#[test]
fn the_acceptance_drive_replays_and_punts() {
    let report = run_mvp().expect("mvp drive");
    assert!(report.stages.windows(2).all(|pair| pair[0] != pair[1]));
    let at = |name: &str| {
        report
            .stages
            .iter()
            .position(|stage| stage == name)
            .unwrap()
    };
    assert!(at("FIRST_DOWN") < at("SECOND_DOWN_GAIN"));
    assert!(at("SECOND_DOWN_GAIN") < at("THIRD_DOWN_PASS"));
    assert!(at("THIRD_DOWN_PASS") < at("SECOND_DOWN_LOSS"));
    assert!(at("SECOND_DOWN_LOSS") < at("FOURTH_DOWN"));
    assert!(at("FOURTH_DOWN") < at("THIRD_DOWN_RUSH"));
    assert_eq!(report.disposition, Some(Disposition::Punt));
    assert!(report.final_state.package().is_some());
    assert!(!report.package_leaks);
    assert!(report.replay_match);
    assert_eq!(report.final_state.status(), DriveStatus::Completed);
    assert_eq!(report.final_state.ytg_milli(), 0);
    assert_eq!(report.final_state.down(), Down::ThirdDownRush);
    assert!(report.false_completion_claims >= 1);
    assert_eq!(report.duplicate_failed_plays, 0);
    assert!(report.control_duplicate_failed_plays >= 1);
    assert_eq!(
        report.finishing_personnel_kind,
        Some(PersonnelKind::DeterministicProcedure)
    );
    let package = report.final_state.package().unwrap();
    let own = report
        .final_state
        .assessments()
        .iter()
        .find(|person| person.id.as_str() == "coding-specialist")
        .and_then(|person| person.own_ytg_milli);
    assert_ne!(own, Some(package.objective_state.measured_ytg_milli));
    let comparison = compare_policies(&report.events).unwrap();
    assert!(comparison.ytg_match);
    assert!(comparison.budget_match);
    assert!(comparison.completion_match);
    assert!(comparison.down_labels_differ);
    assert!(comparison.tool_failures >= 1);
    assert!(comparison.fourth_down_index.is_some());
    assert_eq!(comparison.gates_satisfied, 3);
    for ytg in [10_000, 6_000, 3_000, 0] {
        assert!(comparison.ytg_trajectory.contains(&ytg));
    }
    assert!(comparison.monetary_cost_micros > 0);
    assert_eq!(comparison.completion_status, "Completed");
    assert_eq!(comparison.yards_gained_ppm, 1_000_000);
    assert!(comparison.efficiency_ratio_milli > 0);
}

#[test]
fn the_acceptance_drive_learns_priors_from_measured_outcomes() {
    let report = run_mvp().expect("mvp drive");
    let priors = report.final_state.priors();
    // Survey, compile, unit, induced failure, finish: five recorded plays,
    // all by the deterministic procedure the router prefers.
    assert_eq!(priors.samples(), 5);
    assert!(!priors.is_empty());
    let cargo = playbook_sim::PersonnelId::new("cargo-toolchain").unwrap();
    let row = priors.row(&cargo).unwrap();
    assert_eq!(row.samples, 5);
    // 0 (survey) + 4_000 (compile) + 3_000 (unit) + 0 (failure) + 3_000
    // (finish): the induction failure is a measured zero, not a gap.
    assert_eq!(row.realized_reduction_sum_milli, 10_000);
    assert_eq!(priors.prior_milli(&cargo, 3), Some(2_000));
    // Above the recorded sample count no prior exists yet.
    assert_eq!(priors.prior_milli(&cargo, 6), None);
    // Replays reproduce the table exactly, in both modes.
    let policy = Policy::builtin();
    let audit = fold(&report.events, &policy, ApplyMode::Audit).unwrap();
    let resim = fold(&report.events, &policy, ApplyMode::Resimulate).unwrap();
    assert_eq!(audit.priors(), priors);
    assert_eq!(resim.priors(), priors);
    // An empty table stays empty.
    assert_eq!(HistoricalPriorTable::empty().samples(), 0);
}

#[test]
fn sprint_transcript_is_a_roundtrippable_audited_fixture() {
    let report = run_mvp().expect("mvp drive");
    let encoded = serde_json::to_vec_pretty(&report.events).unwrap();
    let decoded: Vec<playbook_sim::DriveEvent> = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, report.events);
    assert_eq!(
        fold(&report.events, &Policy::builtin(), ApplyMode::Audit).unwrap(),
        report.final_state
    );
    let fixture = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/sprint_transcript.json"),
    )
    .unwrap();
    let fixture_events: Vec<playbook_sim::DriveEvent> = serde_json::from_slice(&fixture).unwrap();
    assert_eq!(fixture_events, report.events);
}
