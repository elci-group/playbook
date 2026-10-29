use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{canonical_bytes, sha256_hex};
use crate::error::PlaybookError;
use crate::ids::{ArtefactId, CapabilityId, EvidenceId, GateId, PersonnelId, PlayId};
use crate::units::{accept_schema, Ppm, PPM_SCALE, SCHEMA_MAGIC, SCHEMA_VERSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Down {
    FirstDown,
    SecondDownLoss,
    SecondDownGain,
    ThirdDownRush,
    ThirdDownPass,
    ThirdDownPlayAction,
    FourthDown,
}

impl Down {
    pub fn machine_id(self) -> &'static str {
        match self {
            Self::FirstDown => "FIRST_DOWN",
            Self::SecondDownLoss => "SECOND_DOWN_LOSS",
            Self::SecondDownGain => "SECOND_DOWN_GAIN",
            Self::ThirdDownRush => "THIRD_DOWN_RUSH",
            Self::ThirdDownPass => "THIRD_DOWN_PASS",
            Self::ThirdDownPlayAction => "THIRD_DOWN_PLAY_ACTION",
            Self::FourthDown => "FOURTH_DOWN",
        }
    }

    pub fn presentation(self) -> &'static str {
        match self {
            Self::FirstDown => "1st Down",
            Self::SecondDownLoss => "2nd Down — Negative Yardage",
            Self::SecondDownGain => "2nd Down — Positive Yardage",
            Self::ThirdDownRush => "3rd Down — Rush",
            Self::ThirdDownPass => "3rd Down — Pass",
            Self::ThirdDownPlayAction => "3rd Down — Play Action",
            Self::FourthDown => "4th Down",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    TaskRegression,
    ToolFailure,
    EnvironmentFailure,
    CapabilityAbsence,
    ValidationFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolFailureKind {
    Transient,
    RateLimit,
    Authorization,
    InvalidInput,
    Environment,
    ToolBug,
    Unsupported,
    Timeout,
    Unknown,
}

impl ToolFailureKind {
    pub fn failure_class(self) -> FailureClass {
        match self {
            Self::InvalidInput => FailureClass::ValidationFailure,
            Self::Environment => FailureClass::EnvironmentFailure,
            Self::Unsupported => FailureClass::CapabilityAbsence,
            Self::Transient
            | Self::RateLimit
            | Self::Authorization
            | Self::ToolBug
            | Self::Timeout
            | Self::Unknown => FailureClass::ToolFailure,
        }
    }

    pub fn stops_repetition(self) -> bool {
        matches!(self, Self::Authorization)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    ReadOnly,
    LocalMutation,
    RepositoryMutation,
    RemoteMutation,
    Deployment,
    Destructive,
    Privileged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonnelKind {
    Model,
    Tool,
    Toolchain,
    SpecialistAgent,
    DeterministicProcedure,
    HumanEscalation,
}

impl PersonnelKind {
    /// Lower tiers are asked first. Deterministic capability outranks model prestige.
    pub fn tier(self) -> u8 {
        match self {
            Self::Tool | Self::Toolchain | Self::DeterministicProcedure => 0,
            Self::Model => 1,
            Self::SpecialistAgent => 2,
            Self::HumanEscalation => 3,
        }
    }

    pub fn deterministic(self) -> bool {
        self.tier() == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayKind {
    InspectEnvironment,
    DiscoverRepository,
    DecomposeTask,
    BaselineTest,
    DiscoverCapabilities,
    RunDeterministicTest,
    ExploitProvenTool,
    RouteSpecialist,
    TargetedInference,
    NarrowSearch,
    ParallelValidation,
    ReplaceInferenceWithTool,
    AcquireCapability,
    RetryCorrected,
    SubstitutePersonnel,
    FieldGoalExtract,
    PuntPossession,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvisionStrategy {
    DiscoverExisting,
    Install,
    Build,
    Delegate,
    RouteExternal,
    Defer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    Unassessed,
    Blocked,
    InProgress,
    Satisfied,
    Failed,
    Waived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SatisfactionSource {
    Unset,
    Evidence,
    Waiver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    CompilerResult,
    TestReport,
    ArtefactValidation,
    ToolInvocation,
    Benchmark,
    StaticAnalysis,
    RepositoryMutation,
    GateTransition,
    CapabilitySnapshot,
    EnvironmentSnapshot,
    NarrativeClaim,
}

impl EvidenceClass {
    pub fn can_satisfy_gate(self) -> bool {
        !matches!(
            self,
            Self::NarrativeClaim | Self::GateTransition | Self::CapabilitySnapshot
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Convert,
    FieldGoal,
    Punt,
    Reset,
    Abandon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveStatus {
    Active,
    Punted,
    Completed,
    Abandoned,
    FieldGoalScored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayStatus {
    Succeeded,
    Failed,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskIntegrity {
    Sound,
    Invalid,
    Unsafe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyState {
    Started,
    Committed,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryPolicy {
    NoRetry,
    TransientOnly { limit: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContract {
    pub statement: String,
    pub workload_class: String,
    pub constraints: Vec<Constraint>,
    pub integrity: TaskIntegrity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub authority: String,
    pub reason: String,
}

impl Waiver {
    pub fn new(
        authority: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<Self, PlaybookError> {
        let authority = authority.into();
        let reason = reason.into();
        if authority.trim().is_empty() || reason.trim().is_empty() {
            return Err(PlaybookError::WaiverAuthority);
        }
        Ok(Self { authority, reason })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub id: GateId,
    pub description: String,
    pub weight_ppm: u32,
    pub dependencies: Vec<GateId>,
    pub required_evidence: Vec<EvidenceClass>,
    pub state: GateState,
    pub satisfaction_ppm: u32,
    pub confidence_ppm: u32,
    pub mandatory: bool,
    pub satisfaction_source: SatisfactionSource,
    pub waiver: Option<Waiver>,
    pub satisfying_evidence: Option<EvidenceId>,
}

impl Gate {
    pub fn declare(
        id: GateId,
        description: impl Into<String>,
        weight_ppm: u32,
        dependencies: Vec<GateId>,
        required_evidence: Vec<EvidenceClass>,
        mandatory: bool,
    ) -> Result<Self, PlaybookError> {
        if weight_ppm == 0 || weight_ppm > PPM_SCALE {
            return Err(PlaybookError::Evidence(
                "gate weight must sit in 1..=1000000".into(),
            ));
        }
        Ok(Self {
            id,
            description: description.into(),
            weight_ppm,
            dependencies,
            required_evidence,
            state: GateState::Unassessed,
            satisfaction_ppm: 0,
            confidence_ppm: 0,
            mandatory,
            satisfaction_source: SatisfactionSource::Unset,
            waiver: None,
            satisfying_evidence: None,
        })
    }

    pub fn effective_satisfaction_ppm(&self) -> u32 {
        if self.state == GateState::Waived {
            PPM_SCALE
        } else {
            self.satisfaction_ppm.min(PPM_SCALE)
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.state, GateState::Satisfied | GateState::Waived)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriveBudget {
    pub tokens: u64,
    pub monetary_cost_micros: u64,
    pub wall_clock_ms: u64,
    pub tool_calls: u64,
    pub inference_calls: u64,
    pub retry_count: u64,
    pub context_capacity: u64,
    pub provider_quota: u64,
}

impl DriveBudget {
    pub fn zero() -> Self {
        Self::default()
    }

    pub fn any_positive(self) -> bool {
        self.tokens > 0
            || self.monetary_cost_micros > 0
            || self.wall_clock_ms > 0
            || self.tool_calls > 0
            || self.inference_calls > 0
            || self.retry_count > 0
            || self.context_capacity > 0
            || self.provider_quota > 0
    }

    pub fn checked_add(self, other: Self) -> Result<Self, PlaybookError> {
        Ok(Self {
            tokens: add_dim(self.tokens, other.tokens)?,
            monetary_cost_micros: add_dim(self.monetary_cost_micros, other.monetary_cost_micros)?,
            wall_clock_ms: add_dim(self.wall_clock_ms, other.wall_clock_ms)?,
            tool_calls: add_dim(self.tool_calls, other.tool_calls)?,
            inference_calls: add_dim(self.inference_calls, other.inference_calls)?,
            retry_count: add_dim(self.retry_count, other.retry_count)?,
            context_capacity: add_dim(self.context_capacity, other.context_capacity)?,
            provider_quota: add_dim(self.provider_quota, other.provider_quota)?,
        })
    }

    pub fn fits_within(self, ceiling: Self) -> bool {
        self.tokens <= ceiling.tokens
            && self.monetary_cost_micros <= ceiling.monetary_cost_micros
            && self.wall_clock_ms <= ceiling.wall_clock_ms
            && self.tool_calls <= ceiling.tool_calls
            && self.inference_calls <= ceiling.inference_calls
            && self.retry_count <= ceiling.retry_count
            && self.context_capacity <= ceiling.context_capacity
            && self.provider_quota <= ceiling.provider_quota
    }

    pub fn remaining(self, consumed: Self) -> Self {
        Self {
            tokens: self.tokens.saturating_sub(consumed.tokens),
            monetary_cost_micros: self
                .monetary_cost_micros
                .saturating_sub(consumed.monetary_cost_micros),
            wall_clock_ms: self.wall_clock_ms.saturating_sub(consumed.wall_clock_ms),
            tool_calls: self.tool_calls.saturating_sub(consumed.tool_calls),
            inference_calls: self
                .inference_calls
                .saturating_sub(consumed.inference_calls),
            retry_count: self.retry_count.saturating_sub(consumed.retry_count),
            context_capacity: self
                .context_capacity
                .saturating_sub(consumed.context_capacity),
            provider_quota: self.provider_quota.saturating_sub(consumed.provider_quota),
        }
    }
}

fn add_dim(left: u64, right: u64) -> Result<u64, PlaybookError> {
    left.checked_add(right)
        .ok_or(PlaybookError::BudgetExhausted)
}

/// Hard envelope for one huddle synchronization. Every dimension is a
/// ceiling; exhausting any of them terminates discussion and forces
/// synthesis from the accumulated reports. A huddle may never extend itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleBudget {
    pub max_tokens: u64,
    pub max_cost_microunits: u64,
    pub max_duration_ms: u64,
    pub max_rounds: u16,
    pub max_participants: u16,
}

impl HuddleBudget {
    pub fn zero() -> Self {
        Self::default()
    }

    pub fn fits(&self, tokens: u64, cost_microunits: u64, duration_ms: u64, rounds: u16) -> bool {
        tokens <= self.max_tokens
            && cost_microunits <= self.max_cost_microunits
            && duration_ms <= self.max_duration_ms
            && rounds <= self.max_rounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarrierMode {
    Soft,
    Scope,
    Hard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetWeights {
    pub tokens: u32,
    pub monetary_cost_micros: u32,
    pub wall_clock_ms: u32,
    pub tool_calls: u32,
    pub inference_calls: u32,
    pub retry_count: u32,
    pub context_capacity: u32,
    pub provider_quota: u32,
}

impl BudgetWeights {
    pub fn pairs(self, initial: DriveBudget, remaining: DriveBudget) -> Vec<(u32, u64, u64)> {
        vec![
            (self.tokens, initial.tokens, remaining.tokens),
            (
                self.monetary_cost_micros,
                initial.monetary_cost_micros,
                remaining.monetary_cost_micros,
            ),
            (
                self.wall_clock_ms,
                initial.wall_clock_ms,
                remaining.wall_clock_ms,
            ),
            (self.tool_calls, initial.tool_calls, remaining.tool_calls),
            (
                self.inference_calls,
                initial.inference_calls,
                remaining.inference_calls,
            ),
            (self.retry_count, initial.retry_count, remaining.retry_count),
            (
                self.context_capacity,
                initial.context_capacity,
                remaining.context_capacity,
            ),
            (
                self.provider_quota,
                initial.provider_quota,
                remaining.provider_quota,
            ),
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: EvidenceId,
    pub class: EvidenceClass,
    pub body: Value,
    pub observed_at_ms: u64,
}

impl Evidence {
    pub fn seal(
        class: EvidenceClass,
        body: Value,
        observed_at_ms: u64,
    ) -> Result<Self, PlaybookError> {
        if !body.is_object() {
            return Err(PlaybookError::Evidence(
                "evidence body must be an object".into(),
            ));
        }
        let fingerprint = serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "class": class,
            "body": body,
            "observed_at_ms": observed_at_ms,
        });
        let bytes = canonical_bytes(&fingerprint)?;
        let id = EvidenceId::new(sha256_hex(&bytes))?;
        Ok(Self {
            id,
            class,
            body,
            observed_at_ms,
        })
    }

    pub fn verify(&self) -> Result<(), PlaybookError> {
        let sealed = Self::seal(self.class, self.body.clone(), self.observed_at_ms)?;
        if sealed.id != self.id {
            Err(PlaybookError::DigestMismatch)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub id: CapabilityId,
    pub version: String,
    pub provider: String,
    pub operations: Vec<String>,
    pub inputs_schema: Value,
    pub outputs_schema: Value,
    pub cost_model: DriveBudget,
    pub side_effect_class: SideEffectClass,
    pub permissions: Vec<String>,
    pub environment_requirements: Vec<String>,
    pub reliability_ppm: u32,
    pub reliability_samples: u32,
    pub evidence_classes: Vec<EvidenceClass>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personnel {
    pub id: PersonnelId,
    pub kind: PersonnelKind,
    pub display_name: String,
    pub cost_tokens: u64,
    pub latency_ms: u64,
    pub capability_fit_ppm: u32,
    pub reliability_ppm: u32,
    pub quota_remaining_ppm: u32,
    pub evidence_quality_ppm: u32,
    pub own_ytg_milli: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub capability_id: String,
    pub capability_class: String,
    pub reason: String,
    pub blocks_gate: GateId,
    pub provision_strategy: ProvisionStrategy,
    pub estimated_cost: DriveBudget,
    pub reusable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayDefinition {
    pub kind: PlayKind,
    pub name: String,
    pub personnel_kind: PersonnelKind,
    pub operation: String,
    pub capability_id: String,
    pub target_gate: Option<String>,
    pub side_effect: SideEffectClass,
    pub expected_ytg_reduction_milli: u32,
    pub marginal_cost: DriveBudget,
    pub reliability_ppm: u32,
    pub survey: bool,
    pub evidence_class: EvidenceClass,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRecord {
    pub policy_version: String,
    pub input_state_hash: String,
    pub classification: Down,
    pub selected_play: PlayKind,
    pub selected_operation: String,
    pub selected_personnel: Option<PersonnelId>,
    pub candidate_plays: Vec<String>,
    pub rejection_reasons: Vec<String>,
    pub evidence_refs: Vec<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideEffectNote {
    pub class: SideEffectClass,
    pub idempotency_key: String,
    pub summary: String,
    pub resolved: bool,
    pub occurred: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayExecution {
    pub drive_id: crate::ids::DriveId,
    pub play_id: PlayId,
    pub play_kind: PlayKind,
    pub operation: String,
    pub down: Down,
    pub capability_id: CapabilityId,
    pub capability_generation: u64,
    pub personnel_id: PersonnelId,
    pub inputs: Value,
    pub expected_evidence: Vec<EvidenceClass>,
    pub expected_ytg_reduction_milli: u32,
    pub survey: bool,
    pub budget_ceiling: DriveBudget,
    pub timeout_ms: u64,
    pub retry_policy: RetryPolicy,
    pub side_effect: SideEffectClass,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayResult {
    pub status: PlayStatus,
    pub play_id: PlayId,
    pub operation: String,
    pub output_refs: Vec<String>,
    pub evidence: Vec<Evidence>,
    pub artefact_refs: Vec<ArtefactId>,
    pub cost: DriveBudget,
    pub duration_ms: u64,
    pub diagnostics: Vec<String>,
    pub side_effects: Vec<SideEffectNote>,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolFailure {
    pub play_id: PlayId,
    pub play_kind: PlayKind,
    pub operation: String,
    pub capability_id: CapabilityId,
    pub personnel_id: PersonnelId,
    pub kind: ToolFailureKind,
    pub diagnostics: Vec<String>,
    pub cost: DriveBudget,
    pub at_ms: u64,
    pub idempotency_key: Option<String>,
    pub side_effects: Vec<SideEffectNote>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtefactRef {
    pub id: ArtefactId,
    pub digest: String,
    pub validated: bool,
    pub rejected: bool,
    pub evidence_id: Option<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveState {
    pub measured_ytg_milli: u32,
    pub down: Down,
    pub possession_generation: u32,
    pub status: DriveStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetState {
    pub initial: DriveBudget,
    pub consumed: DriveBudget,
    pub remaining_fraction_ppm: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPackage {
    pub magic: String,
    pub schema_version: u32,
    pub drive_id: crate::ids::DriveId,
    pub task_contract: TaskContract,
    pub objective_state: ObjectiveState,
    pub validated_artefacts: Vec<ArtefactRef>,
    pub unsatisfied_gates: Vec<GateId>,
    pub evidence_refs: Vec<EvidenceId>,
    pub failure_refs: Vec<EvidenceId>,
    pub available_capabilities: Vec<CapabilityId>,
    pub budget_state: BudgetState,
    pub environment_snapshot: Value,
    pub continuation_constraints: Vec<Constraint>,
    pub integrity_sha256: String,
}

impl ContinuationPackage {
    pub fn seal_unchecked(mut self) -> Result<Self, PlaybookError> {
        accept_schema(self.schema_version)?;
        if self.magic != SCHEMA_MAGIC {
            return Err(PlaybookError::Schema(self.schema_version));
        }
        self.integrity_sha256.clear();
        let bytes = crate::canonical::to_canonical_bytes(&self)?;
        if leaks_competitive_metadata(&bytes) {
            return Err(PlaybookError::AmnesiaLeak);
        }
        self.integrity_sha256 = sha256_hex(&bytes);
        Ok(self)
    }
}

pub fn forbidden_competitive_keys() -> &'static [&'static str] {
    &[
        "previous_model_identity",
        "previous_model_self_assessed_confidence",
        "previous_model_ytg_prediction",
        "other_candidate_model_predictions",
        "competitive_ranking",
        "previous_model",
        "previous_ytg",
        "nominal_ytg",
        "competitor_assessments",
        "route_rank",
        "selection_history",
        "coordinator_notes",
        "model_reputation",
        "conversation",
        "own_ytg_milli",
    ]
}

pub fn leaks_competitive_metadata(bytes: &[u8]) -> bool {
    let text = String::from_utf8_lossy(bytes);
    forbidden_competitive_keys()
        .iter()
        .any(|key| text.contains(key))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwarmOrder {
    pub objective: String,
    pub maximum_workers: u32,
    pub worker_roles: Vec<String>,
    pub budget_envelope: DriveBudget,
    pub evidence_requirements: Vec<EvidenceClass>,
    pub termination: String,
    pub merge_strategy: String,
    pub expected_marginal_ytg_milli: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measurement {
    pub validated_progress: bool,
    pub material_failure: bool,
    pub cycle_failure: bool,
    pub failure_class: Option<FailureClass>,
    pub attempts: u32,
    pub progress_cycles: u32,
    pub ytg_milli: u32,
    pub previous_ytg_milli: u32,
    pub budget_remaining_ppm: u32,
    pub completion_ppm: u32,
    pub blocking_capability: bool,
    pub blocking_unresolvable: bool,
    pub mandatory_open: bool,
    pub mandatory_failed: bool,
    pub previous_down: Down,
    pub preflight_complete: bool,
    pub stall_cycles: u32,
    pub possession_restarted: bool,
    pub open_gate: Option<String>,
    pub count_regression: bool,
    pub workload_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub down: Down,
    pub reasons: Vec<String>,
    pub completion_ppm: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalSnapshot {
    pub drive_id: String,
    pub current_down: Down,
    pub presentation: String,
    pub current_play: Option<String>,
    pub current_personnel: Option<String>,
    pub ytg_milli: u32,
    pub ytg_delta_milli: i32,
    pub budget_remaining_ppm: u32,
    pub budget_consumed: DriveBudget,
    pub mandatory_gates: Vec<String>,
    pub blocked_gates: Vec<String>,
    pub active_capability: Option<String>,
    pub last_result: Option<PlayStatus>,
    pub completion_probability_ppm: u32,
    pub failure_class: Option<FailureClass>,
    pub status: DriveStatus,
    pub explanation: Vec<String>,
}

/// Keys a model must not be able to mark complete by assertion.
pub fn narrative_has_no_weight(class: EvidenceClass) -> bool {
    matches!(class, EvidenceClass::NarrativeClaim)
}

pub fn check_ppm(value: u32) -> Result<Ppm, PlaybookError> {
    Ppm::new(value)
}
