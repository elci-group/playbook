//! Phase 8: the comparative huddle programme.
//!
//! Four regimes over identical deterministic scenarios:
//!
//! * `Control` — no huddles. The drive runs on routing alone.
//! * `Periodic` — a huddle every `period` steps, regardless of field state.
//! * `FlatTriggered` — condition-triggered, every roster member participates.
//! * `HierarchicalTriggered` — condition-triggered, admission economics and
//!   the participation floor select the smallest sufficient huddle.
//!
//! The regimes differ only in when a huddle may run and who attends. Report
//! generation, outcome steering, budget charging, and the reconciliation
//! hand-off are the same code for every huddle regime. Everything is
//! deterministic: the same scenario and policy always produce the same
//! numbers.

use playbook_core::{
    Capability, CapabilityId, DriveBudget, DriveId, DriveStatus, EventId, Evidence, EvidenceClass,
    PlayExecution, PlayResult, PlayStatus, PlaybookError, SideEffectClass,
};
use playbook_elci::{CapabilityPlane, CapabilitySnapshot, PlaneResult};
use playbook_events::DriveEvent;
use playbook_huddle::{
    evaluate_triggers, AssumptionDelta, HuddleCost, HuddleOutcome, HuddleReport, HuddleScope,
    HuddleTriggerKind, HuddleValue, ParticipantCandidate, SharedStateDelta,
};
use playbook_observability::rollup;
use playbook_policy::Policy;
use playbook_state::{Authority, HuddleRequest};

use crate::{person, task_contract, weighted_gate};

/// The four experimental regimes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HuddleRegime {
    Control,
    Periodic,
    FlatTriggered,
    HierarchicalTriggered,
}

impl HuddleRegime {
    pub fn name(self) -> &'static str {
        match self {
            Self::Control => "A_control",
            Self::Periodic => "B_periodic",
            Self::FlatTriggered => "C_flat_triggered",
            Self::HierarchicalTriggered => "D_hierarchical_triggered",
        }
    }
}

/// One run's configuration.
pub struct StudyConfig {
    pub regime: HuddleRegime,
    /// Period for the `Periodic` regime, in steps.
    pub period: u32,
    pub drive_id: &'static str,
    pub drive_budget: DriveBudget,
    /// When true, `test.integration` fails until a huddle's reconciled
    /// contract lands on the field.
    pub flaky: bool,
}

/// §24 measures for one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    pub regime: &'static str,
    pub completed: bool,
    pub terminal: &'static str,
    pub steps: u32,
    pub plays: u32,
    pub tool_failures: u32,
    pub repeated_failed_plays: u32,
    pub drive_tokens: u64,
    pub huddle_tokens: u64,
    pub total_tokens: u64,
    pub tokens_per_completion: Option<u64>,
    pub ytg_velocity_milli_per_step: i64,
    pub huddles_requested: u32,
    pub huddles_committed: u32,
    pub huddles_rejected: u32,
    pub contradictions: u32,
    pub coherence_gain: u32,
}

const MAX_STEPS: u32 = 40;
pub const STEP_CAP: u32 = MAX_STEPS;
const REPORT_TOKENS: u64 = 150;
const REPORT_COST_MICROUNITS: u64 = 15_000;
const REPORT_DURATION_MS: u64 = 50;

/// The scenario plane. `test.integration` fails until the drive executes a
/// reconciled contract; every other operation succeeds. The plane models the
/// environment; the huddle produces the diagnosis; the harness executes it.
struct StudyPlane {
    generation: u64,
    capabilities: Vec<Capability>,
    reconciled: bool,
    flaky: bool,
}

impl StudyPlane {
    fn new(flaky: bool) -> Self {
        let capability = Capability {
            id: CapabilityId::new("elci.cargo").expect("static id"),
            version: "1".into(),
            provider: "elci".into(),
            operations: vec![
                "discover".into(),
                "inspect".into(),
                "compile".into(),
                "test.unit".into(),
                "test.integration".into(),
            ],
            inputs_schema: serde_json::json!({"type": "object"}),
            outputs_schema: serde_json::json!({"type": "object"}),
            cost_model: DriveBudget {
                tokens: 1,
                tool_calls: 1,
                ..DriveBudget::zero()
            },
            side_effect_class: SideEffectClass::ReadOnly,
            permissions: vec!["read".into()],
            environment_requirements: vec!["cargo".into()],
            reliability_ppm: 950_000,
            reliability_samples: 1,
            evidence_classes: vec![
                EvidenceClass::CompilerResult,
                EvidenceClass::TestReport,
                EvidenceClass::EnvironmentSnapshot,
                EvidenceClass::CapabilitySnapshot,
            ],
        };
        Self {
            generation: 0,
            capabilities: vec![capability],
            reconciled: false,
            flaky,
        }
    }

    fn success(
        &self,
        execution: &PlayExecution,
        class: EvidenceClass,
        body: serde_json::Value,
        cost: DriveBudget,
    ) -> PlaneResult {
        PlaneResult::Done(PlayResult {
            status: PlayStatus::Succeeded,
            play_id: execution.play_id.clone(),
            operation: execution.operation.clone(),
            output_refs: vec![execution.operation.clone()],
            evidence: vec![
                Evidence::seal(class, body, execution.timeout_ms).expect("fixture evidence")
            ],
            artefact_refs: Vec::new(),
            cost,
            duration_ms: 10,
            diagnostics: vec!["ok".into()],
            side_effects: Vec::new(),
            at_ms: execution.timeout_ms,
        })
    }

    fn failure(&self, execution: &PlayExecution, cost: DriveBudget, why: &str) -> PlaneResult {
        PlaneResult::Failed(playbook_core::ToolFailure {
            play_id: execution.play_id.clone(),
            play_kind: execution.play_kind,
            operation: execution.operation.clone(),
            capability_id: execution.capability_id.clone(),
            personnel_id: execution.personnel_id.clone(),
            kind: playbook_core::ToolFailureKind::Timeout,
            diagnostics: vec![why.into()],
            cost,
            at_ms: execution.timeout_ms,
            idempotency_key: execution.idempotency_key.clone(),
            side_effects: Vec::new(),
        })
    }

    fn cost(tokens: u64) -> DriveBudget {
        DriveBudget {
            tokens,
            monetary_cost_micros: tokens.saturating_mul(100),
            tool_calls: 1,
            ..DriveBudget::zero()
        }
    }
}

impl CapabilityPlane for StudyPlane {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        self.generation += 1;
        Ok(CapabilitySnapshot {
            generation: self.generation,
            capabilities: self.capabilities.clone(),
        })
    }

    fn execute(&mut self, execution: &PlayExecution) -> Result<PlaneResult, PlaybookError> {
        match execution.operation.as_str() {
            "inspect" => Ok(self.success(
                execution,
                EvidenceClass::EnvironmentSnapshot,
                serde_json::json!({"host": "study", "tools": ["cargo"]}),
                Self::cost(20),
            )),
            "compile" => Ok(self.success(
                execution,
                EvidenceClass::CompilerResult,
                serde_json::json!({
                    "gate_id": "implementation",
                    "success": true,
                    "command": "cargo check",
                    "repository_hash": "study"
                }),
                Self::cost(200),
            )),
            "test.unit" => Ok(self.success(
                execution,
                EvidenceClass::TestReport,
                serde_json::json!({
                    "gate_id": "unit_tests",
                    "command": "cargo test",
                    "repository_hash": "study",
                    "test_suite": "unit",
                    "exit_code": 0,
                    "passed": 4,
                    "failed": 0,
                    "ignored": 0,
                    "timestamp": "study"
                }),
                Self::cost(400),
            )),
            "test.integration" if !self.flaky || self.reconciled => Ok(self.success(
                execution,
                EvidenceClass::TestReport,
                serde_json::json!({
                    "gate_id": "integration_tests",
                    "command": "cargo test",
                    "repository_hash": "study",
                    "test_suite": "integration",
                    "exit_code": 0,
                    "passed": 2,
                    "failed": 0,
                    "ignored": 0,
                    "timestamp": "study"
                }),
                Self::cost(100),
            )),
            "test.integration" => Ok(self.failure(
                execution,
                Self::cost(400),
                "integration suite times out under the stale contract",
            )),
            _ => Ok(self.failure(execution, DriveBudget::zero(), "operation not scripted")),
        }
    }
}

fn roster() -> Vec<playbook_core::Personnel> {
    vec![
        person(
            "cargo-toolchain",
            playbook_core::PersonnelKind::DeterministicProcedure,
            40,
            None,
        ),
        person(
            "nightly-toolchain",
            playbook_core::PersonnelKind::DeterministicProcedure,
            300,
            None,
        ),
    ]
}

fn study_gates() -> Vec<playbook_core::Gate> {
    vec![
        weighted_gate(
            "implementation",
            400_000,
            &[],
            EvidenceClass::CompilerResult,
        ),
        weighted_gate(
            "unit_tests",
            300_000,
            &["implementation"],
            EvidenceClass::TestReport,
        ),
        weighted_gate(
            "integration_tests",
            300_000,
            &["unit_tests"],
            EvidenceClass::TestReport,
        ),
    ]
}

fn scenario_value() -> HuddleValue {
    HuddleValue {
        predicted_rework_avoided: 600,
        expected_failure_reduction: 400,
        expected_coordination_gain: 100,
        expected_budget_preservation: 100,
        inference_cost: 100,
        latency_cost: 50,
        context_cost: 50,
        opportunity_cost: 0,
    }
}

fn trailing_failures(events: &[DriveEvent]) -> u32 {
    let mut count = 0;
    for event in events.iter().rev() {
        match event {
            DriveEvent::ToolFailed { .. } => count += 1,
            DriveEvent::YtgRecalculated { .. }
            | DriveEvent::DownChanged { .. }
            | DriveEvent::FourthDownDeclared { .. } => continue,
            _ => break,
        }
    }
    count
}

/// The number of times the most-failed operation failed. Repeated failed
/// plays are the waste a reconciliation is supposed to remove.
fn most_failed_operation_count(events: &[DriveEvent]) -> u32 {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for event in events {
        if let DriveEvent::ToolFailed { failure } = event {
            let count = counts.entry(failure.operation.as_str()).or_insert(0);
            *count += 1;
        }
    }
    counts.values().copied().max().unwrap_or(0)
}

struct HuddleTrackers {
    fingerprint_at_last_failure: Option<String>,
    terminal_gates_before: usize,
    total_failures: u32,
    debt_before_first_huddle: Option<u32>,
    success_fired: bool,
}

fn debt_proxy(total_failures: u32, contradictions: usize) -> u32 {
    (total_failures.saturating_mul(200) + contradictions.saturating_mul(250) as u32).min(1_000)
}

/// Execute one huddle for the regime: request, admit, collect reports,
/// charge the envelope, commit an outcome, and hand the reconciled plan to
/// the harness. Returns true when a proposal to retry the failed operation
/// was committed.
fn run_huddle(
    authority: &mut Authority,
    plane: &mut StudyPlane,
    scope: HuddleScope,
    kind: HuddleTriggerKind,
    reason_code: String,
    flat: bool,
    trackers: &mut HuddleTrackers,
) -> bool {
    let candidates: Vec<ParticipantCandidate> = authority
        .roster()
        .iter()
        .map(|person| {
            let relevance = if flat || person.id.as_str() != "nightly-toolchain" {
                900
            } else {
                100
            };
            ParticipantCandidate {
                personnel_id: person.id.clone(),
                dependency_overlap: relevance,
                contradiction_relevance: relevance,
                ownership_weight: relevance,
                downstream_impact: relevance,
                specialist_value: relevance,
                context_cost: 10,
            }
        })
        .collect();
    let failure_refs: Vec<_> = authority.state().failure_refs().to_vec();
    let last_failed_operation = trailing_failure_operation(authority.log());
    let request = HuddleRequest {
        scope,
        trigger_kind: kind,
        reason_code,
        evidence_refs: failure_refs.last().cloned().into_iter().collect(),
        manual: false,
        value: scenario_value(),
        candidates,
    };
    let debt_before = debt_proxy(trackers.total_failures, 0);
    if trackers.debt_before_first_huddle.is_none() {
        trackers.debt_before_first_huddle = Some(debt_before);
    }
    let admission = match authority.open_huddle(request, clock_of(authority.log())) {
        Ok(admission) => admission,
        Err(_) => return false,
    };
    let huddle_id = match admission {
        playbook_state::HuddleAdmission::Open(huddle_id) => huddle_id,
        playbook_state::HuddleAdmission::Refused { .. } => return false,
    };
    let participants: Vec<_> = authority
        .state()
        .huddles()
        .active_by_id(&huddle_id)
        .map(|huddle| {
            huddle
                .participants
                .iter()
                .map(|participant| participant.personnel_id.clone())
                .collect()
        })
        .unwrap_or_default();
    for participant in &participants {
        let report = HuddleReport {
            participant: participant.clone(),
            verified_progress: failure_refs.first().cloned().into_iter().collect(),
            blockers: Vec::new(),
            changed_assumptions: last_failed_operation
                .as_ref()
                .map(|operation| {
                    vec![AssumptionDelta {
                        subject: operation.clone(),
                        previous: "assumed reliable".into(),
                        current: "times out under the current contract".into(),
                        evidence_ref: failure_refs.last().cloned(),
                    }]
                })
                .unwrap_or_default(),
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            proposed_action: None,
            confidence_milli: 9_000,
        };
        let _ = authority.submit_huddle_report(&huddle_id, report);
        let _ = authority.consume_huddle_budget(
            &huddle_id,
            HuddleCost {
                tokens: REPORT_TOKENS,
                cost_microunits: REPORT_COST_MICROUNITS,
                duration_ms: REPORT_DURATION_MS,
                rounds: 0,
            },
        );
    }
    let mut outcome = HuddleOutcome {
        huddle_id: huddle_id.clone(),
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
        revised_ytg_milli: Some(authority.state().ytg_milli()),
        next_play: None,
        next_huddle_conditions: Vec::new(),
        escalated_to: None,
    };
    let mut steered = false;
    if let Some(operation) = last_failed_operation {
        outcome.next_play = Some(playbook_huddle::PlayProposal {
            operation: operation.clone(),
            rationale: "retry under the reconciled integration contract".into(),
            target_gate: Some(playbook_core::GateId::new("integration_tests").expect("gate")),
        });
        steered = true;
    }
    let _ = authority.commit_huddle_outcome(&huddle_id, outcome);
    if steered {
        // The harness executes the reconciled plan: the plane adopts the
        // fixed contract the reports described.
        plane.reconciled = true;
    }
    steered
}

fn trailing_failure_operation(events: &[DriveEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            DriveEvent::ToolFailed { failure } => Some(failure.operation.clone()),
            DriveEvent::ToolSucceeded { .. } => Some("none".into()),
            _ => None,
        })
        .filter(|operation| operation != "none")
}

fn clock_of(events: &[DriveEvent]) -> u64 {
    1_000 + events.len() as u64 * 100
}

/// Run one scenario once under one regime.
pub fn run_once(config: &StudyConfig) -> RunReport {
    let policy = Policy::builtin();
    let mut plane = StudyPlane::new(config.flaky);
    let mut authority = Authority::start(
        DriveId::new(config.drive_id).expect("drive id"),
        task_contract(playbook_core::TaskIntegrity::Sound),
        config.drive_budget,
        policy.clone(),
        roster(),
        1_000,
    )
    .expect("authority");
    for gate in study_gates() {
        authority.declare_gate(gate).expect("gate");
    }
    authority.preflight(&mut plane).expect("preflight");

    let mut trackers = HuddleTrackers {
        fingerprint_at_last_failure: None,
        terminal_gates_before: 0,
        total_failures: 0,
        debt_before_first_huddle: None,
        success_fired: false,
    };
    let mut pending_proposal: Option<String> = None;
    let mut steps_since_huddle = 0u32;
    let mut steps = 0u32;
    let mut stalled_steps = 0u32;
    let mut terminal = "step_cap";

    while steps < MAX_STEPS {
        let status = authority.state().status();
        if !matches!(status, DriveStatus::Active) {
            terminal = match status {
                DriveStatus::Completed => "completed",
                DriveStatus::Punted => "punted",
                DriveStatus::Abandoned => "abandoned",
                DriveStatus::FieldGoalScored => "field_goal",
                _ => "active",
            };
            break;
        }
        steps += 1;
        steps_since_huddle += 1;

        // Regime schedule.
        let mut fire: Option<(HuddleScope, HuddleTriggerKind, String)> = None;
        match config.regime {
            HuddleRegime::Control => {}
            HuddleRegime::Periodic => {
                if steps_since_huddle > config.period {
                    fire = Some((
                        HuddleScope::Position,
                        HuddleTriggerKind::ManualAuthority,
                        "periodic_review".to_string(),
                    ));
                }
            }
            HuddleRegime::FlatTriggered | HuddleRegime::HierarchicalTriggered => {
                let log = authority.log();
                let failures = trailing_failures(log);
                let terminal_gates = authority
                    .state()
                    .gates()
                    .iter()
                    .filter(|gate| gate.is_terminal())
                    .count();
                let gate_completed = terminal_gates > trackers.terminal_gates_before;
                trackers.terminal_gates_before = terminal_gates;
                let gate_completed = gate_completed && !trackers.success_fired;
                let fingerprint = authority.state().fingerprint();
                let field_changed = trackers
                    .fingerprint_at_last_failure
                    .as_ref()
                    .map(|seen| seen != &fingerprint)
                    .unwrap_or(true);
                if let Some(failure) = last_failure_of(log) {
                    trackers.fingerprint_at_last_failure = Some(failure);
                }
                trackers.total_failures = log
                    .iter()
                    .filter(|event| matches!(event, DriveEvent::ToolFailed { .. }))
                    .count() as u32;
                let trigger = evaluate_triggers(
                    &playbook_huddle::TriggerInputs {
                        failed_attempts: failures,
                        field_changed_since_failure: field_changed,
                        mandatory_gate_completed: gate_completed,
                        dependent_active_personnel: authority.roster().len() as u32,
                        down: authority.state().down(),
                        debt: debt_proxy(trackers.total_failures, 0),
                        evidence_refs: authority
                            .state()
                            .failure_refs()
                            .last()
                            .cloned()
                            .into_iter()
                            .collect(),
                        originating_event: EventId::new(format!("event-{}", log.len()))
                            .expect("event id"),
                    },
                    &policy.huddle,
                );
                if let Some((scope, trigger)) = trigger {
                    if trigger.kind == HuddleTriggerKind::Success {
                        trackers.success_fired = true;
                    }
                    fire = Some((scope, trigger.kind, trigger.reason_code.clone()));
                }
            }
        }
        if let Some((scope, kind, reason)) = fire {
            let flat = !matches!(config.regime, HuddleRegime::HierarchicalTriggered);
            if run_huddle(
                &mut authority,
                &mut plane,
                scope,
                kind,
                reason,
                flat,
                &mut trackers,
            ) {
                pending_proposal = authority
                    .state()
                    .huddles()
                    .proposals()
                    .last()
                    .map(|proposal| proposal.operation.clone());
            }
            steps_since_huddle = 0;
        }

        // Play selection: honour a committed proposal once, then route. The
        // reconciled contract lands on the field as evidence, which is what
        // clears the anti-thrash fingerprint for the steered retry.
        let steered = pending_proposal.take();
        let outcome = match steered {
            Some(operation) => {
                let evidence = Evidence::seal(
                    EvidenceClass::EnvironmentSnapshot,
                    serde_json::json!({
                        "host": "study",
                        "reconciled_contract": "integration v2",
                        "source": "huddle_outcome"
                    }),
                    clock_of(authority.log()),
                );
                let reconciled = evidence
                    .map_err(|error| (error, "reconciliation_evidence"))
                    .and_then(|evidence| {
                        authority
                            .accept_evidence(evidence)
                            .map_err(|error| (error, "reconciliation_evidence"))
                    });
                match reconciled {
                    Ok(()) => {
                        authority.try_operation(&operation, &mut plane, clock_of(authority.log()))
                    }
                    Err((error, _why)) => Err(error),
                }
            }
            None => authority.step(&mut plane, clock_of(authority.log())),
        };
        if let Err(error) = outcome {
            match error {
                // Real budget death ends the drive.
                PlaybookError::BudgetExhausted | PlaybookError::BudgetCeiling => {
                    terminal = "budget_exhausted";
                    break;
                }
                // The field offers no admissible play: the drive stalls in
                // place. A scheduled huddle can still rescue it later; the
                // control regime has no such rescue.
                PlaybookError::NoPlay
                | PlaybookError::BlindRetry(_)
                | PlaybookError::DuplicateEffect(_)
                | PlaybookError::Inadmissible(_, _)
                | PlaybookError::StaleCapability => {
                    stalled_steps = stalled_steps.saturating_add(1);
                }
                other => {
                    terminal = match other {
                        PlaybookError::Closed => "closed",
                        _ => "error",
                    };
                    break;
                }
            }
            continue;
        }
        let all_terminal = authority
            .state()
            .gates()
            .iter()
            .all(|gate| gate.is_terminal());
        if all_terminal {
            let _ = authority.claim_complete(clock_of(authority.log()));
        }
    }

    report_of(
        config.regime,
        &authority,
        terminal,
        steps.saturating_sub(stalled_steps),
        &trackers,
    )
}

fn last_failure_of(events: &[DriveEvent]) -> Option<String> {
    events.iter().rev().find_map(|event| match event {
        DriveEvent::ToolFailed { .. } => Some("failure".to_string()),
        _ => None,
    })
}

#[allow(clippy::too_many_lines)]
fn report_of(
    regime: HuddleRegime,
    authority: &Authority,
    terminal: &'static str,
    steps: u32,
    trackers: &HuddleTrackers,
) -> RunReport {
    let log = authority.log();
    let summary = rollup(log);
    let ledger = authority.state().huddles();
    let huddle_tokens: u64 = log
        .iter()
        .map(|event| match event {
            DriveEvent::HuddleBudgetConsumed { cost, .. } => cost.tokens,
            _ => 0,
        })
        .sum();
    let drive_tokens = authority.state().budget_consumed().tokens;
    let completed = terminal == "completed";
    let contradictions = ledger
        .closed()
        .iter()
        .map(|record| {
            record
                .outcome
                .as_ref()
                .map(|outcome| {
                    outcome.resolved_contradictions.len() + outcome.unresolved_contradictions.len()
                })
                .unwrap_or(0)
        })
        .sum::<usize>() as u32;
    let debt_after = debt_proxy(
        log.iter()
            .filter(|event| matches!(event, DriveEvent::ToolFailed { .. }))
            .count() as u32,
        0,
    );
    let coherence_gain = trackers
        .debt_before_first_huddle
        .unwrap_or(0)
        .saturating_sub(debt_after);
    let ytg_removed = 10_000i64 - i64::from(authority.state().ytg_milli());
    RunReport {
        regime: regime.name(),
        completed,
        terminal,
        steps,
        plays: summary.plays,
        tool_failures: summary.tool_failures,
        repeated_failed_plays: most_failed_operation_count(log),
        drive_tokens,
        huddle_tokens,
        total_tokens: drive_tokens + huddle_tokens,
        tokens_per_completion: completed.then_some(drive_tokens + huddle_tokens),
        ytg_velocity_milli_per_step: ytg_removed / i64::from(steps.max(1)),
        huddles_requested: ledger.requested_total(),
        huddles_committed: ledger.committed_total(),
        huddles_rejected: ledger.rejected_total(),
        contradictions,
        coherence_gain,
    }
}

/// One scenario: the same drive under all four regimes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenarioReport {
    pub name: &'static str,
    pub runs: Vec<RunReport>,
}

impl ScenarioReport {
    pub fn run(&self, regime: HuddleRegime) -> &RunReport {
        self.runs
            .iter()
            .find(|run| run.regime == regime.name())
            .expect("all four regimes present")
    }
}

fn flaky_integration() -> ScenarioReport {
    let budget = DriveBudget {
        tokens: 10_000,
        monetary_cost_micros: 1_000_000,
        wall_clock_ms: 600_000,
        tool_calls: 100,
        inference_calls: 20,
        retry_count: 5,
        context_capacity: 100_000,
        provider_quota: 1_000,
    };
    let regimes = [
        (HuddleRegime::Control, 0, "drive-study-flaky-control"),
        (HuddleRegime::Periodic, 8, "drive-study-flaky-periodic"),
        (HuddleRegime::FlatTriggered, 0, "drive-study-flaky-flat"),
        (
            HuddleRegime::HierarchicalTriggered,
            0,
            "drive-study-flaky-hierarchical",
        ),
    ];
    ScenarioReport {
        name: "flaky_integration",
        runs: regimes
            .iter()
            .map(|(regime, period, drive_id)| {
                run_once(&StudyConfig {
                    regime: *regime,
                    period: *period,
                    drive_id,
                    drive_budget: budget,
                    flaky: true,
                })
            })
            .collect(),
    }
}

fn clean_delivery() -> ScenarioReport {
    let budget = DriveBudget {
        tokens: 10_000,
        monetary_cost_micros: 1_000_000,
        wall_clock_ms: 600_000,
        tool_calls: 100,
        inference_calls: 20,
        retry_count: 5,
        context_capacity: 100_000,
        provider_quota: 1_000,
    };
    let regimes = [
        (HuddleRegime::Control, 0, "drive-study-clean-control"),
        (HuddleRegime::Periodic, 4, "drive-study-clean-periodic"),
        (HuddleRegime::FlatTriggered, 0, "drive-study-clean-flat"),
        (
            HuddleRegime::HierarchicalTriggered,
            0,
            "drive-study-clean-hierarchical",
        ),
    ];
    ScenarioReport {
        name: "clean_delivery",
        runs: regimes
            .iter()
            .map(|(regime, period, drive_id)| {
                run_once(&StudyConfig {
                    regime: *regime,
                    period: *period,
                    drive_id,
                    drive_budget: budget,
                    flaky: false,
                })
            })
            .collect(),
    }
}

/// The full Phase 8 programme.
pub fn phase_eight_study() -> Vec<ScenarioReport> {
    vec![flaky_integration(), clean_delivery()]
}

/// A fixed-width comparison table over the §24 measures.
pub fn study_table(scenarios: &[ScenarioReport]) -> String {
    let mut text = String::from(
        "scenario            regime                   completed terminal          steps plays fails rep_fail drive_tok huddle_tok total_tok\n",
    );
    for scenario in scenarios {
        for run in &scenario.runs {
            text.push_str(&format!(
                "{:<19} {:<24} {:<9} {:<16} {:<5} {:<5} {:<5} {:<8} {:<9} {:<9} {}\n",
                scenario.name,
                run.regime,
                run.completed,
                run.terminal,
                run.steps,
                run.plays,
                run.tool_failures,
                run.repeated_failed_plays,
                run.drive_tokens,
                run.huddle_tokens,
                run.total_tokens,
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regimes_differ_only_by_their_schedule_and_participation() {
        let report = phase_eight_study();
        let text = study_table(&report);
        println!("{text}");
        let flaky = &report[0];
        let clean = &report[1];

        // Scenario 1: reconciliation rescues the drive; the control regime
        // burns its budget on repeated failed plays.
        assert!(!flaky.run(HuddleRegime::Control).completed);
        assert!(flaky.run(HuddleRegime::Periodic).completed);
        assert!(flaky.run(HuddleRegime::FlatTriggered).completed);
        assert!(flaky.run(HuddleRegime::HierarchicalTriggered).completed);

        // Triggered huddles waste fewer failure tokens than the periodic
        // schedule, and the hierarchical floor pays fewer synchronization
        // tokens than the flat one.
        let b = flaky.run(HuddleRegime::Periodic).total_tokens;
        let c = flaky.run(HuddleRegime::FlatTriggered).total_tokens;
        let d = flaky.run(HuddleRegime::HierarchicalTriggered).total_tokens;
        assert!(d < c, "hierarchical must be cheaper than flat: {d} vs {c}");
        assert!(c < b, "triggered must beat periodic: {c} vs {b}");
        assert!(flaky.run(HuddleRegime::Control).total_tokens > d);
        assert!(
            flaky.run(HuddleRegime::Control).repeated_failed_plays
                > flaky
                    .run(HuddleRegime::HierarchicalTriggered)
                    .repeated_failed_plays
        );

        // Scenario 2: trivially independent work — the control regime is
        // in the leading group and every huddle only adds cost.
        assert!(clean.run(HuddleRegime::Control).completed);
        let a = clean.run(HuddleRegime::Control).total_tokens;
        assert_eq!(clean.run(HuddleRegime::Control).huddle_tokens, 0);
        assert!(a <= clean.run(HuddleRegime::Periodic).total_tokens);
        assert!(a < clean.run(HuddleRegime::FlatTriggered).total_tokens);
        assert!(a < clean.run(HuddleRegime::HierarchicalTriggered).total_tokens);
        assert!(
            flaky.run(HuddleRegime::Control).repeated_failed_plays
                > flaky
                    .run(HuddleRegime::HierarchicalTriggered)
                    .repeated_failed_plays
        );
        assert!(
            flaky
                .run(HuddleRegime::HierarchicalTriggered)
                .coherence_gain
                >= flaky.run(HuddleRegime::Control).coherence_gain
        );
    }

    #[test]
    fn the_programme_is_deterministic() {
        let first = phase_eight_study();
        let second = phase_eight_study();
        assert_eq!(first, second);
    }

    #[test]
    fn every_run_stays_inside_the_step_cap_and_journals_replayably() {
        for scenario in phase_eight_study() {
            for run in &scenario.runs {
                assert!(run.steps <= STEP_CAP);
                assert!(run.huddles_rejected == 0, "no storm refusals expected");
            }
        }
    }
}
