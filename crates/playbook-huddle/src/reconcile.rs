//! Report normalization, contradiction detection, and synthesis.
//!
//! Participant output is untrusted: it is schema-validated, length-bounded,
//! and normalized before it can enter the ledger. Contradictions become
//! explicit objects; none of them disappears because agents agree.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

use playbook_core::{EvidenceId, PlaybookError};

use crate::domain::{
    ActiveHuddle, ClaimRef, Contradiction, ContradictionId, ContradictionSeverity, HuddleOutcome,
    HuddleReport, SharedStateDelta,
};

const MAX_SUBJECT: usize = 200;
const MAX_STATEMENT: usize = 2_000;
const MAX_ARRAY: usize = 64;
const MAX_OBSERVATION_DETAIL: usize = 512;

#[tracing::instrument]
fn bounded_text(value: &Value, field: &str, max: usize) -> Result<(), PlaybookError> {
    if let Some(text) = value.get(field).and_then(Value::as_str) {
        if text.len() > max {
            return Err(PlaybookError::Evidence(format!(
                "{field} exceeds {max} bytes"
            )));
        }
    }
    Ok(())
}

/// Normalize raw participant output into a schema-valid report. Structured
/// output is mandatory; anything serde cannot read is refused.
pub #[tracing::instrument]
fn normalize_report(body: &Value) -> Result<HuddleReport, PlaybookError> {
    if !body.is_object() {
        return Err(PlaybookError::Evidence(
            "huddle report must be a JSON object".into(),
        ));
    }
    for field in [
        "participant",
        "verified_progress",
        "blockers",
        "changed_assumptions",
        "dependency_changes",
        "contradictions",
        "proposed_action",
        "confidence_milli",
    ] {
        if body.get(field).is_none() {
            return Err(PlaybookError::Evidence(format!(
                "huddle report is missing `{field}`"
            )));
        }
    }
    bounded_text(body, "participant", MAX_SUBJECT)?;
    for field in [
        "blockers",
        "changed_assumptions",
        "dependency_changes",
        "contradictions",
    ] {
        bounded_text(body, field, MAX_STATEMENT)?;
    }
    let confidence = body
        .get("confidence_milli")
        .and_then(Value::as_u64)
        .ok_or_else(|| PlaybookError::Evidence("confidence_milli must be an integer".into()))?;
    if confidence > u64::from(u16::MAX) {
        return Err(PlaybookError::Evidence(
            "confidence_milli is out of range".into(),
        ));
    }
    for field in [
        "verified_progress",
        "blockers",
        "changed_assumptions",
        "dependency_changes",
        "contradictions",
    ] {
        let length = body
            .get(field)
            .and_then(Value::as_array)
            .map(|array| array.len())
            .unwrap_or(0);
        if length > MAX_ARRAY {
            return Err(PlaybookError::Evidence(format!(
                "{field} exceeds {MAX_ARRAY} entries"
            )));
        }
    }
    let mut report: HuddleReport = serde_json::from_value(body.clone())
        .map_err(|error| PlaybookError::Evidence(format!("invalid huddle report: {error}")))?;
    report.confidence_milli = report.confidence_milli.min(10_000);
    Ok(report)
}

/// Structured cognitive exhaust (the CERS seam). Raw chain-of-thought is
/// refused: an observation is a bounded subject/detail pair.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub subject: String,
    pub detail: String,
    pub evidence_ref: Option<EvidenceId>,
}

pub #[tracing::instrument]
fn normalize_observation(body: &Value) -> Result<Observation, PlaybookError> {
    let observation: Observation = serde_json::from_value(body.clone())
        .map_err(|error| PlaybookError::Evidence(format!("invalid observation: {error}")))?;
    if observation.subject.is_empty() || observation.subject.len() > MAX_SUBJECT {
        return Err(PlaybookError::Evidence(
            "observation subject is unbounded".into(),
        ));
    }
    if observation.detail.is_empty() || observation.detail.len() > MAX_OBSERVATION_DETAIL {
        return Err(PlaybookError::Evidence(
            "observation detail must be a bounded statement, not a transcript".into(),
        ));
    }
    Ok(observation)
}

/// Detect contradictions across reports: two participants asserting
/// different current values for the same subject. Deterministic: subjects in
/// ascending order, ids assigned in that order.
pub #[tracing::instrument]
fn detect_contradictions(reports: &[HuddleReport]) -> Vec<Contradiction> {
    use std::collections::BTreeMap;
    let mut subjects: BTreeMap<String, Vec<ClaimRef>> = BTreeMap::new();
    for report in reports {
        for delta in &report.changed_assumptions {
            let entry = subjects.entry(delta.subject.clone()).or_default();
            if entry
                .iter()
                .any(|claim| claim.personnel_id == report.participant)
            {
                continue;
            }
            entry.push(ClaimRef {
                personnel_id: report.participant.clone(),
                statement: delta.current.clone(),
                evidence_ref: delta.evidence_ref.clone(),
            });
        }
    }
    let mut found = Vec::new();
    for (index, (subject, claims)) in subjects.iter().enumerate() {
        let distinct: Vec<&ClaimRef> = {
            let mut seen: Vec<&str> = Vec::new();
            claims
                .iter()
                .filter(|claim| {
                    if seen.contains(&claim.statement.as_str()) {
                        false
                    } else {
                        seen.push(claim.statement.as_str());
                        true
                    }
                })
                .collect()
        };
        if distinct.len() < 2 {
            continue;
        }
        let severity = if distinct.len() >= 3 {
            ContradictionSeverity::High
        } else {
            ContradictionSeverity::Medium
        };
        found.push(Contradiction {
            id: ContradictionId::new(format!("con-{index:03}")).expect("bounded id"),
            subject: subject.clone(),
            claims: distinct.into_iter().cloned().collect(),
            severity,
        });
    }
    found
}

/// Forced synthesis after budget exhaustion: commit what the accumulated
/// reports support, resolve nothing, and hand the blocking remainder to the
/// next scope up.
pub #[tracing::instrument]
fn forced_synthesis(huddle: &ActiveHuddle) -> HuddleOutcome {
    let mut verified: Vec<EvidenceId> = huddle
        .reports
        .iter()
        .flat_map(|report| report.verified_progress.iter().cloned())
        .collect();
    verified.sort_unstable();
    verified.dedup();
    HuddleOutcome {
        huddle_id: huddle.id.clone(),
        reconciled_state: SharedStateDelta {
            verified_evidence: verified,
            gate_snapshots: Vec::new(),
            assumptions: Vec::new(),
        },
        resolved_contradictions: Vec::new(),
        unresolved_contradictions: huddle.contradictions.clone(),
        assignment_changes: Vec::new(),
        strategy_change: None,
        personnel_changes: Vec::new(),
        revised_ytg_milli: None,
        debt_after: None,
        next_play: None,
        next_huddle_conditions: vec![crate::domain::HuddleCondition {
            kind: crate::domain::HuddleTriggerKind::CoherenceDebt,
            reason_code: "post_synthesis_review".into(),
        }],
        escalated_to: huddle.scope.next_up(),
    }
}

/// The Rust authority for report shape. A missing or failing Tov adapter
/// falls back here, deterministically.
pub #[tracing::instrument]
fn report_shape_is_valid(body: &Value) -> bool {
    normalize_report(body).is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NormalizedOutcome {
    pub accepted: bool,
    pub isolated: bool,
    pub source: &'static str,
}

/// Try the Tov adapter first; any failure falls back to the Rust authority.
pub #[tracing::instrument]
fn try_tov_normalize_report(
    script: &Path,
    body: &Value,
) -> Result<NormalizedOutcome, PlaybookError> {
    if !script.is_file() {
        return rust_outcome(body);
    }
    let payload = serde_json::to_string(body).map_err(PlaybookError::json)?;
    let ran = Command::new("tov").arg(script).arg(&payload).output();
    match ran {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<Value>(text.trim()) {
                if let Some(accepted) = parsed.get("accepted").and_then(Value::as_bool) {
                    return Ok(NormalizedOutcome {
                        accepted,
                        isolated: true,
                        source: "tov",
                    });
                }
            }
            rust_outcome(body)
        }
        _ => rust_outcome(body),
    }
}

#[tracing::instrument]
fn rust_outcome(body: &Value) -> Result<NormalizedOutcome, PlaybookError> {
    Ok(NormalizedOutcome {
        accepted: report_shape_is_valid(body),
        isolated: true,
        source: "rust",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::PersonnelId;
    use serde_json::json;

    #[tracing::instrument]
fn raw_report(participant: &str, confidence: u64) -> Value {
        json!({
            "participant": participant,
            "verified_progress": [],
            "blockers": [],
            "changed_assumptions": [],
            "dependency_changes": [],
            "contradictions": [],
            "proposed_action": null,
            "confidence_milli": confidence
        })
    }

    #[test]
    #[tracing::instrument]
fn reports_normalize_and_junk_is_refused() {
        let report = normalize_report(&raw_report("worker", 9_500)).unwrap();
        assert_eq!(report.participant.as_str(), "worker");
        assert_eq!(report.confidence_milli, 9_500);
        assert!(normalize_report(&json!({"participant": "x"})).is_err());
        assert!(normalize_report(&json!("not an object")).is_err());
        let overconfident = normalize_report(&raw_report("worker", 15_000)).unwrap();
        assert_eq!(overconfident.confidence_milli, 10_000);
        assert!(normalize_report(&raw_report("worker", 70_000)).is_err());
        let mut oversized = raw_report("worker", 1);
        oversized["blockers"] = json!(vec!["x"; MAX_ARRAY + 1]);
        assert!(normalize_report(&oversized).is_err());
    }

    #[test]
    #[tracing::instrument]
fn observations_reject_transcripts() {
        let ok = normalize_observation(&json!({
            "subject": "crate-x",
            "detail": "crate X failed three times because feature Y is unavailable offline",
            "evidence_ref": null
        }))
        .unwrap();
        assert_eq!(ok.subject, "crate-x");
        let transcript = "x".repeat(MAX_OBSERVATION_DETAIL + 1);
        assert!(normalize_observation(&json!({
            "subject": "crate-x",
            "detail": transcript,
            "evidence_ref": null
        }))
        .is_err());
    }

    #[test]
    #[tracing::instrument]
fn contradiction_detection_is_deterministic_and_bounded() {
        let claim = |participant: &str, current: &str| HuddleReport {
            participant: PersonnelId::new(participant).unwrap(),
            verified_progress: Vec::new(),
            blockers: Vec::new(),
            changed_assumptions: vec![crate::domain::AssumptionDelta {
                subject: "interface".into(),
                previous: "v1".into(),
                current: current.into(),
                evidence_ref: None,
            }],
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            proposed_action: None,
            confidence_milli: 8_000,
        };
        let reports = vec![
            claim("agent-b", "v2"),
            claim("agent-a", "v1"),
            claim("agent-c", "v3"),
        ];
        let found = detect_contradictions(&reports);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id.as_str(), "con-000");
        assert_eq!(found[0].subject, "interface");
        assert_eq!(found[0].claims.len(), 3);
        assert_eq!(found[0].severity, ContradictionSeverity::High);
        assert_eq!(found, detect_contradictions(&reports));
        let agreeing = vec![claim("agent-a", "v1"), claim("agent-b", "v1")];
        assert!(detect_contradictions(&agreeing).is_empty());
    }
}
