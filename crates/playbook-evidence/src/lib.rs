//! Turn raw tool output into scored evidence.
//!
//! A narrative claim is stored and then ignored by the gate graph.

#![forbid(unsafe_code)]

use serde_json::Value;

use playbook_core::{EvidenceClass, PlaybookError, PPM_SCALE};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceScore {
    pub satisfaction_ppm: u32,
    pub failed: bool,
    pub applies_to_gates: bool,
    pub environment: Option<Value>,
}

impl EvidenceScore {
    fn inert() -> Self {
        Self {
            satisfaction_ppm: 0,
            failed: false,
            applies_to_gates: false,
            environment: None,
        }
    }
}

pub fn score(class: EvidenceClass, body: &Value) -> Result<EvidenceScore, PlaybookError> {
    match class {
        EvidenceClass::NarrativeClaim
        | EvidenceClass::GateTransition
        | EvidenceClass::CapabilitySnapshot => Ok(EvidenceScore::inert()),
        EvidenceClass::EnvironmentSnapshot => Ok(EvidenceScore {
            environment: Some(body.clone()),
            ..EvidenceScore::inert()
        }),
        EvidenceClass::CompilerResult => {
            let success = require_bool(body, "success")?;
            Ok(binary(success))
        }
        EvidenceClass::ArtefactValidation => Ok(binary(require_bool(body, "accepted")?)),
        EvidenceClass::Benchmark | EvidenceClass::StaticAnalysis => {
            Ok(binary(require_bool(body, "improved")?))
        }
        EvidenceClass::RepositoryMutation => Ok(binary(require_bool(body, "confirmed")?)),
        EvidenceClass::ToolInvocation => {
            let status = require_str(body, "status")?;
            Ok(binary(status == "ok"))
        }
        EvidenceClass::TestReport => score_test(body),
    }
}

pub fn targets_gate(body: &Value, gate_id: &str) -> bool {
    match body.get("gate_id").and_then(Value::as_str) {
        Some(id) => id == gate_id,
        None => true,
    }
}

fn binary(success: bool) -> EvidenceScore {
    EvidenceScore {
        satisfaction_ppm: if success { PPM_SCALE } else { 0 },
        failed: !success,
        applies_to_gates: true,
        environment: None,
    }
}

fn score_test(body: &Value) -> Result<EvidenceScore, PlaybookError> {
    let _command = require_str(body, "command")?;
    let _hash = require_str(body, "repository_hash")?;
    let _suite = require_str(body, "test_suite")?;
    let _timestamp = require_str(body, "timestamp")?;
    let exit_code = require_i64(body, "exit_code")?;
    let passed = require_u64(body, "passed")?;
    let failed = require_u64(body, "failed")?;
    let _ignored = require_u64(body, "ignored")?;
    let total = passed.saturating_add(failed);
    let satisfaction_ppm = if failed == 0 && exit_code == 0 && passed > 0 {
        PPM_SCALE
    } else {
        passed
            .saturating_mul(u64::from(PPM_SCALE))
            .checked_div(total)
            .and_then(|ratio| u32::try_from(ratio).ok())
            .unwrap_or(0)
    };
    let hard_fail = exit_code != 0 && passed == 0;
    Ok(EvidenceScore {
        satisfaction_ppm,
        failed: hard_fail || (failed > 0 && satisfaction_ppm == 0),
        applies_to_gates: true,
        environment: None,
    })
}

fn require_bool(body: &Value, key: &str) -> Result<bool, PlaybookError> {
    body.get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| PlaybookError::Evidence(format!("{key} must be a bool")))
}

fn require_str<'a>(body: &'a Value, key: &str) -> Result<&'a str, PlaybookError> {
    body.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PlaybookError::Evidence(format!("{key} must be a string")))
}

fn require_u64(body: &Value, key: &str) -> Result<u64, PlaybookError> {
    body.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| PlaybookError::Evidence(format!("{key} must be an unsigned integer")))
}

fn require_i64(body: &Value, key: &str) -> Result<i64, PlaybookError> {
    body.get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| PlaybookError::Evidence(format!("{key} must be an integer")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn narrative_claim_does_not_apply_to_gates() {
        let score = score(
            EvidenceClass::NarrativeClaim,
            &json!({"text": "implemented successfully"}),
        )
        .unwrap();
        assert!(!score.applies_to_gates);
        assert_eq!(score.satisfaction_ppm, 0);
    }

    #[test]
    fn test_report_requires_the_structured_record() {
        let err = score(EvidenceClass::TestReport, &json!({"passed": 1})).unwrap_err();
        assert!(matches!(err, PlaybookError::Evidence(_)));
    }

    #[test]
    fn partial_suite_is_proportional() {
        let score = score(
            EvidenceClass::TestReport,
            &json!({
                "command": "cargo test",
                "repository_hash": "abc",
                "test_suite": "integration",
                "exit_code": 1,
                "passed": 1,
                "failed": 1,
                "ignored": 0,
                "timestamp": "10"
            }),
        )
        .unwrap();
        assert_eq!(score.satisfaction_ppm, PPM_SCALE / 2);
        assert!(!score.failed);
    }
}
