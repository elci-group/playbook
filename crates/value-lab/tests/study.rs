//! A miniature study, end to end: corpus frozen, faults scheduled, runs
//! recorded, verdict classified, scoreboard rendered. The whole §37
//! pipeline in one honest loop.

use value_lab::{
    bootstrap_ci, classify, corpus_for_study, emit_protocol, emit_scoreboard, median,
    pareto_frontier, read_results, scoreboard, write_results, FaultSchedule, FrozenThresholds,
    Regime, RunRecord, Scoreboard, Stratum, Verdict,
};

fn run(
    assignment: &str,
    regime: Regime,
    completed: bool,
    cost: u64,
    stratum: Stratum,
    faulted: bool,
) -> RunRecord {
    RunRecord {
        assignment_id: assignment.into(),
        stratum,
        regime,
        completed,
        cost_micros: cost,
        latency_ms: 4_000,
        tokens: 2_000,
        model_calls: 2,
        tool_calls: 3,
        retries: 0,
        failed_calls: 0,
        human_interventions: 0,
        validated_progress_milli: 800,
        constraint_compliant: true,
        catastrophic_failure: false,
        fault_injected: faulted,
        recovered: completed,
        complexity_tax_micros: 40,
        decisions: Vec::new(),
        ocav: Some(if completed { 120 } else { -40 }),
        cumulative_regret: 0,
    }
}

#[test]
fn a_miniature_season_produces_a_defensible_scoreboard() {
    let corpus = corpus_for_study();
    corpus
        .validate_primary()
        .expect("the study corpus is legal");
    let schedule = FaultSchedule::from_seed(11).assign(&corpus, 250_000);
    assert!(!schedule.is_empty(), "the fault campaign has fixtures");

    // The season: Playbook completes 90% at lower cost than the strongest
    // control's 50%, across two strata.
    let mut runs = Vec::new();
    let faulted_ids: Vec<String> = schedule.iter().map(|(id, _)| id.clone()).collect();
    for assignment in corpus.assignments() {
        let play_won = !assignment.id.ends_with("9");
        let control_won = assignment.id.ends_with('0') || assignment.id.ends_with('2');
        let stratum = assignment.stratum;
        let faulted = faulted_ids.contains(&assignment.id);
        runs.push(run(
            &assignment.id,
            Regime::P0,
            play_won,
            1_500,
            stratum,
            faulted,
        ));
        runs.push(run(
            &assignment.id,
            Regime::C1Strongest,
            control_won,
            3_000,
            stratum,
            faulted,
        ));
    }
    let verdict = classify(&runs, &FrozenThresholds::registered());
    assert_eq!(verdict, Verdict::ValueProven, "the headline result holds");

    let board = Scoreboard {
        rows: scoreboard(&runs),
        verdict,
    };
    let rendered = emit_scoreboard(&board);
    assert!(rendered.contains("Validated completion rate"));
    assert!(rendered.contains("90.0%"));
    assert!(rendered.contains("VALUE_PROVEN"));

    // The completion-cost distribution supports a bootstrap interval.
    let playbook_costs: Vec<i64> = runs
        .iter()
        .filter(|run| run.regime == Regime::P0 && run.completed)
        .map(|run| run.cost_micros as i64)
        .collect();
    let point = median(&playbook_costs).unwrap();
    let interval = bootstrap_ci(&playbook_costs, median, 500, 950_000, 11).unwrap();
    assert!(
        interval.low <= point && point <= interval.high,
        "{interval:?}"
    );

    // The protocol document froze the thresholds the verdict used.
    let protocol = emit_protocol();
    assert!(protocol.contains("100000 ppm"));
    assert!(protocol.contains("OCAV"));
}

#[test]
fn results_round_trip_and_the_pareto_frontier_hold() {
    let mut runs = Vec::new();
    for index in 0..20 {
        let id = format!("value-{index:03}");
        // P0: strong all-round. C1: pricier, no better. C5: cheap but weak.
        runs.push(run(
            &id,
            Regime::P0,
            index % 10 != 9,
            1_500,
            Stratum::Routine,
            false,
        ));
        runs.push(run(
            &id,
            Regime::C1Strongest,
            index % 10 != 9,
            3_000,
            Stratum::Routine,
            false,
        ));
        runs.push(run(
            &id,
            Regime::C5Random,
            index % 4 == 0,
            400,
            Stratum::Routine,
            false,
        ));
    }
    // Round trip: the results file is the record (§31).
    let path = std::env::temp_dir().join("value-lab-roundtrip.jsonl");
    write_results(&path, &runs).unwrap();
    let read_back = read_results(&path).unwrap();
    assert_eq!(read_back, runs, "the results file reproduces the runs");
    let _ = std::fs::remove_file(&path);

    // The frontier keeps the efficient shapes and drops the dominated one
    // (§25): C1 is never better than P0, so it leaves the frontier.
    let frontier = pareto_frontier(&runs);
    let on_frontier: Vec<&str> = frontier.iter().map(|vector| vector.regime.key()).collect();
    assert!(on_frontier.contains(&"P0_playbook"));
    assert!(
        on_frontier.contains(&"C5_random"),
        "cheap-and-weak can be efficient"
    );
    assert!(
        !on_frontier.contains(&"C1_strongest"),
        "dominated on every axis"
    );
}

#[test]
fn a_real_journal_feeds_the_counterfactual_pipeline() {
    let path = std::env::temp_dir().join("value-lab-pipeline.jsonl");
    let _ = std::fs::remove_file(&path);
    let report = playbook_sim::run_mvp_with_journal(&path).unwrap();
    let snapshots =
        value_lab::decision_snapshots(&report.events, &playbook_policy::Policy::builtin()).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(!snapshots.is_empty());
}
