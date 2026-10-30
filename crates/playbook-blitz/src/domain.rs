//! Blitz domain model.
//!
//! A blitz is a bounded, budget-governed, speculative execution event in
//! which multiple independently operating inference workers receive
//! materially equivalent problem definitions and race to produce the
//! earliest independently verifiable solution to a single defensive target.
//! Same problem × multiple independent solvers × race to verified recovery.
//! Natural-language urgency is an implementation detail; every type here is
//! the authority.
//!
//! A blitz is not generic parallelism, a swarm, a huddle, voting, an
//! ensemble, a retry, or a model fallback. It exists to purchase verified
//! recovery latency with redundant inference under material defensive
//! pressure, and to stop spending the moment the defensive objective is met.

use serde::{Deserialize, Serialize};

use playbook_core::{
    canonical_bytes, sha256_hex, to_canonical_bytes, BlitzBudget, BlitzId, CandidateId, EventId,
    EvidenceClass, EvidenceId, PersonnelId, PlaybookError, WorkerId, SCHEMA_VERSION,
};

use playbook_policy::BlitzCorroboration;

/// How bad the defensive event is. Drives instinctive eligibility and the
/// pressure score, never roster prestige.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Per-mille weight used by the pressure score.
    pub #[tracing::instrument]
fn milli(self) -> u32 {
        match self {
            Self::Low => 100,
            Self::Medium => 400,
            Self::High => 700,
            Self::Critical => 1_000,
        }
    }
}

/// How the blitz was invoked. An instinctive blitz is deterministically
/// triggered by a predefined condition; a strategic blitz is an inference
/// recommendation that must still pass deterministic policy authorisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlitzClass {
    Instinctive,
    Strategic,
}

/// What is scarce. The mode selects the envelope and biases formation
/// preference; it never relaxes verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlitzResourceMode {
    Normal,
    LatencyCritical,
    PowerCritical,
    NetworkCritical,
    CostCritical,
}

impl BlitzResourceMode {
    /// The policy envelope key for this mode.
    pub #[tracing::instrument]
fn key(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::LatencyCritical => "latency_critical",
            Self::PowerCritical => "power_critical",
            Self::NetworkCritical => "network_critical",
            Self::CostCritical => "cost_critical",
        }
    }

    /// Formation bias. Under `POWER_CRITICAL` local compute is expensive, so
    /// remote workers are preferred; under `NETWORK_CRITICAL` the preference
    /// inverts. Any other mode is indifferent.
    pub #[tracing::instrument]
fn prefers_remote(self) -> Option<bool> {
        match self {
            Self::PowerCritical => Some(true),
            Self::NetworkCritical => Some(false),
            _ => None,
        }
    }
}

/// Why the blitz was called. The evidence bundle is frozen with the target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzTrigger {
    pub class: BlitzClass,
    pub reason_code: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub originating_event: EventId,
}

/// The measured defensive situation. Every component is a deterministic
/// integer: per-mille rates and confidences, utility units for loss. The
/// representation may evolve; the contract — distinguishing difficult from
/// urgent — may not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefensivePressure {
    pub severity: Severity,
    /// Milliseconds until the defensive objective is lost. `None` means the
    /// deadline is not the binding constraint.
    pub deadline_ms: Option<u64>,
    /// Per-mille per second deterioration of the failing component.
    pub deterioration_rate_ppm: u32,
    /// Per-mille blast radius: how much of the system the failure touches.
    pub blast_radius_ppm: u32,
    /// Per-mille uncertainty about which available solver can recover.
    pub solver_uncertainty_ppm: u32,
    /// Per-mille confidence that the failure is currently contained.
    pub containment_confidence_ppm: u32,
    /// Per-mille confidence that a candidate will actually recover.
    pub recovery_confidence_ppm: u32,
    /// Per-mille per second decay of the remaining resources.
    pub resource_decay_ppm: u32,
    /// Utility lost per second of delay, in the drive's utility units.
    pub expected_delay_loss: u64,
}

/// One gate in the verification hierarchy. Static evidence outranks model
/// adjudication; adjudication outranks human review. Gates marked
/// non-deterministic never constitute sufficient evidence on their own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationKind {
    StaticInvariant,
    Compilation,
    UnitTest,
    RegressionTest,
    SecurityTest,
    IntegrationTest,
    Benchmark,
    RuntimeHealth,
    IndependentAdjudication,
    HumanReview,
}

impl VerificationKind {
    /// Verification preference ordering: deterministic evidence first.
    pub #[tracing::instrument]
fn rank(self) -> u8 {
        match self {
            Self::StaticInvariant => 0,
            Self::Compilation => 1,
            Self::UnitTest => 2,
            Self::RegressionTest => 3,
            Self::SecurityTest => 4,
            Self::IntegrationTest => 5,
            Self::Benchmark => 6,
            Self::RuntimeHealth => 7,
            Self::IndependentAdjudication => 8,
            Self::HumanReview => 9,
        }
    }

    /// Generative self-certification is not deterministic evidence. Only
    /// adjudication and human review are generative, and neither may satisfy
    /// a critical recovery alone.
    pub #[tracing::instrument]
fn deterministic(self) -> bool {
        !matches!(self, Self::IndependentAdjudication | Self::HumanReview)
    }

    /// The evidence class a passing gate is sealed as.
    pub #[tracing::instrument]
fn evidence_class(self) -> EvidenceClass {
        match self {
            Self::StaticInvariant | Self::SecurityTest => EvidenceClass::StaticAnalysis,
            Self::Compilation => EvidenceClass::CompilerResult,
            Self::UnitTest | Self::RegressionTest | Self::IntegrationTest => {
                EvidenceClass::TestReport
            }
            Self::Benchmark => EvidenceClass::Benchmark,
            Self::RuntimeHealth => EvidenceClass::ToolInvocation,
            Self::IndependentAdjudication => EvidenceClass::ArtefactValidation,
            Self::HumanReview => EvidenceClass::NarrativeClaim,
        }
    }
}

/// The frozen contract every worker operates against. Sealed
/// content-addressably: the hash proves the workers were independently
/// attempting the same problem. `target_hash` is computed by [`TargetPackage::seal`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetPackage {
    pub problem_statement: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub artefact_refs: Vec<String>,
    pub constraints: Vec<String>,
    pub forbidden_actions: Vec<String>,
    pub expected_output: String,
    pub win_condition: String,
    pub verification_gates: Vec<VerificationKind>,
    pub deadline_ms: u64,
    pub target_hash: String,
}

impl TargetPackage {
    /// Freeze the target: compute and seal its content hash. The package is
    /// immutable once execution begins except through explicitly logged
    /// supersession.
    pub #[tracing::instrument]
fn seal(mut self) -> Result<Self, PlaybookError> {
        if self.problem_statement.trim().is_empty() || self.win_condition.trim().is_empty() {
            return Err(PlaybookError::Evidence(
                "target requires a problem statement and a win condition".into(),
            ));
        }
        if self.verification_gates.is_empty() {
            return Err(PlaybookError::Evidence(
                "target requires at least one verification gate".into(),
            ));
        }
        self.verification_gates.sort_by_key(|gate| gate.rank());
        self.target_hash.clear();
        let bytes = to_canonical_bytes(&self)?;
        self.target_hash = sha256_hex(&bytes);
        Ok(self)
    }

    /// The package is well-formed and the sealed digest matches the
    /// contents. The fold refuses to open a race against a malformed or
    /// tampered target.
    pub #[tracing::instrument]
fn verify(&self) -> Result<(), PlaybookError> {
        if self.problem_statement.trim().is_empty() || self.win_condition.trim().is_empty() {
            return Err(PlaybookError::Evidence(
                "target requires a problem statement and a win condition".into(),
            ));
        }
        if self.verification_gates.is_empty() {
            return Err(PlaybookError::Evidence(
                "target requires at least one verification gate".into(),
            ));
        }
        let mut unsealed = self.clone();
        unsealed.target_hash.clear();
        let sealed = sha256_hex(&to_canonical_bytes(&unsealed)?);
        if sealed != self.target_hash {
            return Err(PlaybookError::DigestMismatch);
        }
        Ok(())
    }

    /// The digest a candidate artefact must attempt. Present for fold-time
    /// single-focus checks; workers receive the whole package.
    pub #[tracing::instrument]
fn hash(&self) -> &str {
        &self.target_hash
    }
}

/// One dispatched worker: who plays, for which wave, under which deadline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSlot {
    pub worker_id: WorkerId,
    pub personnel_id: PersonnelId,
    pub provider: String,
    pub model_family: String,
    /// Shared upstream failure domains, sorted and deduplicated. Independence
    /// is computed from these, never from brand names.
    pub failure_domains: Vec<String>,
    pub wave: u16,
    pub dispatched_at_ms: u64,
    pub deadline_ms: u64,
}

/// Why an individual worker left the race without a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerExitKind {
    ProviderFailure,
    ToolFailure,
    Timeout,
}

/// A candidate artefact emitted mid-race. Arrival immediately makes it
/// eligible for verification; a worker identifying a plausible patch at
/// second 4 must not spend another 30 seconds writing an essay first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub candidate_id: CandidateId,
    pub blitz_id: BlitzId,
    pub worker_id: WorkerId,
    /// The target hash the worker was actually attempting. The fold refuses
    /// candidates attributable to any other problem.
    pub target_hash: String,
    pub artefact_digest: String,
    pub claim: String,
    pub confidence_ppm: u32,
    pub evidence_refs: Vec<EvidenceId>,
    pub verification_hint: String,
    pub emitted_at_ms: u64,
    pub wave: u16,
}

/// One candidate's verification trail. A candidate is untrusted until a
/// required gate set passes; an unavailable verifier leaves the candidate
/// untrusted (fail closed) until a policy-bounded retry restarts it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRecord {
    pub candidate_id: CandidateId,
    pub worker_id: WorkerId,
    pub passed: Vec<VerificationKind>,
    pub failed: Option<(VerificationKind, String)>,
    pub verifier_unavailable: bool,
    /// Verification starts attempted for this candidate, including the one
    /// that produced this record. The fold bounds restarts with the policy
    /// retry ceiling.
    pub attempts: u16,
}

/// The consumption delta of one race step. Every dimension is monotonically
/// increasing; the governor refuses any dimension that would pass its
/// ceiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzCost {
    pub tokens: u64,
    pub cost_microunits: u64,
    pub duration_ms: u64,
    pub workers: u16,
    pub waves: u16,
}

impl BlitzCost {
    pub #[tracing::instrument]
fn zero() -> Self {
        Self::default()
    }
}

/// The authoritative winner. Exactly one exists per recovered blitz; later
/// candidates are rejected, never double-committed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedWinner {
    pub candidate_id: CandidateId,
    pub worker_id: WorkerId,
    pub at_ms: u64,
    pub human_confirmed: bool,
}

/// One worker cancelled after the win condition was satisfied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelledWorker {
    pub worker_id: WorkerId,
    pub avoided_tokens: u64,
    pub avoided_cost_microunits: u64,
}

/// The cancellation summary recorded when redundant work has stopped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelSummary {
    pub cancelled: u16,
    pub latency_ms: u64,
    pub avoided_tokens: u64,
    pub avoided_cost_microunits: u64,
}

/// What one worker contributed, as scouting evidence. This is how the blitz
/// becomes progressively better at deciding who is worth sending.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerScouting {
    pub personnel_id: PersonnelId,
    pub provider: String,
    pub model_family: String,
    pub wave: u16,
    pub produced_candidate: bool,
    pub candidate_verified: bool,
    pub tokens: u64,
    pub cost_microunits: u64,
    pub latency_ms: u64,
}

/// The closed outcome of one blitz. Recovery metrics, economics, and the
/// scouting evidence all derive from this record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzOutcome {
    pub target_hash: String,
    pub winner: Option<SelectedWinner>,
    pub first_candidate_ms: Option<u64>,
    pub time_to_verified_ms: Option<u64>,
    pub time_to_recovery_ms: Option<u64>,
    pub raw_parallelism: u16,
    pub effective_parallelism_milli: u32,
    pub correlated_failures: u32,
    pub tokens_to_recovery: u64,
    pub cost_to_recovery_microunits: u64,
    pub cancellation: Option<CancelSummary>,
    pub worker_results: Vec<WorkerScouting>,
    pub failure: Option<BlitzFailure>,
}

/// Failure taxonomy. Different classes require different remediation, so
/// they are never collapsed into a generic failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlitzFailure {
    NoCandidate,
    InvalidCandidates,
    VerificationFailure,
    DeadlineExhausted,
    BudgetExhausted,
    ProviderFailure,
    ToolFailure,
    TargetAmbiguity,
    TargetDrift,
    CorrelatedFailure,
    CancellationFailure,
    RecoveryRegression,
    PolicyRejection,
}

/// How a blitz leaves the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlitzClosure {
    Rejected,
    Recovered,
    Exhausted,
    Escalated,
    Superseded,
    Failed,
    Aborted,
}

/// The live state machine. `PROPOSED → AUTHORISED → FREEZING_TARGET →
/// DISPATCHING → RACING → VERIFYING → RECOVERED → CANCELLING`, with
/// `EXHAUSTED`, `ESCALATING`, `FAILED`, and `ABORTED` as the defensive
/// exits. Every transition is fold-validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlitzPhase {
    Proposed,
    Authorised,
    FreezingTarget,
    Dispatching,
    Racing,
    Verifying,
    Recovered,
    Cancelling,
    Exhausted,
    Escalating,
    Failed,
    Aborted,
}

/// A live blitz. The ledger holds at most one: all workers target the same
/// defined objective, and a second formation would silently dilute focus.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveBlitz {
    pub id: BlitzId,
    pub class: BlitzClass,
    pub phase: BlitzPhase,
    pub trigger: BlitzTrigger,
    pub target: TargetPackage,
    pub pressure: DefensivePressure,
    pub budget: BlitzBudget,
    pub spent: BlitzCost,
    pub corroboration: BlitzCorroboration,
    pub resource_mode: BlitzResourceMode,
    pub workers: Vec<WorkerSlot>,
    pub closed_waves: Vec<u16>,
    /// Workers that left the race without ever emitting a candidate.
    pub exited: Vec<(WorkerId, WorkerExitKind)>,
    pub candidates: Vec<Candidate>,
    pub verifications: Vec<VerificationRecord>,
    pub human_confirmations: Vec<(CandidateId, String)>,
    pub winner: Option<SelectedWinner>,
    /// Per-worker consumption attributed as workers leave the race.
    pub worker_spend: Vec<WorkerSpent>,
    pub cancellations: Vec<CancelledWorker>,
    pub cancel_started_at: Option<u64>,
    pub cancel_complete: Option<CancelSummary>,
    pub failure: Option<BlitzFailure>,
    pub correlated_failures: u32,
    pub effective_parallelism_milli: u32,
    pub at_ms: u64,
    pub deadline_at_ms: u64,
}

impl ActiveBlitz {
    pub #[tracing::instrument]
fn worker(&self, worker_id: &WorkerId) -> Option<&WorkerSlot> {
        self.workers
            .iter()
            .find(|slot| &slot.worker_id == worker_id)
    }

    pub #[tracing::instrument]
fn candidate(&self, candidate_id: &CandidateId) -> Option<&Candidate> {
        self.candidates
            .iter()
            .find(|candidate| &candidate.candidate_id == candidate_id)
    }

    pub #[tracing::instrument]
fn verification(&self, candidate_id: &CandidateId) -> Option<&VerificationRecord> {
        self.verifications
            .iter()
            .find(|record| &record.candidate_id == candidate_id)
    }

    /// A worker is terminal once it can no longer affect the race: it was
    /// cancelled, it exited without a candidate, or its candidate has a
    /// resolved verification trail. A worker with a pending verification is
    /// never terminal.
    pub #[tracing::instrument]
fn worker_terminal(&self, worker_id: &WorkerId) -> bool {
        self.cancellations
            .iter()
            .any(|cancelled| &cancelled.worker_id == worker_id)
            || self.exited.iter().any(|(exited, _)| exited == worker_id)
            || self.candidates.iter().any(|candidate| {
                &candidate.worker_id == worker_id
                    && self
                        .verification(&candidate.candidate_id)
                        .is_some_and(|record| {
                            record.failed.is_some() || record.verifier_unavailable
                        })
            })
    }

    /// Workers still consuming inference — everyone except terminal workers
    /// and the worker that produced the selected winner, whose work is no
    /// longer redundant by definition.
    pub #[tracing::instrument]
fn in_flight(&self) -> Vec<&WorkerSlot> {
        let winner_worker = self.winner.as_ref().map(|winner| &winner.worker_id);
        self.workers
            .iter()
            .filter(|slot| {
                Some(&slot.worker_id) != winner_worker && !self.worker_terminal(&slot.worker_id)
            })
            .collect()
    }

    /// True once every worker of a wave is terminal. A later wave may only
    /// be dispatched after the previous wave closed — that is what keeps
    /// wave 1 independent of wave 2.
    pub #[tracing::instrument]
fn wave_closed(&self, wave: u16) -> bool {
        let workers: Vec<&WorkerSlot> = self
            .workers
            .iter()
            .filter(|slot| slot.wave == wave)
            .collect();
        !workers.is_empty()
            && workers
                .iter()
                .all(|slot| self.worker_terminal(&slot.worker_id))
    }

    /// Candidates that passed every required gate without verifier
    /// unavailability.
    pub #[tracing::instrument]
fn verified_candidates(&self) -> Vec<&Candidate> {
        self.candidates
            .iter()
            .filter(|candidate| {
                self.verification(&candidate.candidate_id)
                    .is_some_and(|record| {
                        !record.passed.is_empty()
                            && record.failed.is_none()
                            && !record.verifier_unavailable
                    })
            })
            .collect()
    }

    /// Whether the corroboration requirement is satisfied by the currently
    /// verified candidates. The first response is never automatically the
    /// winner; this decides when one may be.
    pub #[tracing::instrument]
fn corroboration_satisfied(&self, mode: &BlitzCorroboration) -> bool {
        let verified = self.verified_candidates();
        match mode {
            BlitzCorroboration::FirstPass => !verified.is_empty(),
            BlitzCorroboration::TwoIndependentPasses => distinct_worker_count(&verified) >= 2,
            BlitzCorroboration::NOfM { n, .. } => {
                usize::try_from(*n).is_ok_and(|n| verified.len() >= n)
            }
            BlitzCorroboration::HeterogeneousConfirmation => heterogeneous(&verified, self),
            BlitzCorroboration::DeterministicOnly => verified.iter().any(|candidate| {
                self.verification(&candidate.candidate_id)
                    .is_some_and(|record| record.passed.iter().all(|gate| gate.deterministic()))
            }),
            BlitzCorroboration::HumanConfirmation => verified.iter().any(|candidate| {
                self.human_confirmations
                    .iter()
                    .any(|(confirmed, _)| confirmed == &candidate.candidate_id)
            }),
        }
    }

    /// The worker scouting rows for a closed blitz. Cancelled and failed
    /// workers are included: knowing who did *not* produce is the point.
    /// Token and cost attribution arrives via `WorkerSpent` bookkeeping in
    /// the fold; the zeros here are the wall the fold fills.
    pub #[tracing::instrument]
fn scouting(&self, spend: &[WorkerSpent]) -> Vec<WorkerScouting> {
        let winner_worker = self.winner.as_ref().map(|winner| &winner.worker_id);
        self.workers
            .iter()
            .map(|slot| {
                let candidate = self
                    .candidates
                    .iter()
                    .find(|candidate| candidate.worker_id == slot.worker_id);
                let verified = candidate.is_some_and(|candidate| {
                    self.verification(&candidate.candidate_id)
                        .is_some_and(|record| {
                            !record.passed.is_empty()
                                && record.failed.is_none()
                                && !record.verifier_unavailable
                        })
                }) || winner_worker == Some(&slot.worker_id);
                let spent = spend
                    .iter()
                    .find(|row| row.worker_id == slot.worker_id)
                    .map(|row| (row.tokens, row.cost_microunits, row.latency_ms))
                    .unwrap_or((0, 0, 0));
                WorkerScouting {
                    personnel_id: slot.personnel_id.clone(),
                    provider: slot.provider.clone(),
                    model_family: slot.model_family.clone(),
                    wave: slot.wave,
                    produced_candidate: candidate.is_some(),
                    candidate_verified: verified,
                    tokens: spent.0,
                    cost_microunits: spent.1,
                    latency_ms: spent.2,
                }
            })
            .collect()
    }
}

/// One worker's consumption, attributed at cancellation, exit, or closure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerSpent {
    pub worker_id: WorkerId,
    pub tokens: u64,
    pub cost_microunits: u64,
    pub latency_ms: u64,
}

#[tracing::instrument]
fn distinct_worker_count(candidates: &[&Candidate]) -> usize {
    let mut workers: Vec<&WorkerId> = candidates
        .iter()
        .map(|candidate| &candidate.worker_id)
        .collect();
    workers.sort_unstable();
    workers.dedup();
    workers.len()
}

#[tracing::instrument]
fn heterogeneous(candidates: &[&Candidate], blitz: &ActiveBlitz) -> bool {
    for left in candidates {
        for right in candidates {
            if left.worker_id == right.worker_id {
                continue;
            }
            let (Some(l), Some(r)) = (
                blitz.worker(&left.worker_id),
                blitz.worker(&right.worker_id),
            ) else {
                continue;
            };
            if l.provider != r.provider && l.model_family != r.model_family {
                return true;
            }
        }
    }
    false
}

/// A closed blitz. Everything needed to reconstruct why it was invoked, what
/// each worker attempted, what won, what was cancelled, what it cost, and
/// whether recovery actually occurred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlitzRecord {
    pub id: BlitzId,
    pub class: BlitzClass,
    pub closure: BlitzClosure,
    pub reason_code: String,
    pub target_hash: String,
    pub resource_mode: BlitzResourceMode,
    pub severity: Severity,
    pub outcome: Option<BlitzOutcome>,
    pub spent: BlitzCost,
    pub at_ms: u64,
    pub closed_at_ms: u64,
}

/// Event-sourced defensive state for one drive. The fold reconstructs
/// everything from blitz events; nothing here is a side channel.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BlitzLedger {
    active: Vec<ActiveBlitz>,
    closed: Vec<BlitzRecord>,
    proposed_total: u32,
    authorised_total: u32,
    rejected_total: u32,
    recovered_total: u32,
    exhausted_total: u32,
    escalated_total: u32,
    correlated_total: u32,
}

impl BlitzLedger {
    pub #[tracing::instrument]
fn empty() -> Self {
        Self::default()
    }

    pub #[tracing::instrument]
fn active(&self) -> &[ActiveBlitz] {
        &self.active
    }

    /// At most one defensive formation runs at a time.
    pub #[tracing::instrument]
fn active_blitz(&self) -> Option<&ActiveBlitz> {
        self.active.first()
    }

    pub #[tracing::instrument]
fn closed(&self) -> &[BlitzRecord] {
        &self.closed
    }

    pub #[tracing::instrument]
fn proposed_total(&self) -> u32 {
        self.proposed_total
    }

    pub #[tracing::instrument]
fn authorised_total(&self) -> u32 {
        self.authorised_total
    }

    pub #[tracing::instrument]
fn rejected_total(&self) -> u32 {
        self.rejected_total
    }

    pub #[tracing::instrument]
fn recovered_total(&self) -> u32 {
        self.recovered_total
    }

    pub #[tracing::instrument]
fn exhausted_total(&self) -> u32 {
        self.exhausted_total
    }

    pub #[tracing::instrument]
fn escalated_total(&self) -> u32 {
        self.escalated_total
    }

    pub #[tracing::instrument]
fn correlated_total(&self) -> u32 {
        self.correlated_total
    }

    pub #[tracing::instrument]
fn known_id(&self, id: &BlitzId) -> bool {
        self.active_blitz().is_some_and(|blitz| &blitz.id == id)
            || self.closed.iter().any(|record| &record.id == id)
    }

    /// The most recent closed blitz, or none.
    pub #[tracing::instrument]
fn last_closed(&self) -> Option<&BlitzRecord> {
        self.closed.last()
    }

    pub(crate) #[tracing::instrument]
fn active_mut(&mut self, id: &BlitzId) -> Result<&mut ActiveBlitz, PlaybookError> {
        self.active
            .iter_mut()
            .find(|blitz| &blitz.id == id)
            .ok_or_else(|| refusal("unknown blitz"))
    }

    pub(crate) #[tracing::instrument]
fn push_active(&mut self, blitz: ActiveBlitz) {
        self.proposed_total = self.proposed_total.saturating_add(1);
        self.active.push(blitz);
    }

    pub(crate) #[tracing::instrument]
fn count_authorised(&mut self) {
        self.authorised_total = self.authorised_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn count_rejected(&mut self) {
        self.rejected_total = self.rejected_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn count_recovered(&mut self) {
        self.recovered_total = self.recovered_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn count_exhausted(&mut self) {
        self.exhausted_total = self.exhausted_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn count_escalated(&mut self) {
        self.escalated_total = self.escalated_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn count_correlated(&mut self) {
        self.correlated_total = self.correlated_total.saturating_add(1);
    }

    pub(crate) #[tracing::instrument]
fn close_active(
        &mut self,
        id: &BlitzId,
        closure: BlitzClosure,
        reason_code: String,
        outcome: Option<BlitzOutcome>,
    ) -> Result<(), PlaybookError> {
        let index = self
            .active
            .iter()
            .position(|blitz| &blitz.id == id)
            .ok_or_else(|| refusal("unknown blitz"))?;
        let blitz = self.active.remove(index);
        self.closed.push(BlitzRecord {
            id: blitz.id,
            class: blitz.class,
            closure,
            reason_code,
            target_hash: blitz.target.target_hash,
            resource_mode: blitz.resource_mode,
            severity: blitz.pressure.severity,
            outcome,
            spent: blitz.spent,
            at_ms: blitz.at_ms,
            closed_at_ms: blitz.deadline_at_ms.max(blitz.at_ms),
        });
        Ok(())
    }
}

/// The error recorded when the fold refuses a blitz action.
pub #[tracing::instrument]
fn refusal(reason_code: &str) -> PlaybookError {
    PlaybookError::Inadmissible("blitz".into(), reason_code.into())
}

/// The recommendation handed to the huddle subsystem when independence has
/// failed and convergence must be purchased. The caller maps this onto an
/// actual huddle request; the blitz never opens one itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleRecommendation {
    pub scope_key: String,
    pub reason_code: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub failure: Option<BlitzFailure>,
    pub correlated_failures: u32,
}

/// Convenience seal helper used by tests and callers assembling a trigger
/// from raw evidence; canonical JSON bytes of the named bundle.
pub #[tracing::instrument]
fn evidence_bundle_digest(evidence_refs: &[EvidenceId]) -> Result<String, PlaybookError> {
    let mut refs: Vec<&str> = evidence_refs.iter().map(EvidenceId::as_str).collect();
    refs.sort_unstable();
    refs.dedup();
    let bytes = canonical_bytes(&serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "evidence_refs": refs,
    }))?;
    Ok(sha256_hex(&bytes))
}
