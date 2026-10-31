//! Traci authority adapter — records literalness drift (`ci/traci`).
//!
//! Advisory by policy: traci's findings are recorded evidence but never
//! independently prevent admission (§4).

use std::time::Duration;

use ci_evidence::{Finding, GateOutcome, GateRecord};

use crate::runner::{authority_unavailable, binary_available, run_gate_capturing};
use crate::GateSpec;

/// The default traci binary name.
pub const TRACI_BINARY: &str = "traci";

/// Run the traci gate against the repository working directory.
/// Skips with `SkippedNotApplicable` when the binary is absent.
pub fn traci_gate(policy_version: &str) -> GateRecord {
    traci_gate_binary(TRACI_BINARY, policy_version)
}

/// [`traci_gate`] with an injected binary name, so tests can force the
/// missing-binary path.
pub fn traci_gate_binary(binary: &str, policy_version: &str) -> GateRecord {
    let spec = GateSpec::new(
        "ci/traci",
        "traci",
        ci_evidence::GateClassification::Advisory,
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

    // Observed traci CLI (v0.1.21) does not accept `--color`/`--no-emoji`;
    // `check` defaults to human output and machines should ask for JSON.
    let (mut record, output) = run_gate_capturing(
        &spec,
        policy_version,
        // The gate observes the working tree as a whole.
        "worktree",
        binary,
        &["check", ".", "--format", "json"],
        Duration::from_secs(120),
    );
    record.findings = parse_traci_output(&output.stdout, &output.stderr);
    record
}

/// Reduce traci output to findings. `traci check --format json` emits a
/// `{"diagnostics": [{"rule", "severity", "path", "line", "message",
/// ...}]}` object (observed on traci 0.1.21); those entries become
/// structured findings. Any other output falls back to scanning lines
/// mentioning "warning" or "error". The gate verdict still comes from
/// the exit code; findings are advisory detail only.
fn parse_traci_output(stdout: &str, stderr: &str) -> Vec<Finding> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
        if let Some(findings) = findings_from_diagnostics(&value) {
            return findings;
        }
    }
    let mut findings = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let lower = line.to_lowercase();
        let severity = if lower.contains("error") {
            "error"
        } else if lower.contains("warning") {
            "warning"
        } else {
            continue;
        };
        findings.push(Finding {
            code: if severity == "error" {
                "traci_error".to_string()
            } else {
                "traci_warning".to_string()
            },
            severity: severity.to_string(),
            message: line.trim().to_string(),
        });
    }
    findings
}

/// Extract `diagnostics` entries as findings; `None` when the JSON is
/// not a diagnostics-shaped object.
fn findings_from_diagnostics(value: &serde_json::Value) -> Option<Vec<Finding>> {
    let diagnostics = value.get("diagnostics")?.as_array()?;
    Some(
        diagnostics
            .iter()
            .filter_map(|d| {
                let obj = d.as_object()?;
                let rule = obj
                    .get("rule")
                    .and_then(|v| v.as_str())
                    .unwrap_or("traci_unknown_rule");
                let severity = obj
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("info")
                    .to_string();
                let location = match (obj.get("path"), obj.get("line")) {
                    (Some(p), Some(l)) => format!("{}:{l}", p.as_str().unwrap_or_default()),
                    (Some(p), None) => p.as_str().unwrap_or_default().to_string(),
                    _ => String::new(),
                };
                let message = obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                Some(Finding {
                    code: rule.to_string(),
                    severity,
                    message: if location.is_empty() {
                        message.to_string()
                    } else {
                        format!("{location}: {message}")
                    },
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diagnostics_json() {
        let stdout = r#"{"diagnostics":[
            {"rule":"TRC001","severity":"error","path":"src/runner.rs","line":136,"column":13,"message":"failure path has no nearby structured diagnostic event"},
            {"rule":"TRC003","severity":"warning","path":"src/lib.rs","line":2,"column":1,"message":"drift"}
        ]}"#;
        let findings = parse_traci_output(stdout, "");
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, "TRC001");
        assert_eq!(findings[0].severity, "error");
        assert_eq!(
            findings[0].message,
            "src/runner.rs:136: failure path has no nearby structured diagnostic event"
        );
        assert_eq!(findings[1].severity, "warning");
    }

    #[test]
    fn parses_warnings_and_errors() {
        let findings = parse_traci_output(
            "src/lib.rs:1: warning: literal drift detected\nok line\nsrc/x.rs: error: unliteral claim",
            "",
        );
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].code, "traci_warning");
        assert_eq!(findings[1].code, "traci_error");
    }

    #[test]
    fn skips_cleanly_when_binary_missing() {
        let record = traci_gate_binary("definitely-not-traci-xyz", "test-policy");
        assert_eq!(record.outcome, GateOutcome::SkippedNotApplicable);
        assert_eq!(record.gate_id, "ci/traci");
        assert_eq!(record.findings.len(), 1);
        assert!(record.findings[0]
            .message
            .contains("authority binary unavailable"));
    }
}
