//! The live execution envelope.
//!
//! Every live invocation receives one envelope, and the envelope is the
//! whole of the invocation's authority: execution identity, the
//! authority-cleared play execution, the personnel playing it, the objective
//! and drive it belongs to, the capabilities and side effects it may touch,
//! the result schema and evidence it must produce, its budget envelope, and
//! its timeout. The executor SHALL NOT infer missing authority: an envelope
//! that fails validation is refused, never completed by guesswork.

use playbook_core::{DriveBudget, EvidenceClass, PlayExecution, PlaybookError, SideEffectClass};
use playbook_security::authorize;

use serde::{Deserialize, Serialize};

/// The complete authority grant for one live invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEnvelope {
    pub execution_id: playbook_core::ExecutionId,
    pub execution: PlayExecution,
    pub personnel_id: playbook_core::PersonnelId,
    pub objective_id: Option<playbook_core::ObjectiveId>,
    /// The operations this invocation may invoke. A capability outside this
    /// set is refused even if the personnel could otherwise perform it.
    pub allowed_operations: Vec<String>,
    /// The side effects this invocation may carry.
    pub allowed_side_effects: Vec<SideEffectClass>,
    /// The schema key the result must satisfy. The executor reports against
    /// it; it does not invent a looser one.
    pub expected_result_schema: String,
    pub expected_evidence: Vec<EvidenceClass>,
    /// What this one invocation may spend. Always a subset of the play's
    /// ceiling: an envelope can narrow authority, never widen it.
    pub budget_envelope: DriveBudget,
    pub timeout_ms: u64,
    /// The directory the executor operates in, when the capability is
    /// repository-bound. Absent means the invocation is location-free.
    pub working_directory: Option<String>,
}

impl ExecutionEnvelope {
    /// Validate the envelope against the drive policy's authority rules.
    /// Every refusal names the missing or exceeding authority; nothing is
    /// defaulted.
    pub fn validate(&self, policy_side_effects: &[SideEffectClass]) -> Result<(), PlaybookError> {
        if self.execution.idempotency_key.is_none() {
            return Err(PlaybookError::Inadmissible(
                self.execution.operation.clone(),
                "live invocation requires an idempotency key".into(),
            ));
        }
        authorize(self.execution.side_effect, policy_side_effects)?;
        if !self
            .allowed_side_effects
            .contains(&self.execution.side_effect)
        {
            return Err(PlaybookError::SideEffect(format!(
                "envelope does not grant {:?}",
                self.execution.side_effect
            )));
        }
        if !self.allowed_operations.contains(&self.execution.operation) {
            return Err(PlaybookError::Inadmissible(
                self.execution.operation.clone(),
                "operation outside the envelope".into(),
            ));
        }
        if !self
            .budget_envelope
            .fits_within(self.execution.budget_ceiling)
        {
            return Err(PlaybookError::BudgetCeiling);
        }
        if self.expected_evidence.is_empty() {
            return Err(PlaybookError::Evidence(
                "live invocation names the evidence it must produce".into(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(PlaybookError::Inadmissible(
                self.execution.operation.clone(),
                "live invocation requires a timeout".into(),
            ));
        }
        if self.working_directory.is_none()
            && self.execution.side_effect != SideEffectClass::ReadOnly
        {
            return Err(PlaybookError::Inadmissible(
                self.execution.operation.clone(),
                "mutating invocation must be bound to a working directory".into(),
            ));
        }
        Ok(())
    }
}

/// What one invocation actually cost, with the unknown made explicit.
/// Unknown execution cost is never zero: the authority reserves capacity for
/// it until the provider reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostReport {
    /// The measured spend. Dimensions the provider did not report stay zero
    /// here and are flagged by the companion flags, never treated as free.
    pub measured: DriveBudget,
    pub tokens_known: bool,
    pub monetary_cost_known: bool,
    pub provider_reported: bool,
}

impl CostReport {
    /// A fully measured report.
    pub fn measured(budget: DriveBudget, provider_reported: bool) -> Self {
        Self {
            measured: budget,
            tokens_known: true,
            monetary_cost_known: true,
            provider_reported,
        }
    }

    /// The spend could not be measured. Wall clock is always knowable to the
    /// executor; everything else is reserved, not zero.
    pub fn partially_unknown(wall_clock_ms: u64, tool_calls: u64) -> Self {
        Self {
            measured: DriveBudget {
                wall_clock_ms,
                tool_calls,
                ..DriveBudget::zero()
            },
            tokens_known: false,
            monetary_cost_known: false,
            provider_reported: false,
        }
    }

    /// The budget the authority must reserve for this report: known
    /// dimensions are taken as reported; unknown dimensions are reserved at
    /// the envelope ceiling, because booking zero would be fiction.
    pub fn reserved(&self, envelope: &ExecutionEnvelope) -> DriveBudget {
        let ceiling = envelope.budget_envelope;
        DriveBudget {
            tokens: if self.tokens_known {
                self.measured.tokens
            } else {
                ceiling.tokens
            },
            monetary_cost_micros: if self.monetary_cost_known {
                self.measured.monetary_cost_micros
            } else {
                ceiling.monetary_cost_micros
            },
            ..self.measured
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{CapabilityId, Down, DriveId, PersonnelId, PlayId, RetryPolicy};

    fn execution(operation: &str, side_effect: SideEffectClass) -> PlayExecution {
        PlayExecution {
            drive_id: DriveId::new("drive-live").unwrap(),
            play_id: PlayId::new("play-1").unwrap(),
            play_kind: playbook_core::PlayKind::RunDeterministicTest,
            down: Down::FirstDown,
            operation: operation.into(),
            capability_id: CapabilityId::new("elci.cargo").unwrap(),
            capability_generation: 1,
            personnel_id: PersonnelId::new("live-local-cargo-v1").unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::CompilerResult],
            expected_ytg_reduction_milli: 4_000,
            survey: false,
            budget_ceiling: DriveBudget {
                tokens: 10_000,
                monetary_cost_micros: 1_000_000,
                wall_clock_ms: 60_000,
                tool_calls: 10,
                inference_calls: 10,
                retry_count: 5,
                context_capacity: 100_000,
                provider_quota: 1_000,
            },
            timeout_ms: 5_000,
            retry_policy: RetryPolicy::NoRetry,
            side_effect,
            idempotency_key: Some("play-1-compile".into()),
        }
    }

    fn envelope(operation: &str, side_effect: SideEffectClass) -> ExecutionEnvelope {
        ExecutionEnvelope {
            execution_id: playbook_core::ExecutionId::new("exec-1").unwrap(),
            execution: execution(operation, side_effect),
            personnel_id: PersonnelId::new("live-local-cargo-v1").unwrap(),
            objective_id: None,
            allowed_operations: vec![operation.to_string()],
            allowed_side_effects: vec![side_effect],
            expected_result_schema: "compiler_result/1".into(),
            expected_evidence: vec![EvidenceClass::CompilerResult],
            budget_envelope: DriveBudget {
                tokens: 5_000,
                monetary_cost_micros: 500_000,
                wall_clock_ms: 30_000,
                tool_calls: 5,
                inference_calls: 5,
                retry_count: 2,
                context_capacity: 50_000,
                provider_quota: 500,
            },
            timeout_ms: 10_000,
            working_directory: Some("/repo".into()),
        }
    }

    #[test]
    fn a_complete_envelope_validates_and_a_missing_field_is_refused() {
        let policy_effects = vec![SideEffectClass::ReadOnly, SideEffectClass::LocalMutation];
        envelope("compile", SideEffectClass::ReadOnly)
            .validate(&policy_effects)
            .unwrap();

        // No idempotency key: the executor would not be able to reconcile a
        // lost response, so the envelope is refused rather than guessed.
        let mut keyless = envelope("compile", SideEffectClass::ReadOnly);
        keyless.execution.idempotency_key = None;
        assert!(keyless.validate(&policy_effects).is_err());

        // An operation outside the envelope is refused even though the play
        // itself would allow it.
        let mut wider = envelope("compile", SideEffectClass::ReadOnly);
        wider.execution.operation = "test.unit".into();
        assert!(wider.validate(&policy_effects).is_err());

        // An envelope cannot widen the play's ceiling.
        let mut greedy = envelope("compile", SideEffectClass::ReadOnly);
        greedy.budget_envelope.tokens = 99_999;
        assert_eq!(
            greedy.validate(&policy_effects).unwrap_err(),
            PlaybookError::BudgetCeiling
        );

        // A mutating invocation must be bound to a working directory.
        let mut roaming = envelope("compile", SideEffectClass::ReadOnly);
        roaming.execution.side_effect = SideEffectClass::LocalMutation;
        roaming.allowed_side_effects = vec![SideEffectClass::LocalMutation];
        roaming.working_directory = None;
        assert!(roaming.validate(&policy_effects).is_err());

        // Side effects outside the drive policy are refused by the
        // security seam, not by the envelope's own say-so.
        let mut dangerous = envelope("compile", SideEffectClass::Deployment);
        dangerous.allowed_side_effects = vec![SideEffectClass::Deployment];
        assert!(dangerous.validate(&policy_effects).is_err());
    }

    #[test]
    fn unknown_cost_is_reserved_at_the_ceiling_never_booked_as_zero() {
        let envelope = envelope("compile", SideEffectClass::ReadOnly);
        let unknown = CostReport::partially_unknown(1_500, 1);
        let reserved = unknown.reserved(&envelope);
        // Unknown tokens are reserved at the envelope ceiling.
        assert_eq!(reserved.tokens, envelope.budget_envelope.tokens);
        // Known dimensions are taken as measured.
        assert_eq!(reserved.wall_clock_ms, 1_500);
        assert_eq!(reserved.tool_calls, 1);

        let known = CostReport::measured(
            DriveBudget {
                tokens: 100,
                monetary_cost_micros: 10_000,
                wall_clock_ms: 2_000,
                tool_calls: 1,
                ..DriveBudget::zero()
            },
            true,
        );
        let reserved = known.reserved(&envelope);
        assert_eq!(reserved.tokens, 100);
        assert_eq!(reserved.monetary_cost_micros, 10_000);
    }
}
