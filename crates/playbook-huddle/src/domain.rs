//! Huddle domain model.
//!
//! A huddle is a budgeted, policy-admitted synchronization barrier invoked
//! when the expected cost of team-state divergence exceeds the expected cost
//! of cross-agent reconciliation. Natural-language discussion is an
//! implementation detail; every type here is the authority.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use playbook_core::{
    BarrierMode, EventId, EvidenceClass, EvidenceId, GateId, GateState, HuddleId, PersonnelId,
};

macro_rules! huddle_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, playbook_core::PlaybookError> {
                let value = value.into();
                if value.is_empty() || value.len() > 200 {
                    return Err(playbook_core::PlaybookError::InvalidId);
                }
                let ok = value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
                if !ok {
                    return Err(playbook_core::PlaybookError::InvalidId);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

huddle_id!(ContradictionId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddleScope {
    Position,
    Unit,
    Coordinator,
    Team,
    Franchise,
}

impl HuddleScope {
    pub fn machine_id(&self) -> &'static str {
        match self {
            Self::Position => "POSITION",
            Self::Unit => "UNIT",
            Self::Coordinator => "COORDINATOR",
            Self::Team => "TEAM",
            Self::Franchise => "FRANCHISE",
        }
    }

    pub fn presentation(&self) -> &'static str {
        match self {
            Self::Position => "Position huddle",
            Self::Unit => "Unit huddle",
            Self::Coordinator => "Coordinator huddle",
            Self::Team => "Team huddle",
            Self::Franchise => "Franchise huddle",
        }
    }

    /// Escalation ordering. A huddle may only move up this ladder, and only
    /// with scope justification.
    pub fn rank(&self) -> u8 {
        match self {
            Self::Position => 0,
            Self::Unit => 1,
            Self::Coordinator => 2,
            Self::Team => 3,
            Self::Franchise => 4,
        }
    }

    pub fn next_up(&self) -> Option<Self> {
        match self {
            Self::Position => Some(Self::Unit),
            Self::Unit => Some(Self::Coordinator),
            Self::Coordinator => Some(Self::Team),
            Self::Team => Some(Self::Franchise),
            Self::Franchise => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddleTriggerKind {
    Success,
    Failure,
    Strategic,
    CoherenceDebt,
    ManualAuthority,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleTrigger {
    pub kind: HuddleTriggerKind,
    pub reason_code: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub originating_event: EventId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddleRole {
    Contributor,
    Specialist,
    Validator,
    Coordinator,
    Observer,
}

impl HuddleRole {
    pub fn reports(&self) -> bool {
        !matches!(self, Self::Observer)
    }
}

/// Asymmetric disclosure control (the Jose seam). The regulator decides what
/// each participant sees; it must never alter evidence itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosureProfile {
    pub include_gates: bool,
    pub include_peer_reports: bool,
    pub include_strategic_state: bool,
    pub max_evidence_refs: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleParticipant {
    pub personnel_id: PersonnelId,
    pub role: HuddleRole,
    pub disclosure_profile: DisclosureProfile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleCost {
    pub tokens: u64,
    pub cost_microunits: u64,
    pub duration_ms: u64,
    pub rounds: u16,
}

impl HuddleCost {
    pub fn zero() -> Self {
        Self {
            tokens: 0,
            cost_microunits: 0,
            duration_ms: 0,
            rounds: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRef {
    pub personnel_id: PersonnelId,
    pub statement: String,
    pub evidence_ref: Option<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contradiction {
    pub id: ContradictionId,
    pub subject: String,
    pub claims: Vec<ClaimRef>,
    pub severity: ContradictionSeverity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionState {
    ResolvedByEvidence,
    ResolvedBySupersession,
    ResolvedByAuthority,
    Deferred,
    UnresolvedBlocking,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContradictionResolution {
    pub contradiction_id: ContradictionId,
    pub state: ResolutionState,
    pub evidence_ref: Option<EvidenceId>,
    pub authority: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blocker {
    pub subject: String,
    pub reason: String,
    pub evidence_ref: Option<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssumptionDelta {
    pub subject: String,
    pub previous: String,
    pub current: String,
    pub evidence_ref: Option<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyDelta {
    pub dependency: String,
    pub change: String,
    pub evidence_ref: Option<EvidenceId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayProposal {
    pub operation: String,
    pub rationale: String,
    pub target_gate: Option<GateId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleReport {
    pub participant: PersonnelId,
    pub verified_progress: Vec<EvidenceId>,
    pub blockers: Vec<Blocker>,
    pub changed_assumptions: Vec<AssumptionDelta>,
    pub dependency_changes: Vec<DependencyDelta>,
    pub contradictions: Vec<Contradiction>,
    pub proposed_action: Option<PlayProposal>,
    pub confidence_milli: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSnapshot {
    pub gate_id: GateId,
    pub weight_ppm: u32,
    pub satisfaction_ppm: u32,
    pub state: GateState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSummary {
    pub evidence_id: EvidenceId,
    pub class: EvidenceClass,
    pub observed_at_ms: u64,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub personnel_id: PersonnelId,
    pub operation: String,
    pub target_gate: Option<GateId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonnelAction {
    SubstitutionRecommended,
    Release,
    Reassign,
    Rest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonnelChange {
    pub personnel_id: PersonnelId,
    pub action: PersonnelAction,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentChange {
    pub personnel_id: PersonnelId,
    pub from: Option<String>,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyChange {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub justification: String,
}

/// The HTTP body for opening a huddle on the drive service. The authority
/// derives the trigger's originating event and the participant roster
/// relevance itself; the caller states the trigger and the economics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleRequestBody {
    pub scope: HuddleScope,
    pub trigger_kind: HuddleTriggerKind,
    pub reason_code: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub manual: bool,
    pub value: crate::admission::HuddleValue,
    /// The caller's coherence-debt reading at request time.
    pub debt: u32,
    /// The caller's estimated rework-avoided value for this huddle.
    pub rework_avoided: u32,
}

/// The minimum sufficient shared state a huddle agrees on. It records which
/// evidence and gate readings became common knowledge; it can never satisfy
/// a gate by itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedStateDelta {
    pub verified_evidence: Vec<EvidenceId>,
    pub gate_snapshots: Vec<GateSnapshot>,
    pub assumptions: Vec<AssumptionDelta>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleCondition {
    pub kind: HuddleTriggerKind,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleOutcome {
    pub huddle_id: HuddleId,
    pub reconciled_state: SharedStateDelta,
    pub resolved_contradictions: Vec<ContradictionResolution>,
    pub unresolved_contradictions: Vec<Contradiction>,
    pub assignment_changes: Vec<AssignmentChange>,
    pub strategy_change: Option<StrategyChange>,
    pub personnel_changes: Vec<PersonnelChange>,
    pub revised_ytg_milli: Option<u32>,
    /// The requester's coherence-debt reading after reconciliation, when one
    /// was measured. The authority journals it; it never computes it.
    pub debt_after: Option<u32>,
    pub next_play: Option<PlayProposal>,
    pub next_huddle_conditions: Vec<HuddleCondition>,
    pub escalated_to: Option<HuddleScope>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantBudget {
    pub max_tokens: u64,
    pub max_duration_ms: u64,
}

/// Purpose-built context packet. Each participant receives the minimum
/// sufficient shared state, never the drive transcript.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HuddleContextPacket {
    pub huddle_id: HuddleId,
    pub trigger: HuddleTrigger,
    pub task_fragment: String,
    pub relevant_gates: Vec<GateSnapshot>,
    pub relevant_evidence: Vec<EvidenceSummary>,
    pub current_assignment: Assignment,
    pub dependency_changes: Vec<DependencyDelta>,
    pub contradictions: Vec<Contradiction>,
    pub budget: ParticipantBudget,
    pub requested_response_schema: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddlePhase {
    Requested,
    Admitted,
    CollectReports,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveHuddle {
    pub id: HuddleId,
    pub scope: HuddleScope,
    pub barrier: BarrierMode,
    pub phase: HuddlePhase,
    pub trigger: HuddleTrigger,
    pub budget: playbook_core::HuddleBudget,
    pub spent: HuddleCost,
    pub participants: Vec<HuddleParticipant>,
    pub reports: Vec<HuddleReport>,
    pub contradictions: Vec<Contradiction>,
    pub exhausted: bool,
    pub escalation: Option<(HuddleScope, String)>,
    pub partition: Vec<String>,
    pub field_fingerprint: String,
    pub at_ms: u64,
    /// The requester's coherence-debt reading at request time.
    pub debt_before: u32,
    /// YTG milli at request time, read from the drive view.
    pub ytg_milli_before: u32,
    /// The request's estimated rework-avoided value.
    pub rework_avoided: u32,
}

impl ActiveHuddle {
    pub fn reported(&self, personnel: &PersonnelId) -> bool {
        self.reports
            .iter()
            .any(|report| &report.participant == personnel)
    }

    pub fn contradiction(&self, id: &ContradictionId) -> bool {
        self.contradictions.iter().any(|item| &item.id == id)
    }

    /// The state partition this huddle locks: its participants and the
    /// evidence its trigger cites. Two huddles with disjoint partitions may
    /// run concurrently.
    pub fn partition(&self) -> Vec<String> {
        self.partition.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddleClosure {
    Rejected,
    Committed,
    Partial,
    ForcedSynthesis,
    Escalated,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HuddleRecord {
    pub id: HuddleId,
    pub scope: HuddleScope,
    pub closure: HuddleClosure,
    pub reason_code: String,
    pub field_fingerprint: String,
    pub outcome: Option<HuddleOutcome>,
    pub debt_before: u32,
    pub ytg_milli_before: u32,
    pub rework_avoided: u32,
}

/// Event-sourced coherence state for one drive. The fold reconstructs
/// everything from huddle events; nothing here is a side channel.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HuddleLedger {
    active: Vec<ActiveHuddle>,
    closed: Vec<HuddleRecord>,
    assignments: BTreeMap<String, Assignment>,
    substitutions: Vec<PersonnelChange>,
    proposals: Vec<PlayProposal>,
    conditions: Vec<HuddleCondition>,
    requested_total: u32,
    rejected_total: u32,
    committed_total: u32,
    escalated_total: u32,
}

impl HuddleLedger {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn active(&self) -> &[ActiveHuddle] {
        &self.active
    }

    pub fn closed(&self) -> &[HuddleRecord] {
        &self.closed
    }

    pub fn assignments(&self) -> &BTreeMap<String, Assignment> {
        &self.assignments
    }

    pub fn substitutions(&self) -> &[PersonnelChange] {
        &self.substitutions
    }

    pub fn proposals(&self) -> &[PlayProposal] {
        &self.proposals
    }

    pub fn conditions(&self) -> &[HuddleCondition] {
        &self.conditions
    }

    pub fn requested_total(&self) -> u32 {
        self.requested_total
    }

    pub fn rejected_total(&self) -> u32 {
        self.rejected_total
    }

    pub fn committed_total(&self) -> u32 {
        self.committed_total
    }

    pub fn escalated_total(&self) -> u32 {
        self.escalated_total
    }

    pub fn active_by_id(&self, id: &HuddleId) -> Option<&ActiveHuddle> {
        self.active.iter().find(|item| &item.id == id)
    }

    pub fn active_by_scope(&self, scope: HuddleScope) -> Option<&ActiveHuddle> {
        self.active.iter().find(|item| item.scope == scope)
    }

    pub fn known_id(&self, id: &HuddleId) -> bool {
        self.active_by_id(id).is_some() || self.closed.iter().any(|record| &record.id == id)
    }

    /// The most recent closed huddle for a scope, or none.
    pub fn last_closed(&self, scope: HuddleScope) -> Option<&HuddleRecord> {
        self.closed
            .iter()
            .rev()
            .find(|record| record.scope == scope)
    }

    pub(crate) fn active_mut(
        &mut self,
        id: &HuddleId,
    ) -> Result<&mut ActiveHuddle, playbook_core::PlaybookError> {
        self.active
            .iter_mut()
            .find(|item| &item.id == id)
            .ok_or_else(|| {
                playbook_core::PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into())
            })
    }

    pub(crate) fn close_active(
        &mut self,
        id: &HuddleId,
        closure: HuddleClosure,
        reason_code: String,
        outcome: Option<HuddleOutcome>,
    ) -> Result<(), playbook_core::PlaybookError> {
        let index = self
            .active
            .iter()
            .position(|item| &item.id == id)
            .ok_or_else(|| {
                playbook_core::PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into())
            })?;
        let huddle = self.active.remove(index);
        self.closed.push(HuddleRecord {
            id: huddle.id,
            scope: huddle.scope,
            closure,
            reason_code,
            field_fingerprint: huddle.field_fingerprint,
            outcome,
            debt_before: huddle.debt_before,
            ytg_milli_before: huddle.ytg_milli_before,
            rework_avoided: huddle.rework_avoided,
        });
        Ok(())
    }

    pub(crate) fn push_active(&mut self, huddle: ActiveHuddle) {
        self.requested_total = self.requested_total.saturating_add(1);
        self.active.push(huddle);
    }

    pub(crate) fn count_rejected(&mut self) {
        self.rejected_total = self.rejected_total.saturating_add(1);
    }

    pub(crate) fn count_committed(&mut self, escalated: bool) {
        self.committed_total = self.committed_total.saturating_add(1);
        if escalated {
            self.escalated_total = self.escalated_total.saturating_add(1);
        }
    }

    pub(crate) fn count_escalated(&mut self) {
        self.escalated_total = self.escalated_total.saturating_add(1);
    }

    pub(crate) fn set_conditions(&mut self, conditions: Vec<HuddleCondition>) {
        self.conditions = conditions;
    }

    pub(crate) fn apply_assignment(&mut self, change: &AssignmentChange, target: Option<GateId>) {
        let assignment = Assignment {
            personnel_id: change.personnel_id.clone(),
            operation: change.to.clone(),
            target_gate: target,
        };
        self.assignments
            .insert(change.personnel_id.as_str().to_string(), assignment);
    }

    pub(crate) fn record_substitutions(&mut self, changes: &[PersonnelChange]) {
        self.substitutions.extend(changes.iter().cloned());
    }

    pub(crate) fn record_proposal(&mut self, proposal: &PlayProposal) {
        self.proposals.push(proposal.clone());
    }
}

/// Summary of coherence for one drive, derived from the ledger and the
/// current coherence-debt inputs. Machine-readable `H_t`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoherenceState {
    pub debt: u32,
    pub band: DebtBand,
    pub active_huddles: u32,
    pub closed_huddles: u32,
    pub unresolved_contradictions: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebtBand {
    Healthy,
    Monitor,
    HuddleEligible,
    CoordinatorRecommended,
    MandatoryReconciliation,
}
