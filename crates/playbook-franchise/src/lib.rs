//! Durable management above disposable execution resources.
//!
//! The franchise owns objectives, roster and allocations. Playbook's authority
//! remains the independent official for evidence, progress and completion.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use playbook_core::*;
use playbook_elci::CapabilityPlane;
use playbook_events::DriveEvent;
use playbook_front_office::RosterContract;
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode, Authority, DriveState, EventSink};
use serde::{Deserialize, Serialize};

pub mod bench;
pub mod manmark;

pub use bench::{decide, BenchCandidate, BenchInput, SubstitutionClass, SubstitutionDecision};
pub use manmark::{InspectionTrigger, ManmarkRegistry, Mark, MarkOutcome, MarkState, Remediation};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Ready,
    Benched,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosterMember {
    pub personnel: Personnel,
    pub position: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub id: ObjectiveId,
    pub task: TaskContract,
    pub gates: Vec<Gate>,
    /// One durable drive ledger; Lastdown creates new possessions inside it.
    pub drive_id: Option<DriveId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedDrive {
    pub objective_id: ObjectiveId,
    /// Lifetime grants, including every renewed possession. Never reset.
    pub allocated: DriveBudget,
    pub seed: HistoricalPriorTable,
    pub events: Vec<DriveEvent>,
    /// Conservative charges for confirmed executions whose costs were lost.
    pub unknown_cost_reservations: BTreeMap<String, DriveBudget>,
    /// Eligibility is immutable for this entire drive, including punts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_roster: Option<FrozenRoster>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRoster {
    pub contract: Option<RosterContract>,
    pub members: BTreeMap<String, Personnel>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagementRecord {
    pub sequence: u64,
    pub action: String,
    pub subject: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FranchiseSnapshot {
    pub schema_version: u32,
    pub id: FranchiseId,
    pub salary_cap: DriveBudget,
    /// Lifetime grants reserve cap before any dispatch, across all objectives.
    pub allocated: DriveBudget,
    pub roster: BTreeMap<String, RosterMember>,
    pub objectives: BTreeMap<String, Objective>,
    pub drives: BTreeMap<String, ManagedDrive>,
    pub management: Vec<ManagementRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_roster: Option<RosterContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_roster: Option<RosterContract>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FranchiseRecord {
    pub objectives: usize,
    pub completed: usize,
    pub active: usize,
    pub punted: usize,
    pub abandoned: usize,
    pub field_goals: usize,
    pub unresolved_invocations: usize,
    pub measured_outcomes: u32,
    pub observed_spend: DriveBudget,
    pub unmeasured_spend_reserved: DriveBudget,
    pub cap_unallocated: DriveBudget,
}

impl FranchiseSnapshot {
    pub #[tracing::instrument]
fn scouting(&self) -> Result<HistoricalPriorTable, PlaybookError> {
        let mut result = HistoricalPriorTable::empty();
        for drive in self.drives.values() {
            let state = drive.state()?;
            result.merge_observations(state.priors(), &drive.seed)?;
        }
        Ok(result)
    }

    pub #[tracing::instrument]
fn record(&self) -> Result<FranchiseRecord, PlaybookError> {
        let mut record = FranchiseRecord {
            objectives: self.objectives.len(),
            cap_unallocated: self.salary_cap.remaining(self.allocated),
            measured_outcomes: self.scouting()?.samples(),
            ..FranchiseRecord::default()
        };
        for drive in self.drives.values() {
            for reserve in drive.unknown_cost_reservations.values() {
                record.unmeasured_spend_reserved =
                    record.unmeasured_spend_reserved.checked_add(*reserve)?;
            }
            let state = drive.state()?;
            record.unresolved_invocations += usize::from(state.open_execution().is_some());
            match state.status() {
                DriveStatus::Completed => record.completed += 1,
                DriveStatus::Active => record.active += 1,
                DriveStatus::Punted => record.punted += 1,
                DriveStatus::Abandoned => record.abandoned += 1,
                DriveStatus::FieldGoalScored => record.field_goals += 1,
            }
            for event in &drive.events {
                let cost = match event {
                    DriveEvent::ToolSucceeded { result } => result.cost,
                    DriveEvent::ToolFailed { failure } => failure.cost,
                    DriveEvent::ToolOutcomeRejected {
                        result, failure, ..
                    } => result
                        .as_ref()
                        .map(|r| r.cost)
                        .or_else(|| failure.as_ref().map(|f| f.cost))
                        .unwrap_or_default(),
                    DriveEvent::BudgetConsumed { cost, .. } => *cost,
                    _ => DriveBudget::zero(),
                };
                record.observed_spend = record.observed_spend.checked_add(cost)?;
            }
        }
        record.observed_spend = record
            .observed_spend
            .remaining(record.unmeasured_spend_reserved);
        Ok(record)
    }

    #[tracing::instrument]
fn audit(&self) -> Result<(), PlaybookError> {
        accept_schema(self.schema_version)?;
        FranchiseId::new(self.id.as_str())?;
        for contract in [&self.current_roster, &self.staged_roster]
            .into_iter()
            .flatten()
        {
            contract.verify()?;
        }
        if !self.allocated.fits_within(self.salary_cap) {
            return Err(PlaybookError::BudgetCeiling);
        }
        let mut allocated = DriveBudget::zero();
        for (id, member) in &self.roster {
            validate_member(member)?;
            if id != member.personnel.id.as_str() {
                return Err(invalid("roster key disagrees with personnel id"));
            }
        }
        for (id, objective) in &self.objectives {
            ObjectiveId::new(id)?;
            validate_objective(objective, true)?;
            if id != objective.id.as_str() {
                return Err(invalid("objective key disagrees with id"));
            }
            if let Some(drive_id) = &objective.drive_id {
                if self.drives.get(drive_id.as_str()).map(|d| &d.objective_id)
                    != Some(&objective.id)
                {
                    return Err(invalid("objective drive is missing"));
                }
            }
        }
        for (id, drive) in &self.drives {
            DriveId::new(id)?;
            let objective = self
                .objectives
                .get(drive.objective_id.as_str())
                .ok_or_else(|| invalid("drive objective is missing"))?;
            let state = drive.state()?;
            if let Some(frozen) = &drive.frozen_roster {
                if let Some(contract) = &frozen.contract {
                    contract.verify()?;
                    if contract.plan.members.len() != frozen.members.len()
                        || contract
                            .plan
                            .members
                            .iter()
                            .any(|m| !frozen.members.contains_key(m.personnel_id.as_str()))
                    {
                        return Err(invalid("frozen eligibility disagrees with roster contract"));
                    }
                }
                for (id, member) in &frozen.members {
                    if id != member.id.as_str() || !self.roster.contains_key(id) {
                        return Err(invalid("frozen roster contains unknown personnel"));
                    }
                }
                for event in &drive.events {
                    if let DriveEvent::ToolInvoked { execution } = event {
                        if !frozen.members.contains_key(execution.personnel_id.as_str()) {
                            return Err(invalid("execution escaped frozen eligibility"));
                        }
                    }
                }
            }
            let seeds: Vec<_> = drive
                .events
                .iter()
                .filter_map(|e| match e {
                    DriveEvent::PriorsLoaded { priors } => Some(priors),
                    _ => None,
                })
                .collect();
            if seeds != vec![&drive.seed] {
                return Err(invalid("drive scouting seed disagrees with history"));
            }
            if state.drive_id().as_str() != id
                || state.task() != &objective.task
                || objective.drive_id.as_ref().map(DriveId::as_str) != Some(id.as_str())
            {
                return Err(invalid("drive ownership disagrees with objective"));
            }
            let mut grants = DriveBudget::zero();
            for event in &drive.events {
                match event {
                    DriveEvent::DriveStarted { budget, .. }
                    | DriveEvent::PossessionStarted { budget, .. } => {
                        grants = grants.checked_add(*budget)?
                    }
                    _ => {}
                }
            }
            if grants != drive.allocated {
                return Err(invalid("drive allocation disagrees with its grants"));
            }
            allocated = allocated.checked_add(drive.allocated)?;
        }
        if allocated != self.allocated {
            return Err(invalid("franchise allocation disagrees with drive grants"));
        }
        self.scouting()?;
        Ok(())
    }

    #[tracing::instrument]
fn note(&mut self, action: &str, subject: &str, reason: &str) {
        self.management.push(ManagementRecord {
            sequence: self.management.len() as u64 + 1,
            action: action.into(),
            subject: subject.into(),
            reason: reason.into(),
        });
    }

    #[tracing::instrument]
fn freeze_roster(&self) -> Result<FrozenRoster, PlaybookError> {
        let mut members = BTreeMap::new();
        if let Some(contract) = &self.current_roster {
            contract.verify()?;
            for entry in &contract.plan.members {
                let member = self
                    .roster
                    .get(entry.personnel_id.as_str())
                    .ok_or_else(|| invalid("roster contract references unregistered personnel"))?;
                if member.provider.as_deref() != Some(entry.provider.as_str()) {
                    return Err(invalid(
                        "registered provider disagrees with roster contract",
                    ));
                }
                members.insert(entry.personnel_id.to_string(), member.personnel.clone());
            }
        } else {
            // Compatibility for manually assembled preseason rosters.
            members.extend(
                self.roster
                    .iter()
                    .map(|(id, m)| (id.clone(), m.personnel.clone())),
            );
        }
        Ok(FrozenRoster {
            contract: self.current_roster.clone(),
            members,
        })
    }
}

impl ManagedDrive {
    pub #[tracing::instrument]
fn state(&self) -> Result<DriveState, PlaybookError> {
        fold(&self.events, &Policy::builtin(), ApplyMode::Audit)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    digest: String,
    franchise: FranchiseSnapshot,
}

struct Storage {
    path: PathBuf,
    state: FranchiseSnapshot,
    /// A rename followed by failed directory sync has uncertain durability.
    /// Refuse more writes until the host reopens the store.
    poisoned: bool,
}

#[tracing::instrument]
fn invalid(message: &str) -> PlaybookError {
    PlaybookError::Evidence(message.into())
}
#[tracing::instrument]
fn io(error: std::io::Error) -> PlaybookError {
    PlaybookError::Io(error.to_string())
}

impl Storage {
    #[tracing::instrument]
fn persist(&mut self, next: FranchiseSnapshot) -> Result<(), PlaybookError> {
        if self.poisoned {
            return Err(PlaybookError::Io(
                "reopen franchise after uncertain write".into(),
            ));
        }
        let envelope = Envelope {
            digest: sha256_hex(&to_canonical_bytes(&next)?),
            franchise: next.clone(),
        };
        let pending = self.path.with_extension("pending");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&pending)
            .map_err(io)?;
        file.write_all(&serde_json::to_vec(&envelope).map_err(PlaybookError::json)?)
            .map_err(io)?;
        file.sync_all().map_err(io)?;
        std::fs::rename(&pending, &self.path).map_err(io)?;
        self.state = next;
        if let Err(error) = File::open(
            self.path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .and_then(|dir| dir.sync_all())
        {
            self.poisoned = true;
            return Err(io(error));
        }
        Ok(())
    }
}

/// Single host owner. Methods require exclusive access; the HTTP service
/// serializes each franchise. Do not open a second writer on the same path.
pub struct Franchise {
    storage: Arc<Mutex<Storage>>,
}

impl Franchise {
    pub #[tracing::instrument]
fn create(
        path: &Path,
        id: FranchiseId,
        salary_cap: DriveBudget,
    ) -> Result<Self, PlaybookError> {
        FranchiseId::new(id.as_str())?;
        let state = FranchiseSnapshot {
            schema_version: SCHEMA_VERSION,
            id,
            salary_cap,
            allocated: DriveBudget::zero(),
            roster: BTreeMap::new(),
            objectives: BTreeMap::new(),
            drives: BTreeMap::new(),
            management: Vec::new(),
            current_roster: None,
            staged_roster: None,
        };
        // Exclusive creation prevents accidental replacement of an existing franchise.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(io)?;
        drop(file);
        let mut storage = Storage {
            path: path.into(),
            state: state.clone(),
            poisoned: false,
        };
        storage.persist(state)?;
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
        })
    }

    pub #[tracing::instrument]
fn open(path: &Path) -> Result<Self, PlaybookError> {
        let envelope: Envelope = serde_json::from_slice(&std::fs::read(path).map_err(io)?)
            .map_err(PlaybookError::json)?;
        if envelope.digest != sha256_hex(&to_canonical_bytes(&envelope.franchise)?) {
            return Err(PlaybookError::DigestMismatch);
        }
        envelope.franchise.audit()?;
        let mut state = envelope.franchise;
        // Old snapshots omitted these optional fields, preserving their original
        // digest. Freeze legacy membership once at recovery and persist it.
        let legacy = state.drives.values().any(|d| d.frozen_roster.is_none());
        if legacy {
            let frozen = state.freeze_roster()?;
            for drive in state
                .drives
                .values_mut()
                .filter(|d| d.frozen_roster.is_none())
            {
                drive.frozen_roster = Some(frozen.clone());
            }
        }
        let mut storage = Storage {
            path: path.into(),
            state,
            poisoned: false,
        };
        if legacy {
            storage.persist(storage.state.clone())?;
        }
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
        })
    }

    pub #[tracing::instrument]
fn snapshot(&self) -> FranchiseSnapshot {
        self.storage
            .lock()
            .expect("franchise storage")
            .state
            .clone()
    }

    #[tracing::instrument]
fn update(
        &mut self,
        action: impl FnOnce(&mut FranchiseSnapshot) -> Result<(), PlaybookError>,
    ) -> Result<(), PlaybookError> {
        let mut storage = self.storage.lock().expect("franchise storage");
        let mut next = storage.state.clone();
        action(&mut next)?;
        storage.persist(next)
    }

    pub #[tracing::instrument]
fn acquire(&mut self, member: RosterMember, reason: &str) -> Result<(), PlaybookError> {
        validate_member(&member)?;
        require_reason(reason)?;
        self.update(|next| {
            let id = member.personnel.id.to_string();
            if next.roster.contains_key(&id) {
                return Err(invalid("personnel already acquired"));
            }
            next.roster.insert(id.clone(), member);
            next.note("acquire", &id, reason);
            Ok(())
        })
    }

    /// Quarantine a prospective roster. This does not change any eligible roster.
    pub #[tracing::instrument]
fn stage_roster(
        &mut self,
        contract: RosterContract,
        reason: &str,
    ) -> Result<(), PlaybookError> {
        contract.verify()?;
        require_reason(reason)?;
        self.update(|next| {
            if next.current_roster.as_ref().is_some_and(|current| {
                current.plan.effective_season >= contract.plan.effective_season
            }) {
                return Err(invalid("a future roster must advance the season"));
            }
            next.note("stage_roster", &contract.digest, reason);
            next.staged_roster = Some(contract);
            Ok(())
        })
    }

    /// Explicit transaction window: every contest must have ended. A punt is
    /// continuity inside the same contest, so it does not open this window.
    pub #[tracing::instrument]
fn activate_roster(&mut self, reason: &str) -> Result<(), PlaybookError> {
        require_reason(reason)?;
        self.update(|next| {
            for drive in next.drives.values() {
                let state = drive.state()?;
                if matches!(state.status(), DriveStatus::Active | DriveStatus::Punted)
                    || state.open_execution().is_some()
                    || !state.unresolved_effects().is_empty()
                {
                    return Err(invalid(
                        "roster transaction window is closed during a contest",
                    ));
                }
            }
            let contract = next
                .staged_roster
                .take()
                .ok_or_else(|| invalid("no roster contract is staged"))?;
            next.current_roster = Some(contract.clone());
            next.freeze_roster()?;
            next.note("activate_roster", &contract.digest, reason);
            Ok(())
        })
    }

    pub #[tracing::instrument]
fn set_availability(
        &mut self,
        id: &PersonnelId,
        availability: Availability,
        reason: &str,
    ) -> Result<(), PlaybookError> {
        require_reason(reason)?;
        self.update(|next| {
            next.roster
                .get_mut(id.as_str())
                .ok_or_else(|| invalid("unknown personnel"))?
                .availability = availability;
            next.note(
                match availability {
                    Availability::Ready => "activate",
                    Availability::Benched => "bench",
                    Availability::Unavailable => "unavailable",
                },
                id.as_str(),
                reason,
            );
            Ok(())
        })
    }

    pub #[tracing::instrument]
fn declare_objective(&mut self, objective: Objective) -> Result<(), PlaybookError> {
        validate_objective(&objective, false)?;
        self.update(|next| {
            let id = objective.id.to_string();
            if next.objectives.contains_key(&id) {
                return Err(invalid("objective already exists"));
            }
            next.objectives.insert(id.clone(), objective);
            next.note("objective", &id, "acceptance criteria declared");
            Ok(())
        })
    }

    pub #[tracing::instrument]
fn start_drive(
        &mut self,
        objective_id: &ObjectiveId,
        drive_id: DriveId,
        budget: DriveBudget,
        at_ms: u64,
    ) -> Result<(), PlaybookError> {
        DriveId::new(drive_id.as_str())?;
        self.update(|next| {
            if next.drives.contains_key(drive_id.as_str()) {
                return Err(invalid("drive already exists"));
            }
            let objective = next
                .objectives
                .get(objective_id.as_str())
                .ok_or_else(|| invalid("unknown objective"))?
                .clone();
            if objective.drive_id.is_some() {
                return Err(invalid(
                    "objective already has a drive; renew its possession",
                ));
            }
            let allocated = next.allocated.checked_add(budget)?;
            if !allocated.fits_within(next.salary_cap) {
                return Err(PlaybookError::BudgetCeiling);
            }
            let seed = next.scouting()?;
            let mut authority = Authority::start(
                drive_id.clone(),
                objective.task,
                budget,
                Policy::builtin(),
                Vec::new(),
                at_ms,
            )?;
            for gate in objective.gates {
                authority.declare_gate(gate)?;
            }
            authority.load_priors(seed.clone())?;
            let frozen_roster = next.freeze_roster()?;
            next.drives.insert(
                drive_id.to_string(),
                ManagedDrive {
                    objective_id: objective_id.clone(),
                    allocated: budget,
                    seed,
                    events: authority.log().to_vec(),
                    unknown_cost_reservations: BTreeMap::new(),
                    frozen_roster: Some(frozen_roster),
                },
            );
            next.objectives
                .get_mut(objective_id.as_str())
                .expect("checked objective")
                .drive_id = Some(drive_id.clone());
            next.allocated = allocated;
            next.note(
                "drive",
                drive_id.as_str(),
                "bounded allocation reserved before dispatch",
            );
            Ok(())
        })
    }

    #[tracing::instrument]
fn authority(
        &self,
        id: &DriveId,
        player: Option<&PersonnelId>,
    ) -> Result<Authority, PlaybookError> {
        let snapshot = self.snapshot();
        let drive = snapshot
            .drives
            .get(id.as_str())
            .ok_or_else(|| invalid("unknown drive"))?;
        let mut busy = Vec::new();
        for (other_id, other) in &snapshot.drives {
            if other_id != id.as_str() {
                if let Some(execution) = other.state()?.open_execution() {
                    busy.push(execution.personnel_id.clone());
                }
            }
        }
        let frozen = drive
            .frozen_roster
            .as_ref()
            .ok_or_else(|| invalid("drive eligibility was not frozen"))?;
        let roster: Vec<Personnel> = frozen
            .members
            .values()
            .filter(|m| {
                snapshot
                    .roster
                    .get(m.id.as_str())
                    .is_some_and(|member| member.availability == Availability::Ready)
                    && !busy.contains(&m.id)
                    && player.is_none_or(|id| id == &m.id)
            })
            .cloned()
            .collect();
        if player.is_some() && roster.is_empty() {
            return Err(invalid("player is unavailable, benched or busy"));
        }
        let mut authority = Authority::resume(drive.events.clone(), Policy::builtin(), roster)?;
        authority.attach_journal(Box::new(FranchiseSink {
            storage: Arc::clone(&self.storage),
            drive_id: id.to_string(),
        }));
        Ok(authority)
    }

    pub #[tracing::instrument]
fn preflight(
        &mut self,
        id: &DriveId,
        plane: &mut dyn CapabilityPlane,
    ) -> Result<(), PlaybookError> {
        self.authority(id, None)?.preflight(plane)
    }

    /// The coordinator selects from ready personnel, or constrains a bounded
    /// play to one explicitly selected substitute. All progress stays official.
    pub #[tracing::instrument]
fn step(
        &mut self,
        id: &DriveId,
        player: Option<&PersonnelId>,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        let snapshot = self.snapshot();
        let record = snapshot.record()?;
        if !record
            .observed_spend
            .checked_add(record.unmeasured_spend_reserved)?
            .fits_within(snapshot.salary_cap)
        {
            return Err(PlaybookError::BudgetExhausted);
        }
        self.authority(id, player)?.step(plane, at_ms)
    }

    pub #[tracing::instrument]
fn pressure(
        &mut self,
        id: &DriveId,
        cost: DriveBudget,
        at_ms: u64,
    ) -> Result<(), PlaybookError> {
        self.authority(id, None)?.pressure(cost, at_ms)
    }

    pub #[tracing::instrument]
fn punt(&mut self, id: &DriveId) -> Result<(), PlaybookError> {
        self.authority(id, None)?.punt()
    }

    pub #[tracing::instrument]
fn receive(
        &mut self,
        id: &DriveId,
        player: &PersonnelId,
        budget: DriveBudget,
        at_ms: u64,
    ) -> Result<(), PlaybookError> {
        let mut authority = self.authority(id, Some(player))?;
        let personnel = authority.roster()[0].clone();
        // The sink reserves this new grant atomically with PossessionStarted.
        authority.receive(personnel, budget, at_ms)
    }

    pub #[tracing::instrument]
fn reconcile(
        &mut self,
        id: &DriveId,
        key: &str,
        occurred: bool,
    ) -> Result<(), PlaybookError> {
        // No external execution occurs here. Commit accounting and reconciliation
        // together, so a crash cannot clear uncertainty before reserving its cost.
        self.update(|next| {
            let drive = next
                .drives
                .get_mut(id.as_str())
                .ok_or_else(|| invalid("unknown drive"))?;
            let mut authority =
                Authority::resume(drive.events.clone(), Policy::builtin(), Vec::new())?;
            let execution = authority
                .state()
                .open_execution()
                .filter(|e| e.idempotency_key.as_deref() == Some(key))
                .cloned();
            if occurred {
                if let Some(execution) = execution {
                    let has_observed_cost = drive
                        .events
                        .iter()
                        .rev()
                        .take_while(|e| !matches!(e, DriveEvent::ToolInvoked { .. }))
                        .any(|e| {
                            matches!(
                                e,
                                DriveEvent::ToolOutcomeRejected {
                                    result: Some(_),
                                    ..
                                } | DriveEvent::ToolOutcomeRejected {
                                    failure: Some(_),
                                    ..
                                }
                            )
                        });
                    if !has_observed_cost {
                        let reserve = authority
                            .state()
                            .budget_initial()
                            .remaining(authority.state().budget_consumed());
                        authority.pressure(reserve, execution.timeout_ms)?;
                        drive.unknown_cost_reservations.insert(key.into(), reserve);
                    }
                }
            }
            authority.reconcile(key, occurred)?;
            drive.events = authority.log().to_vec();
            next.note(
                "reconcile",
                id.as_str(),
                if occurred {
                    "effect occurred; unknown cost conservatively reserved"
                } else {
                    "effect did not occur"
                },
            );
            Ok(())
        })
    }

    pub #[tracing::instrument]
fn complete(&mut self, id: &DriveId, at_ms: u64) -> Result<bool, PlaybookError> {
        self.authority(id, None)?.claim_complete(at_ms)
    }

    pub #[tracing::instrument]
fn abandon(&mut self, id: &DriveId, reason: &str) -> Result<(), PlaybookError> {
        require_reason(reason)?;
        self.authority(id, None)?.abandon(reason)
    }
}

#[tracing::instrument]
fn require_reason(reason: &str) -> Result<(), PlaybookError> {
    if reason.trim().is_empty() {
        Err(invalid("management action requires a reason"))
    } else {
        Ok(())
    }
}

#[tracing::instrument]
fn validate_member(member: &RosterMember) -> Result<(), PlaybookError> {
    PersonnelId::new(member.personnel.id.as_str())?;
    if member.position.trim().is_empty() {
        return Err(invalid("position is required"));
    }
    for value in [
        member.personnel.capability_fit_ppm,
        member.personnel.reliability_ppm,
        member.personnel.quota_remaining_ppm,
        member.personnel.evidence_quality_ppm,
    ] {
        Ppm::new(value)?;
    }
    Ok(())
}

#[tracing::instrument]
fn validate_objective(objective: &Objective, existing: bool) -> Result<(), PlaybookError> {
    ObjectiveId::new(objective.id.as_str())?;
    if objective.task.statement.trim().is_empty()
        || objective.gates.is_empty()
        || (!existing && objective.drive_id.is_some())
        || !objective.gates.iter().any(|g| g.mandatory)
    {
        return Err(invalid(
            "objective requires a task and mandatory acceptance gates",
        ));
    }
    let mut visited = Vec::new();
    for gate in &objective.gates {
        GateId::new(gate.id.as_str())?;
        let declared = Gate::declare(
            gate.id.clone(),
            &gate.description,
            gate.weight_ppm,
            gate.dependencies.clone(),
            gate.required_evidence.clone(),
            gate.mandatory,
        )?;
        if gate != &declared
            || visited.contains(&gate.id)
            || gate.required_evidence.is_empty()
            || gate.dependencies.iter().any(|id| !visited.contains(id))
        {
            return Err(invalid(
                "gates must be pristine declarations in dependency order",
            ));
        }
        visited.push(gate.id.clone());
    }
    Ok(())
}

struct FranchiseSink {
    storage: Arc<Mutex<Storage>>,
    drive_id: String,
}

impl EventSink for FranchiseSink {
    #[tracing::instrument]
fn record(&mut self, event: &DriveEvent) -> Result<(), PlaybookError> {
        let mut storage = self.storage.lock().expect("franchise storage");
        let mut next = storage.state.clone();
        if let DriveEvent::PossessionStarted { budget, .. } = event {
            let allocated = next.allocated.checked_add(*budget)?;
            if !allocated.fits_within(next.salary_cap) {
                return Err(PlaybookError::BudgetCeiling);
            }
            next.allocated = allocated;
            let drive = next.drives.get_mut(&self.drive_id).expect("sink drive");
            drive.allocated = drive.allocated.checked_add(*budget)?;
        }
        next.drives
            .get_mut(&self.drive_id)
            .expect("sink drive")
            .events
            .push(event.clone());
        storage.persist(next)
    }

    #[tracing::instrument]
fn mark(&self) -> u64 {
        self.storage.lock().expect("franchise storage").state.drives[&self.drive_id]
            .events
            .len() as u64
    }

    #[tracing::instrument]
fn rollback(&mut self, mark: u64) -> Result<(), PlaybookError> {
        let mut storage = self.storage.lock().expect("franchise storage");
        let mut next = storage.state.clone();
        let drive = next.drives.get_mut(&self.drive_id).expect("sink drive");
        if mark as usize > drive.events.len() {
            return Err(invalid("invalid journal rollback"));
        }
        // Authority only rolls back decisions that never reached dispatch.
        if drive.events[mark as usize..]
            .iter()
            .any(|e| matches!(e, DriveEvent::ToolInvoked { .. }))
        {
            return Err(invalid("cannot roll back a dispatched invocation"));
        }
        drive.events.truncate(mark as usize);
        storage.persist(next)
    }
}
