//! Single writer for a drive.
//!
//! Workers return evidence. This authority commits it. Parallel agents do not
//! mutate the drive struct themselves.

use playbook_core::{
    DecisionRecord, Disposition, Down, DriveBudget, DriveId, Evidence, Gate, Personnel,
    PlayExecution, PlayId, PlaybookError, RetryPolicy, TaskContract,
};
use playbook_elci::{missing_evidence, CapabilityPlane, PlaneResult};
use playbook_events::DriveEvent;
use playbook_gates::is_complete;
use playbook_policy::Policy;

use crate::apply::ApplyMode;
use crate::{apply, DriveState};

pub struct Authority {
    state: DriveState,
    events: Vec<DriveEvent>,
    policy: Policy,
    roster: Vec<Personnel>,
}

impl Authority {
    pub fn start(
        drive_id: DriveId,
        task: TaskContract,
        budget: DriveBudget,
        policy: Policy,
        roster: Vec<Personnel>,
        at_ms: u64,
    ) -> Result<Self, PlaybookError> {
        let event = DriveEvent::DriveStarted {
            drive_id,
            task,
            budget,
            at_ms,
        };
        let state = apply(None, &event, &policy, ApplyMode::Audit)?;
        let mut authority = Self {
            state,
            events: vec![event],
            policy,
            roster,
        };
        authority.witnesses()?;
        Ok(authority)
    }

    pub fn state(&self) -> &DriveState {
        &self.state
    }

    pub fn log(&self) -> &[DriveEvent] {
        &self.events
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    pub fn roster(&self) -> &[Personnel] {
        &self.roster
    }

    pub fn declare_gate(&mut self, gate: Gate) -> Result<(), PlaybookError> {
        self.push(DriveEvent::GateDeclared { gate })?;
        self.witnesses()
    }

    pub fn preflight(&mut self, plane: &mut dyn CapabilityPlane) -> Result<(), PlaybookError> {
        let snapshot = plane.discover()?;
        for capability in snapshot.capabilities {
            self.push(DriveEvent::CapabilityObserved {
                capability,
                generation: snapshot.generation,
            })?;
        }
        let missing = missing_evidence(&self.state.gates, &self.state.capabilities);
        self.push(DriveEvent::PreflightCompleted {
            missing,
            generation: snapshot.generation,
        })?;
        self.witnesses()
    }

    pub fn step(
        &mut self,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        let field = self.state.field_position(&self.roster, &self.policy)?;
        let decision = playbook_routing::select(&field, &self.policy)?;
        self.run_decision(decision, plane, at_ms)
    }

    pub fn try_operation(
        &mut self,
        operation: &str,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        let definition = self
            .policy
            .play(operation)
            .cloned()
            .ok_or(PlaybookError::NoPlay)?;
        let personnel = self
            .roster
            .iter()
            .find(|person| person.kind == definition.personnel_kind)
            .cloned()
            .ok_or(PlaybookError::NoPlay)?;
        let field = self.state.field_position(&self.roster, &self.policy)?;
        playbook_routing::admit(&field, &definition, &personnel.id, &self.policy)?;
        let decision = DecisionRecord {
            policy_version: self.policy.version.clone(),
            input_state_hash: self.state.state_hash()?,
            classification: self.state.down(),
            selected_play: definition.kind,
            selected_operation: definition.operation.clone(),
            selected_personnel: Some(personnel.id.clone()),
            candidate_plays: vec![definition.operation.clone()],
            rejection_reasons: Vec::new(),
            evidence_refs: field.evidence_refs,
        };
        self.run_decision(decision, plane, at_ms)
    }

    pub fn pressure(&mut self, cost: DriveBudget, at_ms: u64) -> Result<(), PlaybookError> {
        self.push(DriveEvent::BudgetConsumed { cost, at_ms })?;
        self.witnesses()
    }

    pub fn punt(&mut self) -> Result<(), PlaybookError> {
        if self.state.down() != Down::FourthDown
            || self.state.disposition() != Some(Disposition::Punt)
        {
            return Err(PlaybookError::PuntNotReady);
        }
        let package = self.state.seal_package(&self.policy)?;
        self.push(DriveEvent::PuntIssued {
            package: Box::new(package),
        })
    }

    pub fn receive(
        &mut self,
        personnel: Personnel,
        budget: DriveBudget,
        at_ms: u64,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::PossessionStarted {
            personnel,
            budget,
            at_ms,
        })?;
        self.witnesses()
    }

    pub fn record_assessments(&mut self, personnel: Vec<Personnel>) -> Result<(), PlaybookError> {
        for person in &personnel {
            if let Some(slot) = self
                .roster
                .iter_mut()
                .find(|existing| existing.id == person.id)
            {
                *slot = person.clone();
            } else {
                self.roster.push(person.clone());
            }
        }
        self.push(DriveEvent::AssessmentsRecorded { personnel })
    }

    /// Seed this drive with history learned by earlier drives. The loaded
    /// table joins the event log, so replays reproduce it exactly.
    pub fn load_priors(
        &mut self,
        priors: playbook_core::HistoricalPriorTable,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::PriorsLoaded { priors })
    }

    pub fn accept_evidence(&mut self, evidence: Evidence) -> Result<(), PlaybookError> {
        self.push(DriveEvent::EvidenceAccepted { evidence })?;
        self.witnesses()
    }

    pub fn claim_complete(&mut self, at_ms: u64) -> Result<bool, PlaybookError> {
        if is_complete(self.state.gates()) && self.state.ytg_milli() == 0 {
            self.push(DriveEvent::DriveCompleted { at_ms })?;
            Ok(true)
        } else {
            self.push(DriveEvent::CompletionRejected {
                at_ms,
                reason: "mandatory gates are still open".into(),
            })?;
            Ok(false)
        }
    }

    pub fn waive(
        &mut self,
        gate_id: playbook_core::GateId,
        authority: &str,
        reason: &str,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::WaiverGranted {
            gate_id,
            authority: authority.to_string(),
            reason: reason.to_string(),
        })?;
        self.witnesses()
    }

    pub fn select_field_goal(
        &mut self,
        gate_id: playbook_core::GateId,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::FieldGoalSelected { gate_id })
    }

    pub fn abandon(&mut self, reason: &str) -> Result<(), PlaybookError> {
        self.push(DriveEvent::AbandonIssued {
            reason: reason.to_string(),
        })
    }

    pub fn produce_artefact(
        &mut self,
        artefact: playbook_core::ArtefactRef,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::ArtefactProduced { artefact })
    }

    pub fn validate_artefact(
        &mut self,
        artefact_id: playbook_core::ArtefactId,
        evidence_id: playbook_core::EvidenceId,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::ArtefactValidated {
            artefact_id,
            evidence_id,
        })
    }

    pub fn reject_artefact(
        &mut self,
        artefact_id: playbook_core::ArtefactId,
        evidence_id: playbook_core::EvidenceId,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::ArtefactRejected {
            artefact_id,
            evidence_id,
        })?;
        self.witnesses()
    }

    pub fn reconcile(&mut self, key: &str, occurred: bool) -> Result<(), PlaybookError> {
        self.push(DriveEvent::SideEffectReconciled {
            key: key.to_string(),
            occurred,
        })
    }

    fn run_decision(
        &mut self,
        decision: DecisionRecord,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        let mark = self.events.len();
        let saved = self.state.clone();
        match self.commit_decision(decision, plane, at_ms) {
            Ok(record) => Ok(record),
            Err(err) => {
                self.events.truncate(mark);
                self.state = saved;
                Err(err)
            }
        }
    }

    fn commit_decision(
        &mut self,
        decision: DecisionRecord,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        let definition = self
            .policy
            .play(&decision.selected_operation)
            .cloned()
            .ok_or(PlaybookError::NoPlay)?;
        let personnel_id = decision
            .selected_personnel
            .clone()
            .ok_or(PlaybookError::NoPlay)?;
        let personnel = self
            .roster
            .iter()
            .find(|person| person.id == personnel_id)
            .cloned()
            .ok_or(PlaybookError::NoPlay)?;
        self.push(DriveEvent::PlaySelected {
            decision: Box::new(decision.clone()),
        })?;
        if self.state.personnel().map(|person| &person.id) != Some(&personnel.id) {
            self.push(DriveEvent::PersonnelChanged {
                personnel: personnel.clone(),
            })?;
        }
        let execution = PlayExecution {
            drive_id: self.state.drive_id().clone(),
            play_id: PlayId::new(format!(
                "play-{}-{}-{}",
                self.state.possession_generation,
                self.state.attempts,
                definition.operation.replace('.', "-")
            ))?,
            play_kind: definition.kind,
            operation: definition.operation.clone(),
            down: self.state.down(),
            capability_id: playbook_core::CapabilityId::new(&definition.capability_id)?,
            capability_generation: self.state.capability_generation,
            personnel_id: personnel.id.clone(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![definition.evidence_class],
            expected_ytg_reduction_milli: definition.expected_ytg_reduction_milli,
            survey: definition.survey,
            budget_ceiling: self
                .state
                .budget_initial()
                .remaining(self.state.budget_consumed()),
            timeout_ms: at_ms,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: definition.side_effect,
            idempotency_key: Some(format!(
                "{}:{}:{}:{}",
                self.state.drive_id(),
                definition.operation,
                self.state.capability_generation,
                self.state.attempts
            )),
        };
        self.push(DriveEvent::ToolInvoked {
            execution: execution.clone(),
        })?;
        match plane.execute(&execution)? {
            PlaneResult::Done(result) => {
                self.push(DriveEvent::ToolSucceeded {
                    result: Box::new(result),
                })?;
            }
            PlaneResult::Failed(failure) => {
                self.push(DriveEvent::ToolFailed { failure })?;
            }
        }
        self.witnesses()?;
        Ok(decision)
    }

    fn witnesses(&mut self) -> Result<(), PlaybookError> {
        self.push(DriveEvent::YtgRecalculated {
            ytg_milli: self.state.ytg_milli(),
            previous_milli: self.state.cycle_previous_ytg(),
        })?;
        self.push(DriveEvent::DownChanged {
            down: self.state.down(),
            reasons: self.state.explanation().to_vec(),
        })?;
        if self.state.down() == Down::FourthDown {
            let disposition = self
                .state
                .disposition()
                .ok_or(PlaybookError::DispositionMismatch)?;
            self.push(DriveEvent::FourthDownDeclared {
                probability_ppm: self.state.completion_ppm(),
                threshold_ppm: self
                    .policy
                    .conversion_threshold(&self.state.task().workload_class),
                disposition,
            })?;
        }
        if self.state.down() == Down::ThirdDownPlayAction {
            if let Some(requirement) = self.state.requirement().cloned() {
                self.push(DriveEvent::CapabilityRequirementIssued { requirement })?;
            }
        }
        Ok(())
    }

    fn push(&mut self, event: DriveEvent) -> Result<(), PlaybookError> {
        let next = apply(Some(&self.state), &event, &self.policy, ApplyMode::Audit)?;
        self.events.push(event);
        self.state = next;
        Ok(())
    }
}

/// Drive creation is the one event applied without a prior state.
pub fn started_budget(tokens: u64, quota: u64) -> DriveBudget {
    DriveBudget {
        tokens,
        monetary_cost_micros: 1_000_000,
        wall_clock_ms: 60_000,
        tool_calls: 100,
        inference_calls: 20,
        retry_count: 5,
        context_capacity: 100_000,
        provider_quota: quota,
    }
}
