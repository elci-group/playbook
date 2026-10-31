//! Amber authority adapter — dependency-replacement review evidence
//! (`ci/amber`).
//!
//! Conditional by policy: an amber failure blocks only when the current
//! policy declares it required. Amber itself is invoked through the
//! `amber` skill/CLI in normal use; here we record its CI gate evidence.

use std::time::Duration;

use serde_json::Value;

use ci_evidence::{Finding, GateOutcome, GateRecord};

use crate::runner::{authority_unavailable, binary_available, run_gate_capturing};
use crate::GateSpec;

/// The default amber binary name.
pub const AMBER_BINARY: &str = "amber";

/// Run the amber gate over the repository root (the current working
/// directory's parent chain is not walked; `root` is used verbatim).
/// Skips with `SkippedNotApplicable` when the binary is absent.
pub fn amber_gate(root: &str, policy_version: &str) -> GateRecord {
    amber_gate_binary(AMBER_BINARY, root, policy_version)
}

/// [`amber_gate`] with an injected binary name, so tests can force the
/// missing-binary path.
///
/// Strategy: run `amber --format json <root>` first; if that invocation
/// errors, fall back to bare `amber` (no args). If stdout parses as
/// JSON, per-finding entries are extracted into structured findings;
/// otherwise the raw output is recorded as one finding.
pub fn amber_gate_binary(binary: &str, root: &str, policy_version: &str) -> GateRecord {
    let spec = GateSpec::new(
        "ci/amber",
        "amber",
        ci_evidence::GateClassification::Conditional,
    );
    if !binary_available(binary) {
        return GateRecord {
            gate_id: spec.gate_id,
            authority: spec.authority,
            authority_version: "unversioned".to_string(),
            policy_version: policy_version.to_string(),
            classification: spec.classification,
            started_at_ms: 0,
            completed_at_ms: 0,
            input_hash: String::new(),
            outcome: GateOutcome::SkippedNotApplicable,
            exit_code: None,
            findings: vec![authority_unavailable(binary)],
            artifact_hashes: Vec::new(),
            duration_ms: 0,
        };
    }

    let (record, output) = run_gate_capturing(
        &spec,
        policy_version,
        root,
        binary,
        &["--format", "json", root],
        Duration::from_secs(120),
    );

    if matches!(record.outcome, GateOutcome::Error) {
        // `amber --format json <root>` could not even run; retry bare.
        let (mut record, output) = run_gate_capturing(
            &spec,
            policy_version,
            root,
            binary,
            &[],
            Duration::from_secs(120),
        );
        record.findings = findings_from_output(&output.stdout, &output.stderr);
        return record;
    }

    let mut record = record;
    record.findings = findings_from_output(&output.stdout, &output.stderr);
    record
}

/// Reduce amber's output to findings: structured extraction from JSON
/// stdout when it parses, otherwise the raw output as one finding.
fn findings_from_output(stdout: &str, stderr: &str) -> Vec<Finding> {
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        let findings = findings_from_json(&value);
        if !findings.is_empty() {
            return findings;
        }
    }
    let raw = if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        stdout.trim().to_string()
    };
    if raw.is_empty() {
        return Vec::new();
    }
    vec![Finding {
        code: "amber_raw_output".to_string(),
        severity: "info".to_string(),
        message: raw,
    }]
}

/// Extract per-finding entries from amber's JSON. The schema is not
/// contractually frozen, so this is defensive: accept a top-level array
/// or an object carrying a `findings` array, and read `code`/`id`,
/// `severity`/`level`, and `message`/`description` fields with sensible
/// fallbacks.
fn findings_from_json(value: &Value) -> Vec<Finding> {
    let entries = match value {
        Value::Array(items) => Some(items.clone()),
        Value::Object(map) => map.get("findings").and_then(|f| f.as_array()).cloned(),
        _ => None,
    };
    let Some(entries) = entries else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let obj = entry.as_object()?;
            let code = obj
                .get("code")
                .or_else(|| obj.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("amber_finding")
                .to_string();
            let severity = obj
                .get("severity")
                .or_else(|| obj.get("level"))
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            let message = obj
                .get("message")
                .or_else(|| obj.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            Some(Finding {
                code,
                severity,
                message,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_structured_findings_from_json() {
        let value = json!({
            "findings": [
                {"code": "dep_replaceable", "severity": "warning", "message": "use std"},
                {"severity": "info", "message": "kept"}
            ]
        });
        let findings = findings_from_json(&value);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, "dep_replaceable");
        assert_eq!(findings[0].severity, "warning");
        assert_eq!(findings[1].code, "amber_finding");
    }

    #[test]
    fn raw_output_becomes_one_finding_when_json_does_not_parse() {
        let findings = findings_from_output("not json at all", "");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "amber_raw_output");
        assert_eq!(findings[0].message, "not json at all");
    }

    #[test]
    fn skips_cleanly_when_binary_missing() {
        let record = amber_gate_binary("definitely-not-amber-xyz", ".", "test-policy");
        assert_eq!(record.outcome, GateOutcome::SkippedNotApplicable);
        assert_eq!(record.gate_id, "ci/amber");
        assert!(record.findings[0]
            .message
            .contains("authority binary unavailable"));
    }
}
