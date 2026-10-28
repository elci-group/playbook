//! Immutable drive events.
//!
//! A drive state is a fold of this log. Snapshots are caches.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use playbook_core::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DriveEvent {
    DriveStarted {
        drive_id: DriveId,
        task: TaskContract,
        budget: DriveBudget,
        at_ms: u64,
    },
    GateDeclared {
        gate: Gate,
    },
    CapabilityObserved {
        capability: Capability,
        generation: u64,
    },
    PreflightCompleted {
        missing: Vec<EvidenceClass>,
        generation: u64,
    },
    PlaySelected {
        decision: Box<DecisionRecord>,
    },
    PersonnelChanged {
        personnel: Personnel,
    },
    ToolInvoked {
        execution: PlayExecution,
    },
    ToolSucceeded {
        result: Box<PlayResult>,
    },
    ToolFailed {
        failure: ToolFailure,
    },
    /// A plane returned an outcome that could not be committed, or its
    /// response was lost after invocation. Preserve the observation for audit
    /// and require reconciliation before another execution.
    ToolOutcomeRejected {
        result: Option<Box<PlayResult>>,
        failure: Option<ToolFailure>,
        reason: String,
    },
    EvidenceAccepted {
        evidence: Evidence,
    },
    ArtefactProduced {
        artefact: ArtefactRef,
    },
    ArtefactValidated {
        artefact_id: ArtefactId,
        evidence_id: EvidenceId,
    },
    ArtefactRejected {
        artefact_id: ArtefactId,
        evidence_id: EvidenceId,
    },
    BudgetConsumed {
        cost: DriveBudget,
        at_ms: u64,
    },
    WaiverGranted {
        gate_id: GateId,
        authority: String,
        reason: String,
    },
    YtgRecalculated {
        ytg_milli: u32,
        previous_milli: u32,
    },
    DownChanged {
        down: Down,
        reasons: Vec<String>,
    },
    FourthDownDeclared {
        probability_ppm: u32,
        threshold_ppm: u32,
        disposition: Disposition,
    },
    CapabilityRequirementIssued {
        requirement: CapabilityRequirement,
    },
    FieldGoalSelected {
        gate_id: GateId,
    },
    PuntIssued {
        package: Box<ContinuationPackage>,
    },
    PossessionStarted {
        personnel: Personnel,
        budget: DriveBudget,
        at_ms: u64,
    },
    AssessmentsRecorded {
        personnel: Vec<Personnel>,
    },
    PriorsLoaded {
        priors: HistoricalPriorTable,
    },
    SideEffectReconciled {
        key: String,
        occurred: bool,
    },
    CompletionRejected {
        at_ms: u64,
        reason: String,
    },
    DriveCompleted {
        at_ms: u64,
    },
    AbandonIssued {
        reason: String,
    },
}

impl DriveEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::DriveStarted { .. } => "drive_started",
            Self::GateDeclared { .. } => "gate_declared",
            Self::CapabilityObserved { .. } => "capability_observed",
            Self::PreflightCompleted { .. } => "preflight_completed",
            Self::PlaySelected { .. } => "play_selected",
            Self::PersonnelChanged { .. } => "personnel_changed",
            Self::ToolInvoked { .. } => "tool_invoked",
            Self::ToolSucceeded { .. } => "tool_succeeded",
            Self::ToolFailed { .. } => "tool_failed",
            Self::ToolOutcomeRejected { .. } => "tool_outcome_rejected",
            Self::EvidenceAccepted { .. } => "evidence_accepted",
            Self::ArtefactProduced { .. } => "artefact_produced",
            Self::ArtefactValidated { .. } => "artefact_validated",
            Self::ArtefactRejected { .. } => "artefact_rejected",
            Self::BudgetConsumed { .. } => "budget_consumed",
            Self::WaiverGranted { .. } => "waiver_granted",
            Self::YtgRecalculated { .. } => "ytg_recalculated",
            Self::DownChanged { .. } => "down_changed",
            Self::FourthDownDeclared { .. } => "fourth_down_declared",
            Self::CapabilityRequirementIssued { .. } => "capability_requirement_issued",
            Self::FieldGoalSelected { .. } => "field_goal_selected",
            Self::PuntIssued { .. } => "punt_issued",
            Self::PossessionStarted { .. } => "possession_started",
            Self::AssessmentsRecorded { .. } => "assessments_recorded",
            Self::PriorsLoaded { .. } => "priors_loaded",
            Self::SideEffectReconciled { .. } => "side_effect_reconciled",
            Self::CompletionRejected { .. } => "completion_rejected",
            Self::DriveCompleted { .. } => "drive_completed",
            Self::AbandonIssued { .. } => "abandon_issued",
        }
    }
}
