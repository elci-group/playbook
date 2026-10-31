//! Rust remains the authority when a Tov adapter is missing or fails to run.

use std::path::Path;
use std::process::Command;

use playbook_core::{EvidenceClass, PlaybookError, PPM_SCALE};
use playbook_evidence::score;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NormaliseOutcome {
    pub accepted: bool,
    pub isolated: bool,
    pub source: &'static str,
}

pub fn normalise_test_report(body: &Value) -> Result<NormaliseOutcome, PlaybookError> {
    let scored = score(EvidenceClass::TestReport, body)?;
    Ok(NormaliseOutcome {
        accepted: !scored.failed && scored.satisfaction_ppm == PPM_SCALE,
        isolated: true,
        source: "rust",
    })
}

pub fn try_tov_normalise(script: &Path, body: &Value) -> Result<NormaliseOutcome, PlaybookError> {
    if !script.is_file() {
        return normalise_test_report(body);
    }
    let payload = serde_json::to_string(body).map_err(PlaybookError::json)?;
    let ran = Command::new("tov").arg(script).arg(&payload).output();
    match ran {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<Value>(text.trim()) {
                if let Some(accepted) = parsed.get("accepted").and_then(Value::as_bool) {
                    return Ok(NormaliseOutcome {
                        accepted,
                        isolated: true,
                        source: "tov",
                    });
                }
            }
            normalise_test_report(body)
        }
        _ => normalise_test_report(body),
    }
}

/// The Rust authority for a field-geometry boundary presentation: the
/// consolidation checkpoint must be complete AND the presented evidence
/// must cover every class the boundary requires (all-of, I-13).
pub fn normalise_field_boundary(body: &Value) -> Result<NormaliseOutcome, PlaybookError> {
    let boundary = body
        .get("boundary_classes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            PlaybookError::Inadmissible(
                "field".into(),
                "boundary presentation names its required classes".into(),
            )
        })?;
    let presented = body
        .get("presented_classes")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<&str>>()
        })
        .unwrap_or_default();
    let checkpoint_complete = body
        .get("checkpoint_complete")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let covered = boundary.iter().all(|need| {
        need.as_str()
            .map(|need| presented.contains(&need))
            .unwrap_or(false)
    });
    Ok(NormaliseOutcome {
        accepted: checkpoint_complete && covered,
        isolated: true,
        source: "rust",
    })
}

/// Tov-first field-boundary normalisation: when the adapter script exists
/// and runs, its verdict stands; otherwise — or when it fails — Rust
/// remains the authority (failure isolation, I-11).
pub fn try_tov_field_normalise(
    script: &Path,
    body: &Value,
) -> Result<NormaliseOutcome, PlaybookError> {
    if !script.is_file() {
        return normalise_field_boundary(body);
    }
    let payload = serde_json::to_string(body).map_err(PlaybookError::json)?;
    let ran = Command::new("tov").arg(script).arg(&payload).output();
    match ran {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = serde_json::from_str::<Value>(text.trim()) {
                if let Some(accepted) = parsed.get("accepted").and_then(Value::as_bool) {
                    return Ok(NormaliseOutcome {
                        accepted,
                        isolated: true,
                        source: "tov",
                    });
                }
            }
            normalise_field_boundary(body)
        }
        _ => normalise_field_boundary(body),
    }
}
