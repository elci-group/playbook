//! Deterministic drive reducer.
//!
//! `classify` is a pure function of the previous state, the current state,
//! and the policy. Replaying the event log reproduces the state.

#![forbid(unsafe_code)]

mod apply;
mod authority;
mod classify;
pub mod evaluate;

pub use apply::{apply, export_log, fold, import_log, trajectory, ApplyMode, TrajectoryPoint};
pub use authority::{started_budget, Authority, HuddleAdmission, HuddleRequest};
pub use classify::{classify_measurement, estimate_completion_ppm};
pub use evaluate::{evaluate_branch, EvaluationInputs};

use std::collections::BTreeMap;

use playbook_budget::effective_remaining_ppm;
use playbook_core::{
    sha256_hex, to_canonical_bytes, ArtefactRef, BudgetState, Capability, CapabilityRequirement,
    ContinuationPackage, DecisionRecord, Disposition, Down, DriveBudget, DriveId, DriveStatus,
    Evidence, EvidenceClass, EvidenceId, FailureClass, Gate, GateId, IdempotencyState, Measurement,
    ObjectiveState, Personnel, PersonnelId, PlayExecution, PlayStatus, PlaybookError,
    SideEffectNote, TacticalSnapshot, TaskContract, ToolFailureKind, YTG_REFERENCE_MILLI,
};
use playbook_events::DriveEvent;
use playbook_gates::open_mandatory;
use playbook_lastdown::{seal_package, PackageDraft};
use playbook_policy::Policy;
use playbook_routing::{FieldPosition, HistoricalPriorTable};
use playbook_ytg::ytg_milli;

use serde_json::{json, Value};

/// Durable write-ahead sink for committed drive events.
///
/// The authority records an event with its sink before committing it to the
/// in-memory state, so a crash between dispatch and outcome leaves the
/// invocation recoverable from storage. A sink must make `record` durable
/// before returning. Hosted services share authorities across threads, so a
/// sink must be `Send`.
pub trait EventSink: Send {
    fn record(&mut self, event: &DriveEvent) -> Result<(), PlaybookError>;
    fn mark(&self) -> u64;
    fn rollback(&mut self, mark: u64) -> Result<(), PlaybookError>;
}

/// One EVALUATING branch decision, journalled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluationRecord {
    pub branch: playbook_core::EvaluationBranch,
    pub reasons: Vec<String>,
    pub at_ms: u64,
}

/// One journalled bench substitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubstitutionRecord {
    pub class: String,
    pub out: PersonnelId,
    pub replacement: Option<PersonnelId>,
    pub reasons: Vec<String>,
    pub at_ms: u64,
}

/// One journalled CERS recovery artefact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CersRecord {
    pub artefact_id: EvidenceId,
    pub source: String,
    pub confirmed_facts: u32,
    pub failed_assumptions: u32,
    pub times_reused: u32,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveState {
    pub(crate) drive_id: DriveId,
    pub(crate) task: TaskContract,
    pub(crate) gates: Vec<Gate>,
    pub(crate) capabilities: Vec<Capability>,
    pub(crate) capability_generation: u64,
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) artefacts: Vec<ArtefactRef>,
    pub(crate) ytg_milli: u32,
    pub(crate) origin_ytg_milli: u32,
    pub(crate) cycle_previous_ytg: u32,
    pub(crate) budget_initial: DriveBudget,
    pub(crate) budget_consumed: DriveBudget,
    pub(crate) last_cost: DriveBudget,
    pub(crate) down: Down,
    pub(crate) cycle_failure: bool,
    pub(crate) failure_class: Option<FailureClass>,
    pub(crate) material_failure: bool,
    pub(crate) validated_progress: bool,
    pub(crate) attempts: u32,
    pub(crate) progress_cycles: u32,
    pub(crate) stall_cycles: u32,
    pub(crate) last_operation: Option<String>,
    pub(crate) last_failed_operation: Option<String>,
    pub(crate) last_failure_kind: Option<ToolFailureKind>,
    pub(crate) last_failed_personnel: Option<PersonnelId>,
    pub(crate) failure_fingerprint: Option<String>,
    pub(crate) last_result: Option<PlayStatus>,
    pub(crate) personnel: Option<Personnel>,
    pub(crate) assessments: Vec<Personnel>,
    pub(crate) decisions: Vec<DecisionRecord>,
    pub(crate) idempotency: BTreeMap<String, IdempotencyState>,
    pub(crate) unresolved_effects: Vec<SideEffectNote>,
    pub(crate) open_execution: Option<PlayExecution>,
    pub(crate) field_goal_gate: Option<GateId>,
    pub(crate) status: DriveStatus,
    pub(crate) possession_generation: u32,
    pub(crate) package: Option<ContinuationPackage>,
    pub(crate) disposition: Option<Disposition>,
    pub(crate) requirement: Option<CapabilityRequirement>,
    pub(crate) preflight_complete: bool,
    pub(crate) field_surveyed: bool,
    pub(crate) at_ms: u64,
    pub(crate) explanation: Vec<String>,
    pub(crate) completion_ppm: u32,
    pub(crate) false_completion_claims: u32,
    pub(crate) gate_reopens: u32,
    pub(crate) environment_snapshot: Value,
    pub(crate) failure_refs: Vec<EvidenceId>,
    pub(crate) possession_restarted: bool,
    pub(crate) count_regression: bool,
    pub(crate) missing_evidence: Vec<EvidenceClass>,
    pub(crate) acquisition_count: u32,
    pub(crate) priors: HistoricalPriorTable,
    pub(crate) huddles: playbook_huddle::HuddleLedger,
    pub(crate) blitzes: playbook_blitz::BlitzLedger,
    pub(crate) benched: Vec<PersonnelId>,
    pub(crate) game_contract: Option<playbook_core::GameContract>,
    pub(crate) last_evaluation: Option<EvaluationRecord>,
    pub(crate) substitutions: Vec<SubstitutionRecord>,
    pub(crate) cers_recovered_total: u32,
    pub(crate) cers_artefacts: Vec<CersRecord>,
    pub(crate) directive: Option<playbook_core::TtDirective>,
    pub(crate) tinker_conformances: u32,
    pub(crate) tinker_deviations: u32,
}

impl DriveState {
    pub(crate) fn fresh(
        drive_id: DriveId,
        task: TaskContract,
        budget: DriveBudget,
        at_ms: u64,
    ) -> Self {
        Self {
            drive_id,
            task,
            gates: Vec::new(),
            capabilities: Vec::new(),
            capability_generation: 0,
            evidence: Vec::new(),
            artefacts: Vec::new(),
            ytg_milli: 0,
            origin_ytg_milli: 0,
            cycle_previous_ytg: 0,
            budget_initial: budget,
            budget_consumed: DriveBudget::zero(),
            last_cost: DriveBudget::zero(),
            down: Down::FirstDown,
            cycle_failure: false,
            failure_class: None,
            material_failure: false,
            validated_progress: false,
            attempts: 0,
            progress_cycles: 0,
            stall_cycles: 0,
            last_operation: None,
            last_failed_operation: None,
            last_failure_kind: None,
            last_failed_personnel: None,
            failure_fingerprint: None,
            last_result: None,
            personnel: None,
            assessments: Vec::new(),
            decisions: Vec::new(),
            idempotency: BTreeMap::new(),
            unresolved_effects: Vec::new(),
            open_execution: None,
            field_goal_gate: None,
            status: DriveStatus::Active,
            possession_generation: 1,
            package: None,
            disposition: None,
            requirement: None,
            preflight_complete: false,
            field_surveyed: false,
            at_ms,
            explanation: Vec::new(),
            completion_ppm: 800_000,
            false_completion_claims: 0,
            gate_reopens: 0,
            environment_snapshot: json!({}),
            failure_refs: Vec::new(),
            possession_restarted: false,
            count_regression: false,
            missing_evidence: Vec::new(),
            acquisition_count: 0,
            priors: HistoricalPriorTable::empty(),
            huddles: playbook_huddle::HuddleLedger::empty(),
            blitzes: playbook_blitz::BlitzLedger::empty(),
            benched: Vec::new(),
            game_contract: None,
            last_evaluation: None,
            substitutions: Vec::new(),
            cers_recovered_total: 0,
            cers_artefacts: Vec::new(),
            directive: None,
            tinker_conformances: 0,
            tinker_deviations: 0,
        }
    }

    pub fn drive_id(&self) -> &DriveId {
        &self.drive_id
    }
    /// The capability snapshot generation the drive currently trusts.
    pub fn capability_generation(&self) -> u64 {
        self.capability_generation
    }
    pub fn down(&self) -> Down {
        self.down
    }
    pub fn ytg_milli(&self) -> u32 {
        self.ytg_milli
    }
    pub fn status(&self) -> DriveStatus {
        self.status
    }
    pub fn disposition(&self) -> Option<Disposition> {
        self.disposition
    }
    pub fn completion_ppm(&self) -> u32 {
        self.completion_ppm
    }
    pub fn explanation(&self) -> &[String] {
        &self.explanation
    }
    pub fn package(&self) -> Option<&ContinuationPackage> {
        self.package.as_ref()
    }
    /// Learned historical priors recorded from measured play outcomes.
    pub fn priors(&self) -> &HistoricalPriorTable {
        &self.priors
    }
    /// Evidence ids sealed by failed plays, oldest first.
    pub fn failure_refs(&self) -> &[EvidenceId] {
        &self.failure_refs
    }
    /// Event-sourced huddle ledger: active barriers, closed records,
    /// committed assignments, and substitution recommendations.
    pub fn huddles(&self) -> &playbook_huddle::HuddleLedger {
        &self.huddles
    }
    /// Event-sourced blitz ledger: the defensive formation, its race state,
    /// and closed records with recovery economics.
    pub fn blitzes(&self) -> &playbook_blitz::BlitzLedger {
        &self.blitzes
    }
    /// The sealed game contract, when one governs this drive. DriveCompleted
    /// is refused unless its mandatory win conditions verify.
    pub fn game_contract(&self) -> Option<&playbook_core::GameContract> {
        self.game_contract.as_ref()
    }
    /// The most recent EVALUATING branch decision.
    pub fn last_evaluation(&self) -> Option<&EvaluationRecord> {
        self.last_evaluation.as_ref()
    }
    /// Sealed evidence, oldest first.
    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }
    /// Artefact refs with their validation state.
    pub fn artefacts(&self) -> &[ArtefactRef] {
        &self.artefacts
    }
    /// Journalled bench substitutions, oldest first.
    pub fn substitutions(&self) -> &[SubstitutionRecord] {
        &self.substitutions
    }
    /// Recovered-cognition artefacts journaled on this drive.
    pub fn cers_artefacts(&self) -> &[CersRecord] {
        &self.cers_artefacts
    }
    /// The sealed Thinker→Tinker directive, when one governs execution.
    pub fn directive(&self) -> Option<&playbook_core::TtDirective> {
        self.directive.as_ref()
    }
    /// Measured Thinker→Tinker discipline.
    pub fn tinker_discipline(&self) -> (u32, u32) {
        (self.tinker_conformances, self.tinker_deviations)
    }
    /// Personnel the Bench has benched. They are excluded from the
    /// selection pool until the verdict is reversed.
    pub fn benched(&self) -> &[PersonnelId] {
        &self.benched
    }
    /// The invocation that was dispatched but never committed an outcome.
    /// A recovering host must reconcile it before further execution.
    pub fn open_execution(&self) -> Option<&PlayExecution> {
        self.open_execution.as_ref()
    }
    /// Side effects the plane reported without a committed outcome.
    pub fn unresolved_effects(&self) -> &[SideEffectNote] {
        &self.unresolved_effects
    }
    pub fn false_completion_claims(&self) -> u32 {
        self.false_completion_claims
    }
    pub fn budget_initial(&self) -> DriveBudget {
        self.budget_initial
    }
    pub fn budget_consumed(&self) -> DriveBudget {
        self.budget_consumed
    }
    pub fn personnel(&self) -> Option<&Personnel> {
        self.personnel.as_ref()
    }
    pub fn assessments(&self) -> &[Personnel] {
        &self.assessments
    }
    pub fn failure_class(&self) -> Option<FailureClass> {
        self.failure_class
    }
    pub fn last_failure_kind(&self) -> Option<ToolFailureKind> {
        self.last_failure_kind
    }
    pub fn gates(&self) -> &[Gate] {
        &self.gates
    }
    pub fn task(&self) -> &TaskContract {
        &self.task
    }
    pub fn cycle_previous_ytg(&self) -> u32 {
        self.cycle_previous_ytg
    }
    pub fn requirement(&self) -> Option<&CapabilityRequirement> {
        self.requirement.as_ref()
    }
    pub fn gate_reopens(&self) -> u32 {
        self.gate_reopens
    }
    pub fn progress_cycles(&self) -> u32 {
        self.progress_cycles
    }

    pub fn budget_remaining_ppm(&self, policy: &Policy) -> u32 {
        effective_remaining_ppm(
            self.budget_initial,
            self.budget_consumed,
            policy.budget_weights,
        )
    }

    pub fn fingerprint(&self) -> String {
        let mut evidence: Vec<&str> = self.evidence.iter().map(|item| item.id.as_str()).collect();
        evidence.sort_unstable();
        let mut capabilities: Vec<&str> = self
            .capabilities
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        capabilities.sort_unstable();
        sha256_hex(
            format!(
                "{}|{}|{}",
                self.capability_generation,
                capabilities.join(","),
                evidence.join(",")
            )
            .as_bytes(),
        )
    }

    pub fn state_hash(&self) -> Result<String, PlaybookError> {
        let value = json!({
            "ytg": self.ytg_milli,
            "down": self.down,
            "consumed": self.budget_consumed,
            "generation": self.capability_generation,
            "satisfactions": self.gates.iter().map(|gate| json!({
                "id": gate.id.as_str(),
                "satisfaction_ppm": gate.satisfaction_ppm,
                "state": format!("{:?}", gate.state),
            })).collect::<Vec<_>>(),
        });
        Ok(sha256_hex(&to_canonical_bytes(&value)?))
    }

    pub fn snapshot(&self, policy: &Policy) -> TacticalSnapshot {
        let remaining = self.budget_remaining_ppm(policy);
        TacticalSnapshot {
            drive_id: self.drive_id.to_string(),
            current_down: self.down,
            presentation: self.down.presentation().to_string(),
            current_play: self.last_operation.clone(),
            current_personnel: self.personnel.as_ref().map(|person| person.id.to_string()),
            ytg_milli: self.ytg_milli,
            ytg_delta_milli: self.ytg_milli as i32 - self.cycle_previous_ytg as i32,
            budget_remaining_ppm: remaining,
            budget_consumed: self.budget_consumed,
            mandatory_gates: self
                .gates
                .iter()
                .filter(|gate| gate.mandatory)
                .map(|gate| gate.id.to_string())
                .collect(),
            blocked_gates: self
                .gates
                .iter()
                .filter(|gate| {
                    gate.state == playbook_core::GateState::Blocked
                        || gate.state == playbook_core::GateState::Failed
                })
                .map(|gate| gate.id.to_string())
                .collect(),
            active_capability: self
                .open_execution
                .as_ref()
                .map(|execution| execution.capability_id.to_string())
                .or_else(|| {
                    self.capabilities
                        .first()
                        .map(|capability| capability.id.to_string())
                }),
            last_result: self.last_result,
            completion_probability_ppm: self.completion_ppm,
            failure_class: self.failure_class,
            status: self.status,
            explanation: self.explanation.clone(),
        }
    }

    pub(crate) fn field_position(
        &self,
        roster: &[Personnel],
        policy: &Policy,
    ) -> Result<FieldPosition, PlaybookError> {
        Ok(FieldPosition {
            down: self.down,
            preflight_complete: self.preflight_complete,
            field_surveyed: self.field_surveyed,
            status: self.status,
            fingerprint: self.fingerprint(),
            failure_fingerprint: self.failure_fingerprint.clone(),
            last_failed_operation: self.last_failed_operation.clone(),
            last_failure_kind: self.last_failure_kind,
            last_failed_personnel: self.last_failed_personnel.clone(),
            unresolved_effects: !self.unresolved_effects.is_empty(),
            gates: self.gates.clone(),
            personnel: roster
                .iter()
                .filter(|person| !self.benched.contains(&person.id))
                .cloned()
                .collect(),
            evidence_refs: self.evidence.iter().map(|item| item.id.clone()).collect(),
            state_hash: self.state_hash()?,
            policy_version: policy.version.clone(),
            available_capabilities: self
                .capabilities
                .iter()
                .map(|capability| capability.id.to_string())
                .collect(),
            priors: self.priors.clone(),
        })
    }

    pub(crate) fn refresh_ytg(&mut self) {
        self.ytg_milli = ytg_milli(&self.gates);
        if self.origin_ytg_milli == 0 && self.ytg_milli > 0 {
            self.origin_ytg_milli = self.ytg_milli;
        }
    }

    pub(crate) fn blocking(&self, policy: &Policy) -> (bool, bool) {
        let blocked = self.gates.iter().any(|gate| {
            gate.mandatory
                && !gate.is_terminal()
                && !gate.required_evidence.is_empty()
                && gate.required_evidence.iter().all(|class| {
                    !self
                        .capabilities
                        .iter()
                        .any(|capability| capability.evidence_classes.contains(class))
                })
        });
        let unresolvable = blocked && self.acquisition_count >= policy.capability_acquisition_limit;
        (blocked, unresolvable)
    }
}

impl DriveState {
    pub(crate) fn seal_package(
        &self,
        policy: &Policy,
    ) -> Result<ContinuationPackage, PlaybookError> {
        seal_package(PackageDraft {
            drive_id: self.drive_id.clone(),
            task_contract: self.task.clone(),
            objective_state: ObjectiveState {
                measured_ytg_milli: self.ytg_milli,
                down: self.down,
                possession_generation: self.possession_generation,
                status: self.status,
            },
            validated_artefacts: self
                .artefacts
                .iter()
                .filter(|artefact| artefact.validated)
                .cloned()
                .collect(),
            unsatisfied_gates: self
                .gates
                .iter()
                .filter(|gate| gate.mandatory && !gate.is_terminal())
                .map(|gate| gate.id.clone())
                .collect(),
            evidence_refs: self.evidence.iter().map(|item| item.id.clone()).collect(),
            failure_refs: self.failure_refs.clone(),
            available_capabilities: self
                .capabilities
                .iter()
                .map(|capability| capability.id.clone())
                .collect(),
            budget_state: BudgetState {
                initial: self.budget_initial,
                consumed: self.budget_consumed,
                remaining_fraction_ppm: self.budget_remaining_ppm(policy),
            },
            environment_snapshot: self.environment_snapshot.clone(),
            continuation_constraints: self.task.constraints.clone(),
        })
    }
}

pub(crate) fn measurement_between(
    before: &DriveState,
    after: &DriveState,
    policy: &Policy,
) -> Measurement {
    let (blocking, unresolvable) = after.blocking(policy);
    Measurement {
        validated_progress: after.validated_progress,
        material_failure: after.material_failure,
        cycle_failure: after.cycle_failure,
        failure_class: after.failure_class,
        attempts: after.attempts,
        progress_cycles: after.progress_cycles,
        ytg_milli: after.ytg_milli,
        previous_ytg_milli: before.ytg_milli,
        budget_remaining_ppm: after.budget_remaining_ppm(policy),
        completion_ppm: 0,
        blocking_capability: blocking,
        blocking_unresolvable: unresolvable,
        mandatory_open: open_mandatory(&after.gates).is_some(),
        mandatory_failed: playbook_gates::mandatory_failed(&after.gates),
        previous_down: before.down,
        preflight_complete: after.preflight_complete,
        stall_cycles: after.stall_cycles,
        possession_restarted: after.possession_restarted,
        open_gate: open_mandatory(&after.gates).map(|gate| gate.id.to_string()),
        count_regression: after.count_regression,
        workload_class: after.task.workload_class.clone(),
    }
}

pub fn classify(previous: &DriveState, current: &DriveState, policy: &Policy) -> Down {
    classify_measurement(&measurement_between(previous, current, policy), policy).down
}

pub fn recover(events: &[DriveEvent], policy: &Policy) -> Result<DriveState, PlaybookError> {
    fold(events, policy, ApplyMode::Audit)
}

/// Reference yards, re-exported for callers that print field position.
pub fn reference_milli() -> u32 {
    YTG_REFERENCE_MILLI
}
