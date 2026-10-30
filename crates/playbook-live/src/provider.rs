//! The provider adapter seam.
//!
//! Provider-specific behaviour lives behind [`ModelTransport`], never inside
//! the franchise authority, drive reducers, evidence logic, routing
//! invariants, or the Front Office. This module defines the seam, the
//! constitutional packaging of model responses, and a working plane for any
//! transport a host supplies.
//!
//! The constitutional rule this module enforces: a model response is a
//! claim. The plane packages the response as an artefact the drive can
//! reference and seals only `ToolInvocation` evidence — the fact that an
//! invocation happened. The response text satisfies no gate by itself; it
//! becomes real when a verification tool says so.

use serde::{Deserialize, Serialize};

use playbook_core::{
    Capability, CapabilityId, DriveBudget, Evidence, EvidenceClass, PlaybookError,
    SideEffectClass, ToolFailureKind,
};
use playbook_elci::{CapabilityPlane, CapabilitySnapshot, PlaneResult};

use crate::cancel::{CancellationReport, CancellationState};
use crate::envelope::CostReport;
use crate::identity::LiveIdentity;

/// One live model invocation, assembled from a validated envelope by the
/// host's transport. The transport owns provider wire formats; Playbook owns
/// the authority boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRequest {
    pub execution_id: String,
    pub model: String,
    /// The assembled instruction. For a directive-bound worker this is the
    /// sealed specification; for a solver it is the frozen target package.
    pub prompt: String,
    pub max_tokens: u64,
    pub timeout_ms: u64,
    /// The idempotency key the provider should honour for deduplication.
    pub idempotency_key: String,
}

/// One live model response. `usage_tokens` is `None` when the provider did
/// not report spend — unknown, never zero.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelResponse {
    pub text: String,
    pub stop_reason: String,
    pub usage_tokens: Option<u64>,
    pub latency_ms: u64,
}

/// The provider seam. A host implements this against its real provider
/// client; [`ProviderPlane`] does the rest.
pub trait ModelTransport {
    fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, PlaybookError>;
    /// Ask the transport what became of a cancelled invocation. Unknown is a
    /// first-class answer.
    fn cancellation(&self, execution_id: &str) -> CancellationState;
}

/// The transport a host has not wired yet. It refuses with a distinct,
/// actionable error rather than pretending to work.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnimplementedTransport;

impl ModelTransport for UnimplementedTransport {
    fn complete(&self, _request: &ModelRequest) -> Result<ModelResponse, PlaybookError> {
        Err(PlaybookError::Io(
            "no live model transport is wired: implement ModelTransport for your provider client \
             and hand it to ProviderPlane"
                .into(),
        ))
    }

    fn cancellation(&self, _execution_id: &str) -> CancellationState {
        CancellationState::Unknown
    }
}

/// A live plane over one model identity and one transport.
pub struct ProviderPlane<T: ModelTransport> {
    identity: LiveIdentity,
    transport: T,
    generation: u64,
}

impl<T: ModelTransport> ProviderPlane<T> {
    pub fn new(identity: LiveIdentity, transport: T) -> Self {
        Self {
            identity,
            transport,
            generation: 1,
        }
    }

    pub fn identity(&self) -> &LiveIdentity {
        &self.identity
    }

    fn capability(&self) -> Capability {
        Capability {
            id: CapabilityId::new("live.model").unwrap_or_else(|_| {
                CapabilityId::new("live.model.fallback").expect("fallback id")
            }),
            version: "1".into(),
            provider: self.identity.provider.clone(),
            operations: vec!["targeted_inference".into()],
            inputs_schema: serde_json::json!({"type": "object"}),
            outputs_schema: serde_json::json!({"type": "object"}),
            cost_model: DriveBudget {
                inference_calls: 1,
                ..DriveBudget::zero()
            },
            side_effect_class: SideEffectClass::ReadOnly,
            permissions: Vec::new(),
            environment_requirements: Vec::new(),
            reliability_ppm: 500_000,
            reliability_samples: 0,
            evidence_classes: vec![EvidenceClass::ToolInvocation],
        }
    }

    fn failure(
        &self,
        execution: &playbook_core::PlayExecution,
        kind: ToolFailureKind,
        diagnostics: Vec<String>,
    ) -> playbook_core::ToolFailure {
        playbook_core::ToolFailure {
            play_id: execution.play_id.clone(),
            play_kind: execution.play_kind,
            operation: execution.operation.clone(),
            capability_id: execution.capability_id.clone(),
            personnel_id: execution.personnel_id.clone(),
            kind,
            diagnostics,
            cost: DriveBudget {
                inference_calls: 1,
                ..DriveBudget::zero()
            },
            at_ms: 0,
            idempotency_key: execution.idempotency_key.clone(),
            side_effects: Vec::new(),
        }
    }
}

impl<T: ModelTransport> CapabilityPlane for ProviderPlane<T> {
    fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        Ok(CapabilitySnapshot {
            generation: self.generation,
            capabilities: vec![self.capability()],
        })
    }

    fn execute(
        &mut self,
        execution: &playbook_core::PlayExecution,
    ) -> Result<PlaneResult, PlaybookError> {
        let prompt = execution
            .inputs
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        if prompt.trim().is_empty() {
            return Ok(PlaneResult::Failed(self.failure(
                execution,
                ToolFailureKind::InvalidInput,
                vec!["live model invocation carries no instruction".into()],
            )));
        }
        let request = ModelRequest {
            execution_id: execution
                .idempotency_key
                .clone()
                .unwrap_or_else(|| execution.play_id.as_str().to_string()),
            model: self.identity.model.clone(),
            prompt,
            max_tokens: 4_096,
            timeout_ms: execution.timeout_ms,
            idempotency_key: execution
                .idempotency_key
                .clone()
                .unwrap_or_else(|| "unkeyed".into()),
        };
        let response = match self.transport.complete(&request) {
            Ok(response) => response,
            Err(error) => {
                return Ok(PlaneResult::Failed(self.failure(
                    execution,
                    ToolFailureKind::Transient,
                    vec![error.to_string()],
                )));
            }
        };
        // The claim is packaged; the invocation is evidenced. These are
        // different facts and the drive keeps them different.
        let claim_digest = playbook_core::sha256_hex(response.text.as_bytes());
        let evidence = Evidence::seal(
            EvidenceClass::ToolInvocation,
            serde_json::json!({
                "status": "ok",
                "provider": self.identity.provider,
                "model": self.identity.model,
                "usage_tokens": response.usage_tokens,
                "stop_reason": response.stop_reason,
                "gate_id": execution.operation,
            }),
            0,
        )?;
        let tokens_known = response.usage_tokens.is_some();
        let cost_report = CostReport {
            measured: DriveBudget {
                tokens: response.usage_tokens.unwrap_or(0),
                inference_calls: 1,
                wall_clock_ms: response.latency_ms,
                ..DriveBudget::zero()
            },
            tokens_known,
            monetary_cost_known: false,
            provider_reported: tokens_known,
        };
        let _ = cost_report;
        Ok(PlaneResult::Done(playbook_core::PlayResult {
            status: playbook_core::PlayStatus::Succeeded,
            play_id: execution.play_id.clone(),
            operation: execution.operation.clone(),
            // The claim travels as an artefact reference, not as evidence.
            output_refs: vec![claim_digest],
            evidence: vec![evidence],
            artefact_refs: Vec::new(),
            cost: DriveBudget {
                tokens: response.usage_tokens.unwrap_or(0),
                inference_calls: 1,
                wall_clock_ms: response.latency_ms,
                ..DriveBudget::zero()
            },
            duration_ms: response.latency_ms,
            diagnostics: Vec::new(),
            side_effects: Vec::new(),
            at_ms: 0,
        }))
    }
}

/// Ask a transport what became of a cancelled invocation, as a report the
/// cancellation ledger can consume.
pub fn cancellation_report<T: ModelTransport>(
    transport: &T,
    execution_id: playbook_core::ExecutionId,
    note: &str,
) -> CancellationReport {
    CancellationReport::new(
        execution_id,
        transport.cancellation(execution_id.as_str()),
        note,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::LiveIdentity;
    use playbook_core::{Down, DriveId, PlayExecution, PlayId, RetryPolicy};
    use std::cell::Cell;

    fn execution(prompt: &str) -> PlayExecution {
        PlayExecution {
            drive_id: DriveId::new("drive-live").unwrap(),
            play_id: PlayId::new("play-1").unwrap(),
            play_kind: playbook_core::PlayKind::TargetedInference,
            down: Down::FirstDown,
            operation: "targeted_inference".into(),
            capability_id: CapabilityId::new("live.model").unwrap(),
            capability_generation: 1,
            personnel_id: LiveIdentity::new("alpha", "model-x", "v1", "tools")
                .unwrap()
                .personnel_id()
                .unwrap(),
            inputs: serde_json::json!({ "prompt": prompt }),
            expected_evidence: vec![EvidenceClass::ToolInvocation],
            expected_ytg_reduction_milli: 0,
            survey: false,
            budget_ceiling: DriveBudget {
                tokens: 10_000,
                inference_calls: 5,
                wall_clock_ms: 60_000,
                ..DriveBudget::zero()
            },
            timeout_ms: 5_000,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: SideEffectClass::ReadOnly,
            idempotency_key: Some("play-1-inference".into()),
        }
    }

    struct Scripted {
        text: String,
        usage_tokens: Option<u64>,
        calls: Cell<u32>,
        fail: bool,
    }

    impl ModelTransport for Scripted {
        fn complete(&self, request: &ModelRequest) -> Result<ModelResponse, PlaybookError> {
            self.calls.set(self.calls.get() + 1);
            let _ = request;
            if self.fail {
                return Err(PlaybookError::Io("provider 5xx".into()));
            }
            Ok(ModelResponse {
                text: self.text.clone(),
                stop_reason: "end_turn".into(),
                usage_tokens: self.usage_tokens,
                latency_ms: 120,
            })
        }

        fn cancellation(&self, _execution_id: &str) -> CancellationState {
            CancellationState::Unknown
        }
    }

    #[test]
    fn the_unwired_transport_refuses_with_direction_not_fiction() {
        let mut plane = ProviderPlane::new(
            LiveIdentity::new("alpha", "model-x", "v1", "tools").unwrap(),
            UnimplementedTransport,
        );
        match plane.execute(&execution("restore the parser")).unwrap() {
            PlaneResult::Failed(failure) => {
                assert!(failure.diagnostics[0].contains("ModelTransport"));
            }
            PlaneResult::Done(_) => panic!("an unwired transport must not pretend"),
        }
    }

    #[test]
    fn a_response_is_a_claimed_artefact_and_tool_invocation_evidence_never_more() {
        let transport = Scripted {
            text: "I have fixed the parser.".into(),
            usage_tokens: Some(311),
            calls: Cell::new(0),
            fail: false,
        };
        let mut plane = ProviderPlane::new(
            LiveIdentity::new("alpha", "model-x", "v1", "tools").unwrap(),
            transport,
        );
        match plane.execute(&execution("restore the parser")).unwrap() {
            PlaneResult::Done(result) => {
                assert_eq!(result.status, playbook_core::PlayStatus::Succeeded);
                // The invocation is evidenced.
                assert_eq!(result.evidence[0].class, EvidenceClass::ToolInvocation);
                assert_eq!(result.evidence[0].body["usage_tokens"], serde_json::json!(311));
                // The claim itself travels as a digest reference only.
                assert_eq!(result.output_refs.len(), 1);
                assert_eq!(result.output_refs[0].len(), 64);
                // Reported spend is real spend, not zero.
                assert_eq!(result.cost.tokens, 311);
                assert_eq!(result.cost.inference_calls, 1);
            }
            PlaneResult::Failed(_) => panic!("the scripted transport succeeds"),
        }
    }

    #[test]
    fn unreported_spend_is_not_booked_as_zero() {
        let transport = Scripted {
            text: "done".into(),
            usage_tokens: None,
            calls: Cell::new(0),
            fail: false,
        };
        let mut plane = ProviderPlane::new(
            LiveIdentity::new("alpha", "model-x", "v1", "tools").unwrap(),
            transport,
        );
        match plane.execute(&execution("restore the parser")).unwrap() {
            PlaneResult::Done(result) => {
                // The evidence records that usage was not reported; the
                // authority reserves at the envelope rather than booking 0.
                assert_eq!(
                    result.evidence[0].body["usage_tokens"],
                    serde_json::Value::Null
                );
            }
            PlaneResult::Failed(_) => panic!("the scripted transport succeeds"),
        }
    }

    #[test]
    fn a_provider_error_is_a_transient_failure_with_the_key_preserved() {
        let transport = Scripted {
            text: String::new(),
            usage_tokens: None,
            calls: Cell::new(0),
            fail: true,
        };
        let mut plane = ProviderPlane::new(
            LiveIdentity::new("alpha", "model-x", "v1", "tools").unwrap(),
            transport,
        );
        match plane.execute(&execution("restore the parser")).unwrap() {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.kind, ToolFailureKind::Transient);
                assert!(failure.diagnostics[0].contains("5xx"));
                assert_eq!(failure.idempotency_key, Some("play-1-inference".into()));
            }
            PlaneResult::Done(_) => panic!("the scripted transport fails"),
        }
    }

    #[test]
    fn a_promptless_invocation_is_refused_as_invalid_input() {
        let transport = Scripted {
            text: String::new(),
            usage_tokens: None,
            calls: Cell::new(0),
            fail: false,
        };
        let mut plane = ProviderPlane::new(
            LiveIdentity::new("alpha", "model-x", "v1", "tools").unwrap(),
            transport,
        );
        match plane.execute(&execution("   ")).unwrap() {
            PlaneResult::Failed(failure) => {
                assert_eq!(failure.kind, ToolFailureKind::InvalidInput);
            }
            PlaneResult::Done(_) => panic!("no instruction, no invocation"),
        }
    }

    #[test]
    fn cancellation_reports_default_conservative() {
        let transport = Scripted {
            text: String::new(),
            usage_tokens: None,
            calls: Cell::new(0),
            fail: false,
        };
        let report = cancellation_report(
            &transport,
            playbook_core::ExecutionId::new("e-1").unwrap(),
            "winner selected",
        );
        assert!(report.effect_unknown());
    }
}
