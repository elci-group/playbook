//! Immutable drive events.
//!
//! A drive state is a fold of this log. Snapshots are caches.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use playbook_blitz::{
    BlitzCorroboration, BlitzCost, BlitzFailure, BlitzTrigger, Candidate, DefensivePressure,
    HuddleRecommendation, TargetPackage, VerificationKind, WorkerExitKind, WorkerSlot, WorkerSpent,
};
use playbook_core::*;
use playbook_huddle::{
    Contradiction, HuddleCost, HuddleOutcome, HuddleParticipant, HuddleReport, HuddleScope,
    HuddleTrigger,
};

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
    /// A Bench verdict journaled by the tactical authority. Benching a
    /// personnel removes them from the selection pool; only a committed
    /// huddle substitution recommendation authorizes one.
    BenchDecision {
        personnel_id: PersonnelId,
        benched: bool,
        reason_code: String,
    },
    HuddleRequested {
        huddle_id: HuddleId,
        scope: HuddleScope,
        trigger: HuddleTrigger,
        at_ms: u64,
        debt: u32,
        rework_avoided: u32,
    },
    HuddleAdmitted {
        huddle_id: HuddleId,
        budget: HuddleBudget,
        barrier: BarrierMode,
        participants: Vec<HuddleParticipant>,
    },
    HuddleRejected {
        huddle_id: HuddleId,
        reason_code: String,
    },
    HuddleOpened {
        huddle_id: HuddleId,
    },
    HuddleParticipantInvited {
        huddle_id: HuddleId,
        participant: HuddleParticipant,
    },
    HuddleReportSubmitted {
        huddle_id: HuddleId,
        report: Box<HuddleReport>,
    },
    HuddleContradictionDetected {
        huddle_id: HuddleId,
        contradiction: Contradiction,
    },
    HuddleBudgetConsumed {
        huddle_id: HuddleId,
        cost: HuddleCost,
    },
    HuddleOutcomeCommitted {
        huddle_id: HuddleId,
        outcome: Box<HuddleOutcome>,
    },
    HuddleEscalationRequested {
        huddle_id: HuddleId,
        to_scope: HuddleScope,
        reason_code: String,
    },
    HuddleEscalated {
        huddle_id: HuddleId,
        new_huddle_id: HuddleId,
        to_scope: HuddleScope,
    },
    HuddleClosed {
        huddle_id: HuddleId,
        reason_code: String,
    },
    BlitzProposed {
        blitz_id: BlitzId,
        trigger: BlitzTrigger,
        target: TargetPackage,
        pressure: DefensivePressure,
        at_ms: u64,
    },
    BlitzAuthorised {
        blitz_id: BlitzId,
        budget: BlitzBudget,
        corroboration: BlitzCorroboration,
        resource_mode: playbook_blitz::BlitzResourceMode,
        at_ms: u64,
    },
    BlitzRejected {
        blitz_id: BlitzId,
        reason_code: String,
    },
    BlitzTargetFrozen {
        blitz_id: BlitzId,
        target_hash: String,
    },
    BlitzWorkerDispatched {
        blitz_id: BlitzId,
        worker: WorkerSlot,
        effective_parallelism_milli: u32,
    },
    BlitzWorkerStarted {
        blitz_id: BlitzId,
        worker_id: WorkerId,
        at_ms: u64,
    },
    BlitzWorkerExited {
        blitz_id: BlitzId,
        worker_id: WorkerId,
        kind: WorkerExitKind,
        at_ms: u64,
    },
    BlitzWorkerSpent {
        blitz_id: BlitzId,
        spent: WorkerSpent,
    },
    BlitzWaveClosed {
        blitz_id: BlitzId,
        wave: u16,
        diagnostics: Vec<String>,
    },
    BlitzCandidateReceived {
        blitz_id: BlitzId,
        candidate: Box<Candidate>,
    },
    BlitzVerificationStarted {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
    },
    BlitzCandidateRejected {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
        gate: VerificationKind,
        reason_code: String,
    },
    BlitzCandidateVerified {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
        passed: Vec<VerificationKind>,
    },
    BlitzVerificationUnavailable {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
    },
    BlitzHumanConfirmed {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
        authority: String,
    },
    BlitzWinnerSelected {
        blitz_id: BlitzId,
        candidate_id: CandidateId,
        worker_id: WorkerId,
        at_ms: u64,
    },
    BlitzCancelStarted {
        blitz_id: BlitzId,
        at_ms: u64,
    },
    BlitzWorkerCancelled {
        blitz_id: BlitzId,
        worker_id: WorkerId,
        avoided_tokens: u64,
        avoided_cost_microunits: u64,
    },
    BlitzCancelComplete {
        blitz_id: BlitzId,
        cancelled: u16,
        latency_ms: u64,
        avoided_tokens: u64,
        avoided_cost_microunits: u64,
    },
    BlitzRecoveryConfirmed {
        blitz_id: BlitzId,
        evidence_refs: Vec<EvidenceId>,
        at_ms: u64,
    },
    BlitzBudgetConsumed {
        blitz_id: BlitzId,
        cost: BlitzCost,
    },
    BlitzExhausted {
        blitz_id: BlitzId,
        failure: BlitzFailure,
    },
    BlitzEscalated {
        blitz_id: BlitzId,
        recommendation: HuddleRecommendation,
    },
    BlitzSuperseded {
        blitz_id: BlitzId,
        successor_id: BlitzId,
    },
    BlitzFailed {
        blitz_id: BlitzId,
        reason_code: String,
    },
    BlitzAborted {
        blitz_id: BlitzId,
        reason_code: String,
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
            Self::BenchDecision { .. } => "bench_decision",
            Self::HuddleRequested { .. } => "huddle_requested",
            Self::HuddleAdmitted { .. } => "huddle_admitted",
            Self::HuddleRejected { .. } => "huddle_rejected",
            Self::HuddleOpened { .. } => "huddle_opened",
            Self::HuddleParticipantInvited { .. } => "huddle_participant_invited",
            Self::HuddleReportSubmitted { .. } => "huddle_report_submitted",
            Self::HuddleContradictionDetected { .. } => "huddle_contradiction_detected",
            Self::HuddleBudgetConsumed { .. } => "huddle_budget_consumed",
            Self::HuddleOutcomeCommitted { .. } => "huddle_outcome_committed",
            Self::HuddleEscalationRequested { .. } => "huddle_escalation_requested",
            Self::HuddleEscalated { .. } => "huddle_escalated",
            Self::HuddleClosed { .. } => "huddle_closed",
            Self::BlitzProposed { .. } => "blitz_proposed",
            Self::BlitzAuthorised { .. } => "blitz_authorised",
            Self::BlitzRejected { .. } => "blitz_rejected",
            Self::BlitzTargetFrozen { .. } => "blitz_target_frozen",
            Self::BlitzWorkerDispatched { .. } => "blitz_worker_dispatched",
            Self::BlitzWorkerStarted { .. } => "blitz_worker_started",
            Self::BlitzWorkerExited { .. } => "blitz_worker_exited",
            Self::BlitzWorkerSpent { .. } => "blitz_worker_spent",
            Self::BlitzWaveClosed { .. } => "blitz_wave_closed",
            Self::BlitzCandidateReceived { .. } => "blitz_candidate_received",
            Self::BlitzVerificationStarted { .. } => "blitz_verification_started",
            Self::BlitzCandidateRejected { .. } => "blitz_candidate_rejected",
            Self::BlitzCandidateVerified { .. } => "blitz_candidate_verified",
            Self::BlitzVerificationUnavailable { .. } => "blitz_verification_unavailable",
            Self::BlitzHumanConfirmed { .. } => "blitz_human_confirmed",
            Self::BlitzWinnerSelected { .. } => "blitz_winner_selected",
            Self::BlitzCancelStarted { .. } => "blitz_cancel_started",
            Self::BlitzWorkerCancelled { .. } => "blitz_worker_cancelled",
            Self::BlitzCancelComplete { .. } => "blitz_cancel_complete",
            Self::BlitzRecoveryConfirmed { .. } => "blitz_recovery_confirmed",
            Self::BlitzBudgetConsumed { .. } => "blitz_budget_consumed",
            Self::BlitzExhausted { .. } => "blitz_exhausted",
            Self::BlitzEscalated { .. } => "blitz_escalated",
            Self::BlitzSuperseded { .. } => "blitz_superseded",
            Self::BlitzFailed { .. } => "blitz_failed",
            Self::BlitzAborted { .. } => "blitz_aborted",
        }
    }
}
