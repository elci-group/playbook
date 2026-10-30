//! The acceptance drive. Downs are read from the event log.

use playbook_core::{
    Disposition, Down, DriveBudget, DriveStatus, Evidence, EvidenceClass, FailureClass, GateState,
    PersonnelKind, PlaybookError, TaskIntegrity, ToolFailureKind, PPM_SCALE,
};
use playbook_elci::{CapabilityPlane, FixturePlane, PlaneResult};
use playbook_events::DriveEvent;
use playbook_lastdown::package_leaks;
use playbook_policy::Policy;
use playbook_routing::HistoricalPriorTable;
use playbook_state::{fold, started_budget, trajectory, ApplyMode, Authority};
use std::collections::HashMap;
use std::path::Path;

use crate::harness::{person, roster, standard_gates, task_contract};

#[derive(Clone, Debug)]
pub struct MvpReport {
    pub stages: Vec<String>,
    pub events: Vec<DriveEvent>,
    pub final_state: playbook_state::DriveState,
    pub replay_match: bool,
    pub disposition: Option<Disposition>,
    pub duplicate_failed_plays: u32,
    pub control_duplicate_failed_plays: u32,
    pub false_completion_claims: u32,
    pub package_leaks: bool,
    pub finishing_personnel_kind: Option<PersonnelKind>,
    pub journal_replay_match: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyComparison {
    pub ytg_match: bool,
    pub budget_match: bool,
    pub completion_match: bool,
    pub down_labels_differ: bool,
    pub completion_status: String,
    pub ytg_trajectory: Vec<u32>,
    pub budget_remaining_ppm: u32,
    pub tool_failures: u32,
    pub model_calls: u32,
    pub personnel_changes: u32,
    pub fourth_down_index: Option<usize>,
    pub wall_clock_ms: u64,
    pub monetary_cost_micros: u64,
    pub gates_satisfied: u32,
    pub yards_gained_ppm: i64,
    pub budget_spent_ppm: i64,
    pub efficiency_ratio_milli: i64,
}

pub #[tracing::instrument]
fn stage_names(events: &[DriveEvent]) -> Vec<String> {
    let mut stages = Vec::new();
    for event in events {
        if let DriveEvent::DownChanged { down, .. } = event {
            let name = down.machine_id().to_string();
            if stages.last() != Some(&name) {
                stages.push(name);
            }
        }
    }
    stages
}

pub #[tracing::instrument]
fn run_mvp() -> Result<MvpReport, PlaybookError> {
    run_mvp_inner(None)
}

/// Run the acceptance drive through a durable journal, then recover a second
/// authority from the file. The recovered log must equal the live log.
pub #[tracing::instrument]
fn run_mvp_with_journal(journal_path: &Path) -> Result<MvpReport, PlaybookError> {
    run_mvp_inner(Some(journal_path))
}

#[tracing::instrument]
fn run_mvp_inner(journal_path: Option<&Path>) -> Result<MvpReport, PlaybookError> {
    let mut plane = FixturePlane::mvp();
    let mut authority = match journal_path {
        Some(path) => {
            let journal = playbook_journal::Journal::create(path)?;
            Authority::start_journaled(
                playbook_core::DriveId::new("drive-mvp")?,
                task_contract(TaskIntegrity::Sound),
                started_budget(1_000, 1_000),
                Policy::builtin(),
                roster(),
                1_000,
                Box::new(journal),
            )?
        }
        None => Authority::start(
            playbook_core::DriveId::new("drive-mvp")?,
            task_contract(TaskIntegrity::Sound),
            started_budget(1_000, 1_000),
            Policy::builtin(),
            roster(),
            1_000,
        )?,
    };
    for gate in standard_gates()? {
        authority.declare_gate(gate)?;
    }
    expect(&authority, "gates", 10_000, Down::FirstDown)?;
    authority.preflight(&mut plane)?;
    expect(&authority, "preflight", 10_000, Down::FirstDown)?;

    let survey = authority.step(&mut plane, 1_100)?;
    if survey.selected_operation != "inspect" {
        return Err(diverged("first play", &survey.selected_operation));
    }
    expect(&authority, "survey", 10_000, Down::FirstDown)?;

    let compile = authority.step(&mut plane, 2_000)?;
    if compile.selected_operation != "compile" {
        return Err(diverged("compile", &compile.selected_operation));
    }
    expect(&authority, "compile", 6_000, Down::SecondDownGain)?;

    let unit = authority.step(&mut plane, 3_000)?;
    if unit.selected_operation != "test.unit" {
        return Err(diverged("unit", &unit.selected_operation));
    }
    expect(&authority, "unit", 3_000, Down::ThirdDownPass)?;
    if authority.state().personnel().map(|person| person.kind)
        != Some(PersonnelKind::DeterministicProcedure)
    {
        return Err(diverged("unit personnel", "specialist"));
    }

    let failed = authority.step(&mut plane, 4_000)?;
    if failed.selected_operation != "test.integration" {
        return Err(diverged("induced failure", &failed.selected_operation));
    }
    expect(&authority, "timeout", 3_000, Down::SecondDownLoss)?;
    if authority.state().failure_class() != Some(FailureClass::ToolFailure) {
        return Err(diverged("failure class", "expected tool failure"));
    }
    if authority.state().last_failure_kind() != Some(ToolFailureKind::Timeout) {
        return Err(diverged("failure kind", "expected timeout"));
    }

    let before_retry = authority.state().ytg_milli();
    let retry = authority.try_operation("test.integration", &mut plane, 4_100);
    if !matches!(retry, Err(PlaybookError::BlindRetry(_))) {
        return Err(diverged(
            "blind retry",
            "the same integration play was admitted",
        ));
    }
    if authority.state().ytg_milli() != before_retry {
        return Err(diverged("blind retry", "state changed"));
    }

    authority.pressure(
        DriveBudget {
            provider_quota: 900,
            ..DriveBudget::zero()
        },
        5_000,
    )?;
    expect(&authority, "pressure", 3_000, Down::FourthDown)?;
    if authority.state().disposition() != Some(Disposition::Punt) {
        return Err(diverged(
            "doctrine",
            &format!("{:?}", authority.state().disposition()),
        ));
    }
    authority.punt()?;
    let package = authority
        .state()
        .package()
        .cloned()
        .ok_or_else(|| diverged("punt", "missing package"))?;
    if package_leaks(&package) {
        return Err(PlaybookError::AmnesiaLeak);
    }
    let measured = package.objective_state.measured_ytg_milli;

    authority.receive(
        person(
            "coding-specialist",
            PersonnelKind::SpecialistAgent,
            5_000,
            None,
        ),
        started_budget(1_000, 1_000),
        6_000,
    )?;
    expect(&authority, "possession", 3_000, Down::ThirdDownRush)?;
    let mut assessed = roster();
    assessed[0].own_ytg_milli = Some(2_500);
    assessed[2].own_ytg_milli = Some(2_800);
    authority.record_assessments(assessed)?;
    let own = authority
        .state()
        .assessments()
        .iter()
        .find(|person| person.id.as_str() == "coding-specialist")
        .and_then(|person| person.own_ytg_milli);
    if own == Some(measured) || own.is_none() {
        return Err(diverged(
            "assessment",
            "own estimate matched the sealed measurement",
        ));
    }

    authority.preflight(&mut plane)?;
    let narrative = Evidence::seal(
        EvidenceClass::NarrativeClaim,
        serde_json::json!({"text": "implemented successfully"}),
        7_000,
    )?;
    let ytg_before_claim = authority.state().ytg_milli();
    authority.accept_evidence(narrative)?;
    if authority.state().ytg_milli() != ytg_before_claim {
        return Err(diverged("narrative", "ytg moved"));
    }
    if authority.claim_complete(7_100)? {
        return Err(diverged("false completion", "open gates were accepted"));
    }

    let finish = authority.step(&mut plane, 9_000)?;
    if finish.selected_operation != "test.integration" {
        return Err(diverged("finish", &finish.selected_operation));
    }
    if authority.state().personnel().map(|person| person.kind)
        != Some(PersonnelKind::DeterministicProcedure)
    {
        return Err(diverged(
            "finish personnel",
            "expected deterministic toolchain",
        ));
    }
    if authority.state().ytg_milli() != 0 {
        return Err(diverged(
            "finish ytg",
            &authority.state().ytg_milli().to_string(),
        ));
    }
    if !authority.claim_complete(9_100)? {
        return Err(diverged("completion", "gates remained open"));
    }
    if authority.state().status() != DriveStatus::Completed {
        return Err(diverged("status", "drive did not complete"));
    }
    if HistoricalPriorTable::empty().samples() != 0 {
        return Err(diverged("priors", "historical table is not empty"));
    }

    let events = authority.log().to_vec();
    let replay = fold(&events, authority.policy(), ApplyMode::Audit)?;
    let journal_replay_match = match journal_path {
        Some(path) => {
            let recovery = playbook_journal::Journal::open(path)?;
            Some(
                recovery.events == events
                    && fold(&recovery.events, authority.policy(), ApplyMode::Audit)?
                        == *authority.state(),
            )
        }
        None => None,
    };
    Ok(MvpReport {
        stages: stage_names(&events),
        duplicate_failed_plays: duplicate_failures(&events),
        control_duplicate_failed_plays: control_duplicate_failures()?,
        false_completion_claims: authority.state().false_completion_claims(),
        package_leaks: package_leaks(&package),
        finishing_personnel_kind: authority.state().personnel().map(|person| person.kind),
        disposition: Some(Disposition::Punt),
        replay_match: replay == *authority.state(),
        final_state: authority.state().clone(),
        events,
        journal_replay_match,
    })
}

pub #[tracing::instrument]
fn compare_policies(events: &[DriveEvent]) -> Result<PolicyComparison, PlaybookError> {
    let candidate = Policy::pass_heavy_contrast();
    compare_policies_with(events, &candidate)
}

/// Compare the recorded event sequence under the builtin audit policy and a
/// selected candidate policy. This reclassifies the same historical events;
/// it does not generate new actions or predict a counterfactual drive.
pub #[tracing::instrument]
fn compare_policies_with(
    events: &[DriveEvent],
    candidate: &Policy,
) -> Result<PolicyComparison, PlaybookError> {
    let current = Policy::builtin();
    let audit = trajectory(events, &current, ApplyMode::Audit)?;
    let resim = trajectory(events, candidate, ApplyMode::Resimulate)?;
    let audit_final = fold(events, &current, ApplyMode::Audit)?;
    let resim_final = fold(events, candidate, ApplyMode::Resimulate)?;
    let ytg_trajectory: Vec<u32> = audit.iter().map(|point| point.ytg_milli).collect();
    // Gate declarations precede the first derived YTG point, whose value is zero;
    // use the observed maximum as the drive's starting YTG for efficiency.
    let ytg_before = ytg_trajectory.iter().copied().max().unwrap_or(0);
    let first_budget = audit.first().map_or(0, |point| point.budget_remaining_ppm);
    let last_budget = audit.last().map_or(0, |point| point.budget_remaining_ppm);
    let yards_gained_ppm = if ytg_before == 0 {
        0
    } else {
        (i64::from(ytg_before) - i64::from(audit_final.ytg_milli())) * i64::from(PPM_SCALE)
            / i64::from(ytg_before)
    };
    // This is the net change between the first and last normalized remaining
    // budget observations. A new possession can reallocate budget, so this is
    // not cumulative consumption across a multi-possession trace.
    let budget_spent_ppm = i64::from(first_budget) - i64::from(last_budget);
    let efficiency_ratio_milli = if budget_spent_ppm <= 0 {
        0
    } else {
        yards_gained_ppm * 1000 / budget_spent_ppm
    };
    let model_calls = count_model_calls(events);
    let budget = audit_final.budget_consumed();
    Ok(PolicyComparison {
        ytg_match: audit_final.ytg_milli() == resim_final.ytg_milli(),
        budget_match: audit_final.budget_consumed() == resim_final.budget_consumed(),
        completion_match: audit_final.status() == resim_final.status(),
        down_labels_differ: audit
            .iter()
            .zip(resim.iter())
            .any(|(left, right)| left.down != right.down),
        completion_status: format!("{:?}", audit_final.status()),
        ytg_trajectory,
        budget_remaining_ppm: last_budget,
        tool_failures: count_unsuccessful_tool_outcomes(events),
        model_calls,
        personnel_changes: events
            .iter()
            .filter(|event| matches!(event, DriveEvent::PersonnelChanged { .. }))
            .count() as u32,
        fourth_down_index: audit
            .iter()
            .find(|point| point.down == Down::FourthDown)
            .map(|point| point.index),
        wall_clock_ms: budget.wall_clock_ms,
        monetary_cost_micros: budget.monetary_cost_micros,
        gates_satisfied: audit_final
            .gates()
            .iter()
            .filter(|gate| matches!(gate.state, GateState::Satisfied | GateState::Waived))
            .count() as u32,
        yards_gained_ppm,
        budget_spent_ppm,
        efficiency_ratio_milli,
    })
}

#[tracing::instrument]
fn count_model_calls(events: &[DriveEvent]) -> u32 {
    let mut personnel_kinds = HashMap::new();
    let mut count = 0u32;
    for event in events {
        match event {
            DriveEvent::PersonnelChanged { personnel }
            | DriveEvent::PossessionStarted { personnel, .. } => {
                personnel_kinds.insert(personnel.id.clone(), personnel.kind);
            }
            DriveEvent::ToolInvoked { execution }
                if personnel_kinds
                    .get(&execution.personnel_id)
                    .is_some_and(|kind| {
                        matches!(kind, PersonnelKind::Model | PersonnelKind::SpecialistAgent)
                    }) =>
            {
                count = count.saturating_add(1);
            }
            _ => {}
        }
    }
    count
}

#[tracing::instrument]
fn count_unsuccessful_tool_outcomes(events: &[DriveEvent]) -> u32 {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                DriveEvent::ToolFailed { .. } | DriveEvent::ToolOutcomeRejected { .. }
            )
        })
        .count() as u32
}

pub #[tracing::instrument]
fn control_duplicate_failures() -> Result<u32, PlaybookError> {
    let mut plane = FixturePlane::failing_twice();
    let generation = plane.discover()?.generation;
    let mut duplicates = 0u32;
    let mut previous_failed = false;
    for index in 0..3 {
        let execution = control_execution(generation, index)?;
        match plane.execute(&execution)? {
            PlaneResult::Failed(_) => {
                if previous_failed {
                    duplicates = duplicates.saturating_add(1);
                }
                previous_failed = true;
            }
            PlaneResult::Done(_) => previous_failed = false,
        }
    }
    Ok(duplicates)
}

#[tracing::instrument]
fn control_execution(
    generation: u64,
    index: u32,
) -> Result<playbook_core::PlayExecution, PlaybookError> {
    use playbook_core::{
        CapabilityId, DriveId, PersonnelId, PlayExecution, PlayId, PlayKind, RetryPolicy,
        SideEffectClass,
    };
    Ok(PlayExecution {
        drive_id: DriveId::new("control-drive")?,
        play_id: PlayId::new(format!("control-{index}"))?,
        play_kind: PlayKind::RunDeterministicTest,
        operation: "test.integration".into(),
        down: Down::ThirdDownPass,
        capability_id: CapabilityId::new("elci.cargo")?,
        capability_generation: generation,
        personnel_id: PersonnelId::new("cargo-toolchain")?,
        inputs: serde_json::json!({}),
        expected_evidence: vec![EvidenceClass::TestReport],
        expected_ytg_reduction_milli: 3_000,
        survey: false,
        budget_ceiling: started_budget(1_000, 1_000),
        timeout_ms: 1,
        retry_policy: RetryPolicy::NoRetry,
        side_effect: SideEffectClass::ReadOnly,
        idempotency_key: Some(format!("control:{index}")),
    })
}

#[tracing::instrument]
fn duplicate_failures(events: &[DriveEvent]) -> u32 {
    let mut seen = std::collections::BTreeSet::new();
    let mut duplicates = 0u32;
    for event in events {
        if let DriveEvent::ToolFailed { failure } = event {
            if !seen.insert(failure.operation.clone()) {
                duplicates = duplicates.saturating_add(1);
            }
        }
    }
    duplicates
}

#[tracing::instrument]
fn expect(authority: &Authority, label: &str, ytg: u32, down: Down) -> Result<(), PlaybookError> {
    let state = authority.state();
    if state.ytg_milli() != ytg || state.down() != down {
        return Err(PlaybookError::ReplayDivergence(format!(
            "{label}: down {} ytg {} budget {} ppm completion {} failure {:?}",
            state.down().machine_id(),
            state.ytg_milli(),
            state.budget_remaining_ppm(authority.policy()),
            state.completion_ppm(),
            state.failure_class()
        )));
    }
    Ok(())
}

#[tracing::instrument]
fn diverged(label: &str, detail: &str) -> PlaybookError {
    PlaybookError::ReplayDivergence(format!("{label}: {detail}"))
}

#[cfg(test)]
mod comparison_tests {
    use super::*;
    use crate::harness::person;

    #[test]
    #[tracing::instrument]
fn model_calls_use_personnel_kind_at_invocation_time() {
        let report = run_mvp().unwrap();
        let execution = report
            .events
            .iter()
            .find_map(|event| match event {
                DriveEvent::ToolInvoked { execution } => Some(execution.clone()),
                _ => None,
            })
            .unwrap();
        let personnel_id = execution.personnel_id.as_str();
        let events = vec![
            DriveEvent::PersonnelChanged {
                personnel: person(personnel_id, PersonnelKind::Model, 1, None),
            },
            DriveEvent::ToolInvoked {
                execution: execution.clone(),
            },
            DriveEvent::PersonnelChanged {
                personnel: person(personnel_id, PersonnelKind::DeterministicProcedure, 1, None),
            },
            DriveEvent::ToolInvoked { execution },
        ];
        assert_eq!(count_model_calls(&events), 1);
    }

    #[test]
    #[tracing::instrument]
fn rejected_tool_outcomes_count_as_unsuccessful_executions() {
        let event = DriveEvent::ToolOutcomeRejected {
            result: None,
            failure: None,
            reason: "response could not be committed".into(),
        };
        assert_eq!(count_unsuccessful_tool_outcomes(&[event]), 1);
    }
}
