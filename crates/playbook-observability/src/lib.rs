//! Live tactical telemetry.
//!
//! Explanations cite measurements. They do not quote model chain-of-thought.
//! Spans follow an OpenTelemetry shape: one drive is a trace, plays are spans,
//! tool calls are child spans. Export over OTLP is a deployment choice; the
//! model here is the contract.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use playbook_core::{ppm_percent, TacticalSnapshot, PPM_SCALE};
use playbook_events::DriveEvent;

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
    };
    for event in events {
        match event {
            DriveEvent::PlaySelected { .. } => rollup.plays += 1,
            DriveEvent::ToolFailed { .. } => rollup.tool_failures += 1,
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
}
