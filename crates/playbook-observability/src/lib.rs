//! Live tactical telemetry.
//!
//! Explanations cite measurements. They do not quote model chain-of-thought.
//! Spans follow an OpenTelemetry shape: one drive is a trace, plays are spans,
//! tool calls are child spans. Export over OTLP is a deployment choice; the
//! model here is the contract.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use playbook_core::{ppm_percent, HuddleId, TacticalSnapshot, PPM_SCALE};
use playbook_events::DriveEvent;
use playbook_huddle::{HuddleLedger, HuddleScope};

pub const METRIC_DRIVE_COMPLETION_RATE: &str = "drive_completion_rate";
pub const METRIC_MEAN_BUDGET_UTILISATION: &str = "mean_budget_utilisation";
pub const METRIC_YTG_REDUCTION_PER_COST: &str = "ytg_reduction_per_cost";
pub const METRIC_PLAYS_PER_DRIVE: &str = "plays_per_drive";
pub const METRIC_PUNT_RATE: &str = "punt_rate";
pub const METRIC_FIELD_GOAL_RATE: &str = "field_goal_rate";
pub const METRIC_FOURTH_DOWN_CONVERSION_RATE: &str = "fourth_down_conversion_rate";
pub const METRIC_TOOL_FAILURE_RATE: &str = "tool_failure_rate";
pub const METRIC_CAPABILITY_ACQUISITION_RATE: &str = "capability_acquisition_rate";
pub const METRIC_MODEL_HANDOFF_RATE: &str = "model_handoff_rate";
pub const METRIC_GATE_REOPEN_RATE: &str = "gate_reopen_rate";
pub const METRIC_FALSE_COMPLETION_RATE: &str = "false_completion_rate";
pub const METRIC_HUDDLES_TOTAL: &str = "huddles_total";
pub const METRIC_HUDDLES_BY_SCOPE: &str = "huddles_by_scope";
pub const METRIC_HUDDLES_REJECTED_TOTAL: &str = "huddles_rejected_total";
pub const METRIC_HUDDLE_TOKENS_TOTAL: &str = "huddle_tokens_total";
pub const METRIC_HUDDLE_COST_TOTAL: &str = "huddle_cost_total";
pub const METRIC_HUDDLE_DURATION_MS: &str = "huddle_duration_ms";
pub const METRIC_HUDDLE_PARTICIPANTS: &str = "huddle_participants";
pub const METRIC_HUDDLE_ESCALATIONS: &str = "huddle_escalations";
pub const METRIC_HUDDLE_CONTRADICTIONS_FOUND: &str = "huddle_contradictions_found";
pub const METRIC_HUDDLE_CONTRADICTIONS_RESOLVED: &str = "huddle_contradictions_resolved";
pub const METRIC_COHERENCE_DEBT_BEFORE: &str = "coherence_debt_before";
pub const METRIC_COHERENCE_DEBT_AFTER: &str = "coherence_debt_after";
pub const METRIC_YTG_VELOCITY_PRE_HUDDLE: &str = "ytg_velocity_pre_huddle";
pub const METRIC_YTG_VELOCITY_POST_HUDDLE: &str = "ytg_velocity_post_huddle";
pub const METRIC_REWORK_AVOIDED_ESTIMATE: &str = "rework_avoided_estimate";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rollup {
    pub plays: u32,
    pub tool_failures: u32,
    pub punts: u32,
    pub field_goals: u32,
    pub fourth_downs: u32,
    pub completions: u32,
    pub false_completion_claims: u32,
    pub handoffs: u32,
    pub capability_requirements: u32,
    pub converts: u32,
    pub huddles_requested: u32,
    pub huddles_admitted: u32,
    pub huddles_rejected: u32,
    pub huddle_reports: u32,
    pub huddle_contradictions: u32,
    pub huddle_outcomes: u32,
    pub huddle_escalations: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpanNode {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
}

pub fn stage_line(snapshot: &TacticalSnapshot) -> String {
    let previous = i64::from(snapshot.ytg_milli) - i64::from(snapshot.ytg_delta_milli);
    let play = snapshot.current_play.as_deref().unwrap_or("none");
    format!(
        "{}\nYTG {previous} → {}\nBudget remaining: {}\nPlay: {play}",
        snapshot.presentation,
        snapshot.ytg_milli,
        ppm_percent(snapshot.budget_remaining_ppm),
    )
}

pub fn audit_text(snapshot: &TacticalSnapshot) -> String {
    let mut lines = vec![format!(
        "Down is {} because:",
        snapshot.current_down.machine_id()
    )];
    lines.extend(snapshot.explanation.iter().cloned());
    lines.join("\n")
}

pub fn rollup(events: &[DriveEvent]) -> Rollup {
    let mut rollup = Rollup {
        plays: 0,
        tool_failures: 0,
        punts: 0,
        field_goals: 0,
        fourth_downs: 0,
        completions: 0,
        false_completion_claims: 0,
        handoffs: 0,
        capability_requirements: 0,
        converts: 0,
        huddles_requested: 0,
        huddles_admitted: 0,
        huddles_rejected: 0,
        huddle_reports: 0,
        huddle_contradictions: 0,
        huddle_outcomes: 0,
        huddle_escalations: 0,
    };
    for event in events {
        match event {
            DriveEvent::PlaySelected { .. } => rollup.plays += 1,
            DriveEvent::ToolFailed { .. } | DriveEvent::ToolOutcomeRejected { .. } => {
                rollup.tool_failures += 1;
            }
            DriveEvent::PuntIssued { .. } => rollup.punts += 1,
            DriveEvent::FieldGoalSelected { .. } => rollup.field_goals += 1,
            DriveEvent::FourthDownDeclared { disposition, .. } => {
                rollup.fourth_downs += 1;
                if *disposition == playbook_core::Disposition::Convert {
                    rollup.converts += 1;
                }
            }
            DriveEvent::DriveCompleted { .. } => rollup.completions += 1,
            DriveEvent::CompletionRejected { .. } => rollup.false_completion_claims += 1,
            DriveEvent::PossessionStarted { .. } => rollup.handoffs += 1,
            DriveEvent::CapabilityRequirementIssued { .. } => rollup.capability_requirements += 1,
            DriveEvent::HuddleRequested { .. } => rollup.huddles_requested += 1,
            DriveEvent::HuddleAdmitted { .. } => rollup.huddles_admitted += 1,
            DriveEvent::HuddleRejected { .. } => rollup.huddles_rejected += 1,
            DriveEvent::HuddleReportSubmitted { .. } => rollup.huddle_reports += 1,
            DriveEvent::HuddleContradictionDetected { .. } => rollup.huddle_contradictions += 1,
            DriveEvent::HuddleOutcomeCommitted { .. } => rollup.huddle_outcomes += 1,
            DriveEvent::HuddleEscalated { .. } => rollup.huddle_escalations += 1,
            _ => {}
        }
    }
    rollup
}

pub fn false_completion_ppm(rollup: &Rollup) -> u32 {
    let claims = rollup.false_completion_claims + rollup.completions;
    rollup
        .false_completion_claims
        .saturating_mul(PPM_SCALE)
        .checked_div(claims)
        .unwrap_or(0)
}

fn huddle_scope_slug(scope: &HuddleScope) -> &'static str {
    match scope {
        HuddleScope::Position => "position",
        HuddleScope::Unit => "unit",
        HuddleScope::Coordinator => "coordinator",
        HuddleScope::Team => "team",
        HuddleScope::Franchise => "franchise",
    }
}

/// §23 huddle metrics over one drive's event log and huddle ledger.
///
/// Ledger totals are authoritative for requests, rejections, escalations and
/// contradiction bookkeeping: the fold only advances them for events that
/// survived admission, so refused or replayed events cannot inflate them.
/// Cost sums come from `HuddleBudgetConsumed` events and participants from
/// admission and invitation events, because closed ledger records drop both
/// the `spent` accumulator and the roster. `coherence_debt_before` is the
/// latest closed record's request-time debt reading and `coherence_debt_after`
/// its outcome's post-reconciliation reading when one was measured; both keys
/// are omitted with no closed records. `rework_avoided_estimate` sums the
/// request-time estimate over closed records: closed records carry the
/// requested estimate through closure, so the sum covers every closed huddle
/// rather than only outcome-measured rework. Pre/post-huddle ytg velocity is
/// still recorded on no ledger record or event, so its keys are omitted
/// rather than proxied.
pub fn huddle_metrics(events: &[DriveEvent], ledger: &HuddleLedger) -> BTreeMap<String, u64> {
    let mut metrics = BTreeMap::new();
    metrics.insert(
        METRIC_HUDDLES_TOTAL.into(),
        u64::from(ledger.requested_total()),
    );
    metrics.insert(
        METRIC_HUDDLES_REJECTED_TOTAL.into(),
        u64::from(ledger.rejected_total()),
    );
    metrics.insert(
        METRIC_HUDDLE_ESCALATIONS.into(),
        u64::from(ledger.escalated_total()),
    );

    let mut by_scope: BTreeMap<HuddleScope, u64> = BTreeMap::new();
    for huddle in ledger.active() {
        *by_scope.entry(huddle.scope).or_insert(0) += 1;
    }
    for record in ledger.closed() {
        *by_scope.entry(record.scope).or_insert(0) += 1;
    }
    for (scope, count) in by_scope {
        let slug = huddle_scope_slug(&scope);
        metrics.insert(format!("{slug}_huddles_total"), count);
    }

    let mut tokens = 0u64;
    let mut cost = 0u64;
    let mut duration = 0u64;
    let mut participants: Vec<&str> = Vec::new();
    for event in events {
        match event {
            DriveEvent::HuddleBudgetConsumed { cost: spent, .. } => {
                tokens = tokens.saturating_add(spent.tokens);
                cost = cost.saturating_add(spent.cost_microunits);
                duration = duration.saturating_add(spent.duration_ms);
            }
            DriveEvent::HuddleAdmitted {
                participants: roster,
                ..
            } => {
                for participant in roster {
                    let id = participant.personnel_id.as_str();
                    if !participants.contains(&id) {
                        participants.push(id);
                    }
                }
            }
            DriveEvent::HuddleParticipantInvited { participant, .. } => {
                let id = participant.personnel_id.as_str();
                if !participants.contains(&id) {
                    participants.push(id);
                }
            }
            _ => {}
        }
    }
    metrics.insert(METRIC_HUDDLE_TOKENS_TOTAL.into(), tokens);
    metrics.insert(METRIC_HUDDLE_COST_TOTAL.into(), cost);
    metrics.insert(METRIC_HUDDLE_DURATION_MS.into(), duration);
    metrics.insert(
        METRIC_HUDDLE_PARTICIPANTS.into(),
        u64::try_from(participants.len()).unwrap_or(u64::MAX),
    );

    let mut found = 0u64;
    let mut resolved = 0u64;
    for huddle in ledger.active() {
        let counted = u64::try_from(huddle.contradictions.len()).unwrap_or(u64::MAX);
        found = found.saturating_add(counted);
    }
    for record in ledger.closed() {
        let Some(outcome) = &record.outcome else {
            continue;
        };
        let resolved_count =
            u64::try_from(outcome.resolved_contradictions.len()).unwrap_or(u64::MAX);
        let unresolved_count =
            u64::try_from(outcome.unresolved_contradictions.len()).unwrap_or(u64::MAX);
        resolved = resolved.saturating_add(resolved_count);
        found = found.saturating_add(resolved_count.saturating_add(unresolved_count));
    }
    metrics.insert(METRIC_HUDDLE_CONTRADICTIONS_FOUND.into(), found);
    metrics.insert(METRIC_HUDDLE_CONTRADICTIONS_RESOLVED.into(), resolved);

    let closed = ledger.closed();
    if let Some(record) = closed.last() {
        metrics.insert(
            METRIC_COHERENCE_DEBT_BEFORE.into(),
            u64::from(record.debt_before),
        );
        if let Some(debt_after) = record
            .outcome
            .as_ref()
            .and_then(|outcome| outcome.debt_after)
        {
            metrics.insert(METRIC_COHERENCE_DEBT_AFTER.into(), u64::from(debt_after));
        }
    }
    if !closed.is_empty() {
        let mut rework = 0u64;
        for record in closed {
            rework = rework.saturating_add(u64::from(record.rework_avoided));
        }
        metrics.insert(METRIC_REWORK_AVOIDED_ESTIMATE.into(), rework);
    }
    metrics
}

pub fn spans(drive_id: &str, events: &[DriveEvent]) -> Vec<SpanNode> {
    let trace_id = drive_id.to_string();
    let mut out = vec![SpanNode {
        trace_id: trace_id.clone(),
        span_id: format!("{drive_id}:drive"),
        parent_span_id: None,
        name: "drive".into(),
        attributes: BTreeMap::from([("drive_id".into(), drive_id.to_string())]),
    }];
    let mut play_span = None;
    let mut tool_span = None;
    let mut huddle_span = None;
    let mut blitz_span = None;
    // HuddleAdmitted carries no scope; it is resolved from the request.
    let mut huddle_scopes: BTreeMap<HuddleId, HuddleScope> = BTreeMap::new();
    for (index, event) in events.iter().enumerate() {
        match event {
            DriveEvent::PlaySelected { decision } => {
                let span_id = format!("{drive_id}:play:{index}");
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: span_id.clone(),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: decision.selected_operation.clone(),
                    attributes: BTreeMap::from([(
                        "down".into(),
                        decision.classification.machine_id().to_string(),
                    )]),
                });
                play_span = Some(span_id);
            }
            DriveEvent::ToolInvoked { execution } => {
                tool_span = Some(out.len());
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:tool:{index}"),
                    parent_span_id: play_span.clone(),
                    name: execution.operation.clone(),
                    attributes: BTreeMap::from([(
                        "capability".into(),
                        execution.capability_id.to_string(),
                    )]),
                });
            }
            DriveEvent::ToolOutcomeRejected { reason, .. } => {
                if let Some(index) = tool_span {
                    out[index]
                        .attributes
                        .insert("outcome".into(), "rejected".into());
                    out[index]
                        .attributes
                        .insert("reconciliation_required".into(), "true".into());
                    out[index].attributes.insert("error".into(), reason.clone());
                }
            }
            DriveEvent::HuddleRequested {
                huddle_id, scope, ..
            } => {
                huddle_scopes.insert(huddle_id.clone(), *scope);
            }
            DriveEvent::HuddleAdmitted { huddle_id, .. } => {
                if let Some(scope) = huddle_scopes.get(huddle_id).copied() {
                    let span_id = format!("{drive_id}:huddle:{index}");
                    let slug = huddle_scope_slug(&scope);
                    out.push(SpanNode {
                        trace_id: trace_id.clone(),
                        span_id: span_id.clone(),
                        parent_span_id: Some(format!("{drive_id}:drive")),
                        name: format!("huddle:{slug}"),
                        attributes: BTreeMap::from([
                            ("huddle_id".into(), huddle_id.to_string()),
                            ("scope".into(), slug.to_string()),
                        ]),
                    });
                    huddle_span = Some(span_id);
                }
            }
            DriveEvent::HuddleReportSubmitted { report, .. } => {
                let parent = huddle_span
                    .clone()
                    .unwrap_or_else(|| format!("{drive_id}:drive"));
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:huddle_report:{index}"),
                    parent_span_id: Some(parent),
                    name: "huddle_report".into(),
                    attributes: BTreeMap::from([(
                        "participant".into(),
                        report.participant.to_string(),
                    )]),
                });
            }
            DriveEvent::BlitzProposed { blitz_id, .. } => {
                let span_id = format!("{drive_id}:blitz:{index}");
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: span_id.clone(),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: "blitz".into(),
                    attributes: BTreeMap::from([("blitz_id".into(), blitz_id.to_string())]),
                });
                blitz_span = Some(span_id);
            }
            DriveEvent::BlitzWorkerDispatched {
                worker,
                effective_parallelism_milli,
                ..
            } => {
                let parent = blitz_span
                    .clone()
                    .unwrap_or_else(|| format!("{drive_id}:drive"));
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:blitz_worker:{index}"),
                    parent_span_id: Some(parent),
                    name: "blitz_worker".into(),
                    attributes: BTreeMap::from([
                        ("worker_id".into(), worker.worker_id.to_string()),
                        ("provider".into(), worker.provider.clone()),
                        (
                            "effective_parallelism_milli".into(),
                            effective_parallelism_milli.to_string(),
                        ),
                    ]),
                });
            }
            DriveEvent::BlitzCandidateVerified {
                candidate_id,
                passed,
                ..
            } => {
                let parent = blitz_span
                    .clone()
                    .unwrap_or_else(|| format!("{drive_id}:drive"));
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:candidate_verified:{index}"),
                    parent_span_id: Some(parent),
                    name: "candidate_verified".into(),
                    attributes: BTreeMap::from([
                        ("candidate_id".into(), candidate_id.to_string()),
                        ("gates_passed".into(), passed.len().to_string()),
                    ]),
                });
            }
            DriveEvent::BlitzWinnerSelected {
                candidate_id,
                worker_id,
                ..
            } => {
                let parent = blitz_span
                    .clone()
                    .unwrap_or_else(|| format!("{drive_id}:drive"));
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:blitz_winner:{index}"),
                    parent_span_id: Some(parent),
                    name: "blitz_winner".into(),
                    attributes: BTreeMap::from([
                        ("candidate_id".into(), candidate_id.to_string()),
                        ("worker_id".into(), worker_id.to_string()),
                        ("basis".into(), "first_verified".into()),
                    ]),
                });
            }
            DriveEvent::BlitzWorkerCancelled {
                worker_id,
                avoided_tokens,
                ..
            } => {
                let parent = blitz_span
                    .clone()
                    .unwrap_or_else(|| format!("{drive_id}:drive"));
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:worker_cancelled:{index}"),
                    parent_span_id: Some(parent),
                    name: "worker_cancelled".into(),
                    attributes: BTreeMap::from([
                        ("worker_id".into(), worker_id.to_string()),
                        ("avoided_tokens".into(), avoided_tokens.to_string()),
                    ]),
                });
            }
            DriveEvent::BlitzExhausted { failure, .. } => {
                if let Some(span_index) = blitz_span
                    .as_ref()
                    .and_then(|span_id| out.iter().position(|span| span.span_id == *span_id))
                {
                    out[span_index]
                        .attributes
                        .insert("outcome".into(), format!("{failure:?}"));
                }
            }
            DriveEvent::TacticAdmitted { tactic, .. } => {
                let span_id = format!("{drive_id}:tactic:{index}");
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: span_id.clone(),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: format!("tactic:{tactic}"),
                    attributes: BTreeMap::from([("outcome".into(), "admitted".into())]),
                });
            }
            DriveEvent::TacticCompleted {
                tactic,
                success,
                ytg_before_milli,
                ytg_after_milli,
                ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:tactic_outcome:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: format!("tactic:{tactic}"),
                    attributes: BTreeMap::from([
                        (
                            "outcome".into(),
                            if *success {
                                "completed".into()
                            } else {
                                "failed_objective".into()
                            },
                        ),
                        ("ytg_before_milli".into(), ytg_before_milli.to_string()),
                        ("ytg_after_milli".into(), ytg_after_milli.to_string()),
                    ]),
                });
            }
            DriveEvent::TacticFailed {
                tactic,
                failure_mode,
                ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:tactic_outcome:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: format!("tactic:{tactic}"),
                    attributes: BTreeMap::from([
                        ("outcome".into(), "failed".into()),
                        ("failure_mode".into(), failure_mode.clone()),
                    ]),
                });
            }
            DriveEvent::TacticCountered {
                tactic, counter, ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:tactic_counter:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: format!("tactic:{tactic}"),
                    attributes: BTreeMap::from([
                        ("outcome".into(), "countered".into()),
                        ("counter".into(), counter.clone()),
                    ]),
                });
            }
            DriveEvent::PlayerSubstituted {
                class,
                out: out_player,
                replacement,
                ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:substitution:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: "substitution".into(),
                    attributes: BTreeMap::from([
                        ("class".into(), class.clone()),
                        ("out".into(), out_player.to_string()),
                        (
                            "replacement".into(),
                            replacement
                                .as_ref()
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "none".into()),
                        ),
                    ]),
                });
            }
            DriveEvent::GameContractSealed { contract, .. } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:game_contract:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: "game_contract".into(),
                    attributes: BTreeMap::from([
                        ("contract_hash".into(), contract.contract_hash.clone()),
                        (
                            "mandatory_win_conditions".into(),
                            contract
                                .win_conditions
                                .iter()
                                .filter(|condition| condition.mandatory)
                                .count()
                                .to_string(),
                        ),
                    ]),
                });
            }
            DriveEvent::EvaluationDecided {
                branch, reasons, ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:evaluation:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: format!("evaluation:{}", branch_key(branch)),
                    attributes: BTreeMap::from([("reasons".into(), reasons.join("|"))]),
                });
            }
            DriveEvent::CersRecovered {
                artefact_id,
                source,
                ..
            } => {
                out.push(SpanNode {
                    trace_id: trace_id.clone(),
                    span_id: format!("{drive_id}:cers:{index}"),
                    parent_span_id: Some(format!("{drive_id}:drive")),
                    name: "cers_recovery".into(),
                    attributes: BTreeMap::from([
                        ("artefact_id".into(), artefact_id.to_string()),
                        ("source".into(), source.clone()),
                    ]),
                });
            }
            _ => {}
        }
    }
    out
}

fn branch_key(branch: &playbook_core::EvaluationBranch) -> &'static str {
    use playbook_core::EvaluationBranch;
    match branch {
        EvaluationBranch::Advance => "advance",
        EvaluationBranch::Huddle => "huddle",
        EvaluationBranch::Substitute => "substitute",
        EvaluationBranch::Lastdown => "lastdown",
        EvaluationBranch::Blitz => "blitz",
        EvaluationBranch::CersRecovery => "cers_recovery",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{Down, DriveStatus, PlayStatus};

    fn snapshot() -> TacticalSnapshot {
        TacticalSnapshot {
            drive_id: "drive-1".into(),
            current_down: Down::ThirdDownRush,
            presentation: Down::ThirdDownRush.presentation().into(),
            current_play: Some("integration validation".into()),
            current_personnel: Some("cargo".into()),
            ytg_milli: 18,
            ytg_delta_milli: -13,
            budget_remaining_ppm: 640_000,
            budget_consumed: playbook_core::DriveBudget::zero(),
            mandatory_gates: vec!["integration".into()],
            blocked_gates: vec![],
            active_capability: Some("elci.cargo".into()),
            last_result: Some(PlayStatus::Succeeded),
            completion_probability_ppm: 700_000,
            failure_class: None,
            status: DriveStatus::Active,
            explanation: vec!["remaining budget is 64%".into()],
        }
    }

    #[test]
    fn stage_line_matches_the_tactical_reading() {
        let text = stage_line(&snapshot());
        assert!(text.contains("3rd Down — Rush"));
        assert!(text.contains("YTG 31 → 18"));
        assert!(text.contains("Budget remaining: 64%"));
        assert!(text.contains("Play: integration validation"));
    }

    #[test]
    fn rejected_outcome_is_visible_in_failure_metrics_and_tool_span() {
        use playbook_core::{Down, PlayExecution, PlayKind, RetryPolicy, SideEffectClass};
        let execution = PlayExecution {
            drive_id: playbook_core::DriveId::new("drive-1").unwrap(),
            play_id: playbook_core::PlayId::new("play-1").unwrap(),
            play_kind: PlayKind::DiscoverCapabilities,
            operation: "discover".into(),
            down: Down::FirstDown,
            capability_id: playbook_core::CapabilityId::new("elci.discovery").unwrap(),
            capability_generation: 1,
            personnel_id: playbook_core::PersonnelId::new("worker").unwrap(),
            inputs: Default::default(),
            expected_evidence: Vec::new(),
            expected_ytg_reduction_milli: 0,
            survey: true,
            budget_ceiling: playbook_core::DriveBudget::zero(),
            timeout_ms: 100,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: SideEffectClass::ReadOnly,
            idempotency_key: Some("invoke-1".into()),
        };
        let events = vec![
            DriveEvent::ToolInvoked { execution },
            DriveEvent::ToolOutcomeRejected {
                result: None,
                failure: None,
                reason: "response lost".into(),
            },
        ];
        assert_eq!(rollup(&events).tool_failures, 1);
        let trace = spans("drive-1", &events);
        let attributes = &trace[1].attributes;
        assert_eq!(attributes["outcome"], "rejected");
        assert_eq!(attributes["reconciliation_required"], "true");
        assert_eq!(attributes["error"], "response lost");
    }

    #[test]
    fn rollup_counts_huddle_events() {
        use playbook_core::{
            BarrierMode, EventId, EvidenceId, HuddleBudget, HuddleId, PersonnelId,
        };
        use playbook_huddle::{
            Contradiction, ContradictionId, ContradictionSeverity, HuddleOutcome, HuddleReport,
            HuddleScope, HuddleTrigger, HuddleTriggerKind, SharedStateDelta,
        };
        let huddle_id = HuddleId::new("h-1").unwrap();
        let trigger = HuddleTrigger {
            kind: HuddleTriggerKind::CoherenceDebt,
            reason_code: "state_divergence".into(),
            evidence_refs: vec![EvidenceId::new("e-1").unwrap()],
            originating_event: EventId::new("evt-1").unwrap(),
        };
        let events = vec![
            DriveEvent::HuddleRequested {
                huddle_id: huddle_id.clone(),
                scope: HuddleScope::Position,
                trigger,
                at_ms: 10,
                debt: 0,
                rework_avoided: 0,
            },
            DriveEvent::HuddleAdmitted {
                huddle_id: huddle_id.clone(),
                budget: HuddleBudget::zero(),
                barrier: BarrierMode::Soft,
                participants: Vec::new(),
            },
            DriveEvent::HuddleRejected {
                huddle_id: HuddleId::new("h-2").unwrap(),
                reason_code: "budget_exhausted".into(),
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
            DriveEvent::HuddleContradictionDetected {
                huddle_id: huddle_id.clone(),
                contradiction: Contradiction {
                    id: ContradictionId::new("c-1").unwrap(),
                    subject: "gate readiness".into(),
                    claims: Vec::new(),
                    severity: ContradictionSeverity::High,
                },
            },
            DriveEvent::HuddleOutcomeCommitted {
                huddle_id: huddle_id.clone(),
                outcome: Box::new(HuddleOutcome {
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
                    revised_ytg_milli: None,
                    debt_after: None,
                    next_play: None,
                    next_huddle_conditions: Vec::new(),
                    escalated_to: None,
                }),
            },
            DriveEvent::HuddleEscalated {
                huddle_id: huddle_id.clone(),
                new_huddle_id: HuddleId::new("h-3").unwrap(),
                to_scope: HuddleScope::Unit,
            },
        ];
        let summary = rollup(&events);
        assert_eq!(summary.huddles_requested, 1);
        assert_eq!(summary.huddles_admitted, 1);
        assert_eq!(summary.huddles_rejected, 1);
        assert_eq!(summary.huddle_reports, 1);
        assert_eq!(summary.huddle_contradictions, 1);
        assert_eq!(summary.huddle_outcomes, 1);
        assert_eq!(summary.huddle_escalations, 1);
    }

    #[test]
    fn huddle_span_links_reports_to_the_open_huddle() {
        use playbook_core::{BarrierMode, EventId, HuddleBudget, HuddleId, PersonnelId};
        use playbook_huddle::{
            DisclosureProfile, HuddleParticipant, HuddleReport, HuddleRole, HuddleScope,
            HuddleTrigger, HuddleTriggerKind,
        };
        let huddle_id = HuddleId::new("h-1").unwrap();
        let report = |personnel: &str| HuddleReport {
            participant: PersonnelId::new(personnel).unwrap(),
            verified_progress: Vec::new(),
            blockers: Vec::new(),
            changed_assumptions: Vec::new(),
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            proposed_action: None,
            confidence_milli: 800,
        };
        let events = vec![
            DriveEvent::HuddleReportSubmitted {
                huddle_id: huddle_id.clone(),
                report: Box::new(report("worker")),
            },
            DriveEvent::HuddleRequested {
                huddle_id: huddle_id.clone(),
                scope: HuddleScope::Position,
                trigger: HuddleTrigger {
                    kind: HuddleTriggerKind::CoherenceDebt,
                    reason_code: "state_divergence".into(),
                    evidence_refs: Vec::new(),
                    originating_event: EventId::new("evt-2").unwrap(),
                },
                at_ms: 5,
                debt: 0,
                rework_avoided: 0,
            },
            DriveEvent::HuddleAdmitted {
                huddle_id: huddle_id.clone(),
                budget: HuddleBudget::zero(),
                barrier: BarrierMode::Hard,
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
                report: Box::new(report("validator")),
            },
        ];
        let trace = spans("drive-1", &events);
        assert_eq!(trace[1].name, "huddle_report");
        assert_eq!(trace[1].parent_span_id.as_deref(), Some("drive-1:drive"));
        assert_eq!(trace[1].attributes["participant"], "worker");
        assert_eq!(trace[2].span_id, "drive-1:huddle:2");
        assert_eq!(trace[2].name, "huddle:position");
        assert_eq!(trace[2].parent_span_id.as_deref(), Some("drive-1:drive"));
        assert_eq!(trace[2].attributes["huddle_id"], "h-1");
        assert_eq!(trace[2].attributes["scope"], "position");
        assert_eq!(trace[3].name, "huddle_report");
        assert_eq!(trace[3].parent_span_id.as_deref(), Some("drive-1:huddle:2"));
        assert_eq!(trace[3].attributes["participant"], "validator");
    }

    #[test]
    fn huddle_metrics_sums_the_event_log_and_is_deterministic() {
        use playbook_core::{BarrierMode, HuddleBudget, HuddleId, PersonnelId};
        use playbook_core::{EventId, EvidenceId};
        use playbook_huddle::{
            DisclosureProfile, HuddleCost, HuddleParticipant, HuddleRole, HuddleScope,
            HuddleTrigger, HuddleTriggerKind,
        };
        let roster = |personnel: &str| HuddleParticipant {
            personnel_id: PersonnelId::new(personnel).unwrap(),
            role: HuddleRole::Contributor,
            disclosure_profile: DisclosureProfile {
                include_gates: true,
                include_peer_reports: true,
                include_strategic_state: false,
                max_evidence_refs: 8,
            },
        };
        let events = vec![
            DriveEvent::HuddleRequested {
                huddle_id: HuddleId::new("h-1").unwrap(),
                scope: HuddleScope::Position,
                trigger: HuddleTrigger {
                    kind: HuddleTriggerKind::CoherenceDebt,
                    reason_code: "state_divergence".into(),
                    evidence_refs: vec![EvidenceId::new("e-1").unwrap()],
                    originating_event: EventId::new("evt-1").unwrap(),
                },
                at_ms: 10,
                debt: 0,
                rework_avoided: 0,
            },
            DriveEvent::HuddleAdmitted {
                huddle_id: HuddleId::new("h-1").unwrap(),
                budget: HuddleBudget::zero(),
                barrier: BarrierMode::Soft,
                participants: vec![roster("worker"), roster("validator")],
            },
            DriveEvent::HuddleBudgetConsumed {
                huddle_id: HuddleId::new("h-1").unwrap(),
                cost: HuddleCost {
                    tokens: 120,
                    cost_microunits: 4_000,
                    duration_ms: 250,
                    rounds: 1,
                },
            },
            DriveEvent::HuddleBudgetConsumed {
                huddle_id: HuddleId::new("h-1").unwrap(),
                cost: HuddleCost {
                    tokens: 80,
                    cost_microunits: 2_500,
                    duration_ms: 750,
                    rounds: 1,
                },
            },
            DriveEvent::HuddleParticipantInvited {
                huddle_id: HuddleId::new("h-1").unwrap(),
                participant: roster("watcher"),
            },
            DriveEvent::HuddleParticipantInvited {
                huddle_id: HuddleId::new("h-1").unwrap(),
                participant: roster("worker"),
            },
            DriveEvent::HuddleRejected {
                huddle_id: HuddleId::new("h-2").unwrap(),
                reason_code: "budget_exhausted".into(),
            },
        ];
        let first = huddle_metrics(&events, &HuddleLedger::empty());
        let second = huddle_metrics(&events, &HuddleLedger::empty());
        assert_eq!(first, second);
        assert_eq!(first[METRIC_HUDDLES_TOTAL], 0);
        assert_eq!(first[METRIC_HUDDLES_REJECTED_TOTAL], 0);
        assert_eq!(first[METRIC_HUDDLE_TOKENS_TOTAL], 200);
        assert_eq!(first[METRIC_HUDDLE_COST_TOTAL], 6_500);
        assert_eq!(first[METRIC_HUDDLE_DURATION_MS], 1_000);
        assert_eq!(first[METRIC_HUDDLE_PARTICIPANTS], 3);
        assert!(!first.contains_key("position_huddles_total"));
        for omitted in [
            METRIC_COHERENCE_DEBT_BEFORE,
            METRIC_COHERENCE_DEBT_AFTER,
            METRIC_YTG_VELOCITY_PRE_HUDDLE,
            METRIC_YTG_VELOCITY_POST_HUDDLE,
            METRIC_REWORK_AVOIDED_ESTIMATE,
        ] {
            assert!(!first.contains_key(omitted));
        }
    }

    #[test]
    fn huddle_metrics_read_ledger_totals_scopes_and_contradictions() {
        use playbook_core::{BarrierMode, Down, EventId, EvidenceId, HuddleBudget, HuddleId};
        use playbook_huddle::{
            fold_huddle, ClaimRef, Contradiction, ContradictionId, ContradictionResolution,
            ContradictionSeverity, DisclosureProfile, DriveView, HuddleEvent, HuddleOutcome,
            HuddleParticipant, HuddleRole, HuddleScope, HuddleTrigger, HuddleTriggerKind,
            ResolutionState, SharedStateDelta,
        };
        let policy = playbook_policy::Policy::builtin();
        let view = DriveView {
            active: true,
            down: Down::FirstDown,
            ytg_milli: 10_000,
            field_fingerprint: "field-a".into(),
            evidence_ids: vec![EvidenceId::new("ev-1").unwrap()],
        };
        let budget = HuddleBudget {
            max_tokens: 2_000,
            max_cost_microunits: 200_000,
            max_duration_ms: 5_000,
            max_rounds: 2,
            max_participants: 3,
        };
        let trigger = |origin: &str| HuddleTrigger {
            kind: HuddleTriggerKind::Failure,
            reason_code: "repeated_failed_play".into(),
            evidence_refs: Vec::new(),
            originating_event: EventId::new(origin).unwrap(),
        };
        let participant = |personnel: &str| HuddleParticipant {
            personnel_id: playbook_core::PersonnelId::new(personnel).unwrap(),
            role: HuddleRole::Contributor,
            disclosure_profile: DisclosureProfile {
                include_gates: true,
                include_peer_reports: true,
                include_strategic_state: false,
                max_evidence_refs: 8,
            },
        };
        let h1 = HuddleId::new("h-1").unwrap();
        let h2 = HuddleId::new("h-2").unwrap();
        let h3 = HuddleId::new("h-3").unwrap();
        let ledger_events = vec![
            HuddleEvent::Requested {
                huddle_id: h1.clone(),
                scope: HuddleScope::Position,
                trigger: trigger("evt-1"),
                at_ms: 1,
                debt: 0,
                rework_avoided: 0,
            },
            HuddleEvent::Rejected {
                huddle_id: h1.clone(),
                reason_code: "budget_exhausted".into(),
            },
            HuddleEvent::Requested {
                huddle_id: h2.clone(),
                scope: HuddleScope::Unit,
                trigger: trigger("evt-2"),
                at_ms: 2,
                debt: 90,
                rework_avoided: 45,
            },
            HuddleEvent::Admitted {
                huddle_id: h2.clone(),
                budget,
                barrier: BarrierMode::Soft,
                participants: vec![participant("agent-a")],
            },
            HuddleEvent::Opened {
                huddle_id: h2.clone(),
            },
            HuddleEvent::ReportSubmitted {
                huddle_id: h2.clone(),
                report: playbook_huddle::HuddleReport {
                    participant: playbook_core::PersonnelId::new("agent-a").unwrap(),
                    verified_progress: Vec::new(),
                    blockers: Vec::new(),
                    changed_assumptions: Vec::new(),
                    dependency_changes: Vec::new(),
                    contradictions: Vec::new(),
                    proposed_action: None,
                    confidence_milli: 9_000,
                },
            },
            HuddleEvent::ContradictionDetected {
                huddle_id: h2.clone(),
                contradiction: Contradiction {
                    id: ContradictionId::new("con-1").unwrap(),
                    subject: "gate readiness".into(),
                    claims: vec![ClaimRef {
                        personnel_id: playbook_core::PersonnelId::new("agent-a").unwrap(),
                        statement: "gate is satisfied".into(),
                        evidence_ref: None,
                    }],
                    severity: ContradictionSeverity::High,
                },
            },
            HuddleEvent::OutcomeCommitted {
                huddle_id: h2.clone(),
                outcome: HuddleOutcome {
                    huddle_id: h2.clone(),
                    reconciled_state: SharedStateDelta {
                        verified_evidence: Vec::new(),
                        gate_snapshots: Vec::new(),
                        assumptions: Vec::new(),
                    },
                    resolved_contradictions: vec![ContradictionResolution {
                        contradiction_id: ContradictionId::new("con-1").unwrap(),
                        state: ResolutionState::ResolvedBySupersession,
                        evidence_ref: None,
                        authority: None,
                    }],
                    unresolved_contradictions: Vec::new(),
                    assignment_changes: Vec::new(),
                    strategy_change: None,
                    personnel_changes: Vec::new(),
                    revised_ytg_milli: None,
                    debt_after: Some(120),
                    next_play: None,
                    next_huddle_conditions: Vec::new(),
                    escalated_to: None,
                },
            },
            HuddleEvent::Requested {
                huddle_id: h3.clone(),
                scope: HuddleScope::Coordinator,
                trigger: trigger("evt-3"),
                at_ms: 3,
                debt: 0,
                rework_avoided: 0,
            },
            HuddleEvent::Admitted {
                huddle_id: h3.clone(),
                budget,
                barrier: BarrierMode::Soft,
                participants: vec![participant("agent-c")],
            },
            HuddleEvent::Opened {
                huddle_id: h3.clone(),
            },
            HuddleEvent::ContradictionDetected {
                huddle_id: h3.clone(),
                contradiction: Contradiction {
                    id: ContradictionId::new("con-2").unwrap(),
                    subject: "interface".into(),
                    claims: vec![ClaimRef {
                        personnel_id: playbook_core::PersonnelId::new("agent-c").unwrap(),
                        statement: "v0".into(),
                        evidence_ref: None,
                    }],
                    severity: ContradictionSeverity::Medium,
                },
            },
        ];
        let mut ledger = HuddleLedger::empty();
        fold_huddle(&mut ledger, &ledger_events, &view, &policy).unwrap();
        let metrics = huddle_metrics(&[], &ledger);
        assert_eq!(metrics, huddle_metrics(&[], &ledger));
        assert_eq!(metrics[METRIC_HUDDLES_TOTAL], 3);
        assert_eq!(metrics[METRIC_HUDDLES_REJECTED_TOTAL], 1);
        assert_eq!(metrics[METRIC_HUDDLE_ESCALATIONS], 0);
        assert_eq!(metrics["position_huddles_total"], 1);
        assert_eq!(metrics["unit_huddles_total"], 1);
        assert_eq!(metrics["coordinator_huddles_total"], 1);
        assert_eq!(metrics[METRIC_HUDDLE_CONTRADICTIONS_FOUND], 2);
        assert_eq!(metrics[METRIC_HUDDLE_CONTRADICTIONS_RESOLVED], 1);
        assert_eq!(metrics[METRIC_HUDDLE_TOKENS_TOTAL], 0);
        assert_eq!(metrics[METRIC_COHERENCE_DEBT_BEFORE], 90);
        assert_eq!(metrics[METRIC_COHERENCE_DEBT_AFTER], 120);
        assert_eq!(metrics[METRIC_REWORK_AVOIDED_ESTIMATE], 45);
        for omitted in [
            METRIC_YTG_VELOCITY_PRE_HUDDLE,
            METRIC_YTG_VELOCITY_POST_HUDDLE,
        ] {
            assert!(!metrics.contains_key(omitted));
        }
    }
}
