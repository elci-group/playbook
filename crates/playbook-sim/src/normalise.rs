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
