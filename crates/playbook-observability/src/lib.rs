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
use playbook_huddle::HuddleScope;

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

#[derive(Clone, Debug, PartialEq, Eq)]
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
            _ => {}
        }
    }
    out
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
}
