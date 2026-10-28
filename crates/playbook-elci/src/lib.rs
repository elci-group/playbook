//! ELCI is the capability plane.
//!
//! Playbook asks the plane what can be inspected, executed, and validated.
//! It does not keep a hard-coded tool list inside the down classifier.
//! `FixturePlane` is the deterministic stand-in used by simulation and tests.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use playbook_core::{
    ArtefactId, Capability, CapabilityId, DriveBudget, Evidence, EvidenceClass, Gate,
    PlayExecution, PlayResult, PlayStatus, PlaybookError, SideEffectClass, SideEffectNote,
    ToolFailure, ToolFailureKind,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    pub generation: u64,
    pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaneResult {
    Done(PlayResult),
    Failed(ToolFailure),
}

pub trait CapabilityPlane {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError>;
    fn execute(&mut self, execution: &PlayExecution) -> Result<PlaneResult, PlaybookError>;
}

#[derive(Clone, Debug)]
enum Script {
    Success {
        evidence: Vec<Evidence>,
        cost: DriveBudget,
        diagnostics: Vec<String>,
        side_effects: Vec<SideEffectNote>,
    },
    Failure {
        kind: ToolFailureKind,
        cost: DriveBudget,
        diagnostics: Vec<String>,
    },
}

#[derive(Clone, Debug)]
pub struct FixturePlane {
    generation: u64,
    capabilities: Vec<Capability>,
    scripts: BTreeMap<String, VecDeque<Script>>,
}

impl FixturePlane {
    pub fn mvp() -> Self {
        let mut plane = Self::base();
        plane.push(
            "inspect",
            Script::Success {
                evidence: vec![seal(
                    EvidenceClass::EnvironmentSnapshot,
                    json!({"host": "fixture", "tools": ["cargo"]}),
                    1_000,
                )],
                cost: tokens(20),
                diagnostics: vec!["environment inspected".into()],
                side_effects: Vec::new(),
            },
        );
        plane.push("compile", Script::Success {
            evidence: vec![seal(EvidenceClass::CompilerResult, json!({"gate_id": "implementation", "success": true, "command": "cargo check", "repository_hash": "abc"}), 2_000)],
            cost: tokens(200),
            diagnostics: vec!["compiler accepted the crate".into()],
            side_effects: Vec::new(),
        });
        plane.push(
            "test.unit",
            Script::Success {
                evidence: vec![seal(
                    EvidenceClass::TestReport,
                    test_body("unit_tests", "unit", 0, 4, 0),
                    3_000,
                )],
                cost: tokens(400),
                diagnostics: vec!["unit tests passed".into()],
                side_effects: Vec::new(),
            },
        );
        plane.push(
            "test.integration",
            Script::Failure {
                kind: ToolFailureKind::Timeout,
                cost: tokens(100),
                diagnostics: vec!["induced timeout".into()],
            },
        );
        plane.push(
            "test.integration",
            Script::Success {
                evidence: vec![seal(
                    EvidenceClass::TestReport,
                    test_body("integration_tests", "integration", 0, 3, 0),
                    9_000,
                )],
                cost: tokens(100),
                diagnostics: vec!["integration tests passed".into()],
                side_effects: Vec::new(),
            },
        );
        plane
    }

    /// Two timeouts, then a pass. Used to measure a control loop's duplicate failures.
    pub fn failing_twice() -> Self {
        let mut plane = Self::base();
        plane.push(
            "test.integration",
            Script::Failure {
                kind: ToolFailureKind::Timeout,
                cost: tokens(10),
                diagnostics: vec!["timeout".into()],
            },
        );
        plane.push(
            "test.integration",
            Script::Failure {
                kind: ToolFailureKind::Timeout,
                cost: tokens(10),
                diagnostics: vec!["timeout again".into()],
            },
        );
        plane.push(
            "test.integration",
            Script::Success {
                evidence: vec![seal(
                    EvidenceClass::TestReport,
                    test_body("integration_tests", "integration", 0, 1, 0),
                    4_000,
                )],
                cost: tokens(10),
                diagnostics: vec!["passed".into()],
                side_effects: Vec::new(),
            },
        );
        plane
    }

    pub fn ready() -> Self {
        Self::base()
    }

    pub fn empty() -> Self {
        Self {
            generation: 0,
            capabilities: Vec::new(),
            scripts: BTreeMap::new(),
        }
    }

    /// Align a recovered plane with the drive's capability generation. The
    /// script queue restarts; only the staleness check is resumed.
    pub fn resume_generation(&mut self, generation: u64) {
        self.generation = generation;
    }

    pub fn push_failure(&mut self, operation: &str, kind: ToolFailureKind) {
        self.push(
            operation,
            Script::Failure {
                kind,
                cost: tokens(1),
                diagnostics: vec![format!("{kind:?}")],
            },
        );
    }

    pub fn push_success(&mut self, operation: &str, evidence: Evidence, cost: DriveBudget) {
        self.push(
            operation,
            Script::Success {
                evidence: vec![evidence],
                cost,
                diagnostics: vec!["ok".into()],
                side_effects: Vec::new(),
            },
        );
    }

    pub fn push_unresolved(&mut self, operation: &str, evidence: Evidence, cost: DriveBudget) {
        self.push(
            operation,
            Script::Success {
                evidence: vec![evidence],
                cost,
                diagnostics: vec!["partial mutation".into()],
                side_effects: vec![SideEffectNote {
                    class: SideEffectClass::RepositoryMutation,
                    idempotency_key: String::new(),
                    summary: "partial repository mutation".into(),
                    resolved: false,
                    occurred: None,
                }],
            },
        );
    }

    fn base() -> Self {
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
            inputs_schema: json!({"type": "object"}),
            outputs_schema: json!({"type": "object"}),
            cost_model: tokens(1),
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
            scripts: BTreeMap::new(),
        }
    }

    fn push(&mut self, operation: &str, script: Script) {
        self.scripts
            .entry(operation.to_string())
            .or_default()
            .push_back(script);
    }
}

impl CapabilityPlane for FixturePlane {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        self.generation = self.generation.saturating_add(1);
        Ok(CapabilitySnapshot {
            generation: self.generation,
            capabilities: self.capabilities.clone(),
        })
    }

    fn execute(&mut self, execution: &PlayExecution) -> Result<PlaneResult, PlaybookError> {
        if execution.capability_generation != self.generation {
            return Err(PlaybookError::StaleCapability);
        }
        let Some(script) = self
            .scripts
            .get_mut(&execution.operation)
            .and_then(VecDeque::pop_front)
        else {
            return Ok(PlaneResult::Failed(failure(
                execution,
                ToolFailureKind::Unsupported,
                DriveBudget::zero(),
                vec!["operation is not in the capability snapshot queue".into()],
            )));
        };
        Ok(match script {
            Script::Success {
                evidence,
                cost,
                diagnostics,
                mut side_effects,
            } => {
                if let Some(key) = &execution.idempotency_key {
                    for effect in &mut side_effects {
                        if effect.idempotency_key.is_empty() {
                            effect.idempotency_key = key.clone();
                        }
                    }
                }
                PlaneResult::Done(PlayResult {
                    status: PlayStatus::Succeeded,
                    play_id: execution.play_id.clone(),
                    operation: execution.operation.clone(),
                    output_refs: vec![execution.operation.clone()],
                    evidence,
                    artefact_refs: Vec::<ArtefactId>::new(),
                    cost,
                    duration_ms: 10,
                    diagnostics,
                    side_effects,
                    at_ms: execution.timeout_ms,
                })
            }
            Script::Failure {
                kind,
                cost,
                diagnostics,
            } => PlaneResult::Failed(failure(execution, kind, cost, diagnostics)),
        })
    }
}

pub fn missing_evidence(gates: &[Gate], capabilities: &[Capability]) -> Vec<EvidenceClass> {
    let mut missing = Vec::new();
    for gate in gates {
        if !gate.mandatory || gate.is_terminal() {
            continue;
        }
        for class in &gate.required_evidence {
            if class.can_satisfy_gate()
                && !capabilities
                    .iter()
                    .any(|capability| capability.evidence_classes.contains(class))
                && !missing.contains(class)
            {
                missing.push(*class);
            }
        }
    }
    missing
}

pub fn classify_status(code: u16) -> ToolFailureKind {
    match code {
        429 => ToolFailureKind::RateLimit,
        401 | 403 => ToolFailureKind::Authorization,
        408 => ToolFailureKind::Timeout,
        404 => ToolFailureKind::Unsupported,
        400 | 422 => ToolFailureKind::InvalidInput,
        500 | 502 | 503 | 504 => ToolFailureKind::Transient,
        _ => ToolFailureKind::Unknown,
    }
}

fn failure(
    execution: &PlayExecution,
    kind: ToolFailureKind,
    cost: DriveBudget,
    diagnostics: Vec<String>,
) -> ToolFailure {
    ToolFailure {
        play_id: execution.play_id.clone(),
        play_kind: execution.play_kind,
        operation: execution.operation.clone(),
        capability_id: execution.capability_id.clone(),
        personnel_id: execution.personnel_id.clone(),
        kind,
        diagnostics,
        cost,
        at_ms: execution.timeout_ms,
        idempotency_key: execution.idempotency_key.clone(),
        side_effects: Vec::new(),
    }
}

fn tokens(tokens: u64) -> DriveBudget {
    DriveBudget {
        tokens,
        monetary_cost_micros: tokens.saturating_mul(100),
        tool_calls: 1,
        ..DriveBudget::zero()
    }
}

fn seal(class: EvidenceClass, body: Value, at_ms: u64) -> Evidence {
    Evidence::seal(class, body, at_ms).expect("fixture evidence")
}

fn test_body(gate: &str, suite: &str, exit_code: i64, passed: u64, failed: u64) -> Value {
    json!({
        "gate_id": gate,
        "command": "cargo test",
        "repository_hash": "abc",
        "test_suite": suite,
        "exit_code": exit_code,
        "passed": passed,
        "failed": failed,
        "ignored": 0,
        "timestamp": "fixture"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_map_to_failure_kinds() {
        assert_eq!(classify_status(429), ToolFailureKind::RateLimit);
        assert_eq!(classify_status(401), ToolFailureKind::Authorization);
        assert_eq!(classify_status(403), ToolFailureKind::Authorization);
        assert_eq!(classify_status(408), ToolFailureKind::Timeout);
        assert_eq!(classify_status(404), ToolFailureKind::Unsupported);
        assert_eq!(classify_status(400), ToolFailureKind::InvalidInput);
        assert_eq!(classify_status(422), ToolFailureKind::InvalidInput);
        for code in [500, 502, 503, 504] {
            assert_eq!(classify_status(code), ToolFailureKind::Transient);
        }
        assert_eq!(classify_status(418), ToolFailureKind::Unknown);
    }

    #[test]
    fn fixture_plane_exposes_verified_compiler_evidence() {
        use playbook_core::{
            CapabilityId, DriveId, PersonnelId, PlayId, PlayKind, RetryPolicy, SideEffectClass,
        };

        let mut plane = FixturePlane::mvp();
        let snapshot = plane.discover().unwrap();
        let execution = PlayExecution {
            drive_id: DriveId::new("fixture-evidence").unwrap(),
            play_id: PlayId::new("compile").unwrap(),
            play_kind: PlayKind::BaselineTest,
            operation: "compile".into(),
            down: playbook_core::Down::FirstDown,
            capability_id: CapabilityId::new("elci.cargo").unwrap(),
            capability_generation: snapshot.generation,
            personnel_id: PersonnelId::new("fixture-tool").unwrap(),
            inputs: json!({}),
            expected_evidence: vec![EvidenceClass::CompilerResult],
            expected_ytg_reduction_milli: 1,
            survey: false,
            budget_ceiling: DriveBudget::default(),
            timeout_ms: 1_000,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: SideEffectClass::ReadOnly,
            idempotency_key: None,
        };

        let PlaneResult::Done(mut result) = plane.execute(&execution).unwrap() else {
            panic!("fixture compile should succeed");
        };
        assert_eq!(result.output_refs, ["compile"]);
        let evidence = result
            .evidence
            .iter()
            .find(|evidence| evidence.class == EvidenceClass::CompilerResult)
            .expect("fixture compile should return compiler evidence");
        evidence.verify().unwrap();
        assert_eq!(evidence.body["success"], true);
        assert_ne!(evidence.id.as_str(), result.output_refs[0]);

        let evidence_before = result.evidence.clone();
        result.output_refs.clear();
        let json = serde_json::to_string(&result).unwrap();
        let restored: playbook_core::PlayResult = serde_json::from_str(&json).unwrap();
        assert!(restored.output_refs.is_empty());
        assert_eq!(restored.evidence, evidence_before);
    }
}
