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

    /// Restore an authority from its validated event history. The caller is
    /// responsible for durably saving `ToolInvoked` before calling the plane;
    /// this in-memory type cannot provide a write-ahead storage guarantee.
    pub fn resume(
        events: Vec<DriveEvent>,
        policy: Policy,
        roster: Vec<Personnel>,
    ) -> Result<Self, PlaybookError> {
        let state = crate::fold(&events, &policy, crate::ApplyMode::Audit)?;
        Ok(Self {
            state,
            events,
            policy,
            roster,
        })
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
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
        self.push(DriveEvent::FieldGoalSelected { gate_id })
    }

    pub fn abandon(&mut self, reason: &str) -> Result<(), PlaybookError> {
        self.ensure_no_open_execution()?;
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
        self.ensure_no_open_execution()?;
        let mark = self.events.len();
        let saved = self.state.clone();
        match self.commit_decision(decision, plane, at_ms) {
            Ok(record) => Ok(record),
            Err(err) => {
                let invocation_recorded = self.events[mark..]
                    .iter()
                    .any(|event| matches!(event, DriveEvent::ToolInvoked { .. }));
                if !invocation_recorded {
                    self.events.truncate(mark);
                    self.state = saved;
                }
                Err(err)
            }
        }
    }

    fn ensure_no_open_execution(&self) -> Result<(), PlaybookError> {
        if let Some(execution) = &self.state.open_execution {
            return Err(PlaybookError::Reconcile(
                execution
                    .idempotency_key
                    .clone()
                    .unwrap_or_else(|| execution.play_id.to_string()),
            ));
        }
        if let Some(effect) = self.state.unresolved_effects.first() {
            return Err(PlaybookError::Reconcile(effect.idempotency_key.clone()));
        }
        Ok(())
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
        let outcome = match plane.execute(&execution) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.push(DriveEvent::ToolOutcomeRejected {
                    result: None,
                    failure: None,
                    reason: error.to_string(),
                })?;
                return Err(error);
            }
        };
        match outcome {
            PlaneResult::Done(result) => {
                if let Err(error) = self.push(DriveEvent::ToolSucceeded {
                    result: Box::new(result.clone()),
                }) {
                    self.push(DriveEvent::ToolOutcomeRejected {
                        result: Some(Box::new(result)),
                        failure: None,
                        reason: error.to_string(),
                    })?;
                    return Err(error);
                }
            }
            PlaneResult::Failed(failure) => {
                if let Err(error) = self.push(DriveEvent::ToolFailed {
                    failure: failure.clone(),
                }) {
                    self.push(DriveEvent::ToolOutcomeRejected {
                        result: None,
                        failure: Some(failure),
                        reason: error.to_string(),
                    })?;
                    return Err(error);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{
        EvidenceClass, Gate, GateId, PersonnelId, PersonnelKind, PlayKind, SideEffectClass,
        SideEffectNote, TaskContract, TaskIntegrity, ToolFailure, ToolFailureKind,
    };
    use playbook_elci::{CapabilitySnapshot, FixturePlane};
    use serde_json::json;

    #[derive(Clone, Copy)]
    enum ResponseMode {
        Error,
        InvalidEvidence,
        OverBudget,
        MismatchedPlayId,
        MismatchedOperation,
        UnresolvedEffect,
        MismatchedEffectKey,
        WrongFailurePlayKind,
    }

    struct MutatingPlane {
        inner: FixturePlane,
        mode: ResponseMode,
        mutations: usize,
    }

    impl CapabilityPlane for MutatingPlane {
        fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
            self.inner.discover()
        }

        fn execute(&mut self, execution: &PlayExecution) -> Result<PlaneResult, PlaybookError> {
            if execution.operation != "compile" {
                return self.inner.execute(execution);
            }
            self.mutations += 1;
            if matches!(self.mode, ResponseMode::Error) {
                return Err(PlaybookError::NoPlay);
            }
            if matches!(self.mode, ResponseMode::WrongFailurePlayKind) {
                return Ok(PlaneResult::Failed(ToolFailure {
                    play_id: execution.play_id.clone(),
                    play_kind: PlayKind::DiscoverCapabilities,
                    operation: execution.operation.clone(),
                    capability_id: execution.capability_id.clone(),
                    personnel_id: execution.personnel_id.clone(),
                    kind: ToolFailureKind::Unknown,
                    diagnostics: vec!["wrong play kind".into()],
                    cost: DriveBudget::zero(),
                    at_ms: execution.timeout_ms,
                    idempotency_key: execution.idempotency_key.clone(),
                    side_effects: Vec::new(),
                }));
            }
            let PlaneResult::Done(mut result) = self.inner.execute(execution)? else {
                panic!("fixture compile should return a result")
            };
            match self.mode {
                ResponseMode::Error => unreachable!(),
                ResponseMode::InvalidEvidence => {
                    result.evidence[0].body["success"] = json!(false);
                    let key = execution.idempotency_key.clone().unwrap();
                    result.side_effects.push(SideEffectNote {
                        class: SideEffectClass::RepositoryMutation,
                        idempotency_key: key,
                        summary: "repository was changed".into(),
                        resolved: false,
                        occurred: None,
                    });
                }
                ResponseMode::OverBudget => {
                    result.cost.tokens = execution.budget_ceiling.tokens.saturating_add(25);
                }
                ResponseMode::MismatchedPlayId => {
                    result.play_id = playbook_core::PlayId::new("foreign-play").unwrap();
                }
                ResponseMode::MismatchedOperation => result.operation = "test.unit".into(),
                ResponseMode::UnresolvedEffect | ResponseMode::MismatchedEffectKey => {
                    result.side_effects.push(SideEffectNote {
                        class: SideEffectClass::RemoteMutation,
                        idempotency_key: if matches!(self.mode, ResponseMode::UnresolvedEffect) {
                            execution.idempotency_key.clone().unwrap()
                        } else {
                            "foreign-effect-key".into()
                        },
                        summary: "remote mutation may have occurred".into(),
                        resolved: false,
                        occurred: None,
                    });
                }
                ResponseMode::WrongFailurePlayKind => unreachable!(),
            }
            Ok(PlaneResult::Done(result))
        }
    }

    fn ready_authority() -> (Authority, MutatingPlane) {
        let policy = Policy::builtin();
        let personnel = Personnel {
            id: PersonnelId::new("cargo-toolchain").unwrap(),
            kind: PersonnelKind::DeterministicProcedure,
            display_name: "cargo-toolchain".into(),
            cost_tokens: 40,
            latency_ms: 20,
            capability_fit_ppm: playbook_core::PPM_SCALE,
            reliability_ppm: 900_000,
            quota_remaining_ppm: playbook_core::PPM_SCALE,
            evidence_quality_ppm: playbook_core::PPM_SCALE,
            own_ytg_milli: None,
        };
        let mut authority = Authority::start(
            playbook_core::DriveId::new("uncertain-execution").unwrap(),
            TaskContract {
                statement: "compile implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            started_budget(1_000, 1_000),
            policy,
            vec![personnel],
            1,
        )
        .unwrap();
        authority
            .declare_gate(
                Gate::declare(
                    GateId::new("implementation").unwrap(),
                    "compiler accepts the implementation",
                    1_000_000,
                    Vec::new(),
                    vec![EvidenceClass::CompilerResult],
                    true,
                )
                .unwrap(),
            )
            .unwrap();
        let mut plane = MutatingPlane {
            inner: FixturePlane::mvp(),
            mode: ResponseMode::Error,
            mutations: 0,
        };
        authority.preflight(&mut plane).unwrap();
        authority.step(&mut plane, 100).unwrap();
        (authority, plane)
    }

    fn compile(authority: &mut Authority, plane: &mut MutatingPlane) -> PlaybookError {
        authority.try_operation("compile", plane, 200).unwrap_err()
    }

    #[test]
    fn plane_error_preserves_open_invocation_and_blocks_reexecution() {
        let (mut authority, mut plane) = ready_authority();
        let original_log_len = authority.log().len();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        assert_eq!(plane.mutations, 1);
        assert!(authority.log()[original_log_len..]
            .iter()
            .any(|event| matches!(event, DriveEvent::ToolInvoked { .. })));
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: None,
                failure: None,
                ..
            })
        ));
        let rejected = authority.log().last().unwrap();
        assert!(matches!(
            crate::apply(
                Some(authority.state()),
                rejected,
                authority.policy(),
                crate::ApplyMode::Audit
            ),
            Err(PlaybookError::Reconcile(_))
        ));
        let invocation = authority
            .log()
            .iter()
            .find(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
            .unwrap();
        assert!(matches!(
            crate::apply(
                Some(authority.state()),
                invocation,
                authority.policy(),
                crate::ApplyMode::Audit
            ),
            Err(PlaybookError::Reconcile(_))
        ));
        let invocation_index = authority
            .log()
            .iter()
            .position(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
            .unwrap();
        let before_invocation = crate::fold(
            &authority.log()[..invocation_index],
            authority.policy(),
            crate::ApplyMode::Audit,
        )
        .unwrap();
        let mut missing_key_invocation = invocation.clone();
        let DriveEvent::ToolInvoked { execution } = &mut missing_key_invocation else {
            unreachable!()
        };
        execution.idempotency_key = None;
        assert!(matches!(
            crate::apply(
                Some(&before_invocation),
                &missing_key_invocation,
                authority.policy(),
                crate::ApplyMode::Audit
            ),
            Err(PlaybookError::Inadmissible(_, _))
        ));
        assert!(matches!(
            authority.claim_complete(202),
            Err(PlaybookError::Reconcile(_))
        ));

        let again = authority
            .try_operation("compile", &mut plane, 201)
            .unwrap_err();
        assert!(matches!(again, PlaybookError::Reconcile(_)));
        assert_eq!(plane.mutations, 1);
    }

    #[test]
    fn pending_execution_blocks_terminal_and_handoff_events() {
        let (mut authority, mut plane) = ready_authority();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        let state = authority.state();
        let package = state.seal_package(authority.policy()).unwrap();
        let events = [
            DriveEvent::PuntIssued {
                package: Box::new(package),
            },
            DriveEvent::PossessionStarted {
                personnel: authority.roster[0].clone(),
                budget: started_budget(1_000, 1_000),
                at_ms: 202,
            },
            DriveEvent::FieldGoalSelected {
                gate_id: GateId::new("implementation").unwrap(),
            },
            DriveEvent::DriveCompleted { at_ms: 202 },
            DriveEvent::AbandonIssued {
                reason: "handoff".into(),
            },
        ];
        for event in &events {
            assert!(matches!(
                crate::apply(
                    Some(authority.state()),
                    event,
                    authority.policy(),
                    crate::ApplyMode::Audit
                ),
                Err(PlaybookError::Reconcile(_))
            ));
        }
        assert!(matches!(authority.punt(), Err(PlaybookError::Reconcile(_))));
        assert!(matches!(
            authority.receive(
                authority.roster[0].clone(),
                started_budget(1_000, 1_000),
                202
            ),
            Err(PlaybookError::Reconcile(_))
        ));
        assert!(matches!(
            authority.select_field_goal(GateId::new("implementation").unwrap()),
            Err(PlaybookError::Reconcile(_))
        ));
    }

    #[test]
    fn rejected_invalid_evidence_keeps_observed_cost_and_effects_for_reconciliation() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::InvalidEvidence;
        assert_eq!(
            compile(&mut authority, &mut plane),
            PlaybookError::DigestMismatch
        );
        assert_eq!(authority.state.budget_consumed().tokens, 220);
        assert_eq!(authority.state.unresolved_effects.len(), 1);
        assert_eq!(
            authority.state.unresolved_effects[0].class,
            SideEffectClass::RepositoryMutation
        );
        assert_eq!(
            authority.state.idempotency.values().next(),
            Some(&playbook_core::IdempotencyState::Unknown)
        );

        let key = authority
            .state
            .open_execution
            .as_ref()
            .unwrap()
            .idempotency_key
            .clone()
            .unwrap();
        let events = authority.log().to_vec();
        let mut recovered =
            Authority::resume(events, authority.policy.clone(), authority.roster.clone()).unwrap();
        assert!(recovered.state.open_execution.is_some());
        recovered.reconcile(&key, true).unwrap();
        assert!(recovered.state.open_execution.is_none());
        assert!(recovered.state.unresolved_effects.is_empty());
    }

    #[test]
    fn resume_detects_crash_open_invocation_and_reconciliation_closes_it() {
        let (mut authority, mut plane) = ready_authority();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        let invoke_end = authority
            .log()
            .iter()
            .position(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
            .unwrap()
            + 1;
        let crash_log = authority.log()[..invoke_end].to_vec();
        let execution = match crash_log.last().unwrap() {
            DriveEvent::ToolInvoked { execution } => execution,
            _ => unreachable!(),
        };
        let key = execution.idempotency_key.clone().unwrap();
        let mut resumed = Authority::resume(
            crash_log,
            authority.policy.clone(),
            authority.roster.clone(),
        )
        .unwrap();
        assert!(resumed.state.open_execution.is_some());
        assert!(matches!(
            resumed.try_operation("compile", &mut plane, 201),
            Err(PlaybookError::Reconcile(_))
        ));
        resumed.reconcile(&key, false).unwrap();
        assert!(resumed.state.open_execution.is_none());
    }

    #[test]
    fn rejected_over_budget_response_records_actual_cost_even_past_allocation() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::OverBudget;
        assert_eq!(
            compile(&mut authority, &mut plane),
            PlaybookError::BudgetCeiling
        );
        assert_eq!(authority.state.budget_consumed().tokens, 1_025);
        assert_eq!(authority.state.budget_remaining_ppm(authority.policy()), 0);
        assert!(authority.state.open_execution.is_some());
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: Some(_),
                failure: None,
                ..
            })
        ));
    }

    #[test]
    fn rejected_mismatched_play_id_is_retained_as_an_observation() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::MismatchedPlayId;
        assert!(matches!(
            compile(&mut authority, &mut plane),
            PlaybookError::Inadmissible(_, _)
        ));
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: Some(result),
                ..
            }) if result.play_id.as_str() == "foreign-play"
        ));
        assert_eq!(authority.state.budget_consumed().tokens, 220);
    }

    #[test]
    fn rejected_mismatched_operation_is_retained_as_an_observation() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::MismatchedOperation;
        assert!(matches!(
            compile(&mut authority, &mut plane),
            PlaybookError::Inadmissible(_, _)
        ));
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: Some(result),
                ..
            }) if result.operation == "test.unit"
        ));
        assert_eq!(authority.state.budget_consumed().tokens, 220);
    }

    #[test]
    fn accepted_success_with_unresolved_effect_blocks_completion_until_reconciled() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::UnresolvedEffect;
        authority.try_operation("compile", &mut plane, 200).unwrap();
        assert_eq!(authority.state.ytg_milli(), 0);
        assert_eq!(authority.state.unresolved_effects.len(), 1);
        let key = authority.state.unresolved_effects[0]
            .idempotency_key
            .clone();
        assert!(matches!(
            authority.claim_complete(201),
            Err(PlaybookError::Reconcile(_))
        ));

        authority.reconcile(&key, true).unwrap();
        assert!(authority.state.unresolved_effects.is_empty());
        assert!(authority.claim_complete(202).unwrap());
    }

    #[test]
    fn mismatched_effect_key_is_preserved_and_reconciled_under_invocation_key() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::MismatchedEffectKey;
        assert!(matches!(
            compile(&mut authority, &mut plane),
            PlaybookError::Inadmissible(_, _)
        ));
        let key = authority
            .state
            .open_execution
            .as_ref()
            .unwrap()
            .idempotency_key
            .clone()
            .unwrap();
        assert_eq!(authority.state.unresolved_effects[0].idempotency_key, key);
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: Some(result),
                ..
            }) if result.side_effects[0].idempotency_key == "foreign-effect-key"
        ));
        authority.reconcile(&key, true).unwrap();
        assert!(authority.state.unresolved_effects.is_empty());
    }

    #[test]
    fn failure_with_wrong_play_kind_is_rejected_and_preserved() {
        let (mut authority, mut plane) = ready_authority();
        plane.mode = ResponseMode::WrongFailurePlayKind;
        assert!(matches!(
            compile(&mut authority, &mut plane),
            PlaybookError::Inadmissible(_, _)
        ));
        assert!(matches!(
            authority.log().last(),
            Some(DriveEvent::ToolOutcomeRejected {
                failure: Some(failure),
                ..
            }) if failure.play_kind == PlayKind::DiscoverCapabilities
        ));
    }

    #[test]
    fn tampered_invocation_contract_is_rejected_by_audit_fold() {
        let (mut authority, mut plane) = ready_authority();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        let invoke_index = authority
            .log()
            .iter()
            .rposition(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
            .unwrap();
        let before = crate::fold(
            &authority.log()[..invoke_index],
            authority.policy(),
            crate::ApplyMode::Audit,
        )
        .unwrap();
        let DriveEvent::ToolInvoked { execution } = &authority.log()[invoke_index] else {
            unreachable!()
        };
        let original = execution.clone();
        let policy = authority.policy();
        let rejected = |execution: PlayExecution| {
            let event = DriveEvent::ToolInvoked { execution };
            let error =
                crate::apply(Some(&before), &event, policy, crate::ApplyMode::Audit).unwrap_err();
            assert!(
                matches!(error, PlaybookError::Inadmissible(_, _)),
                "{error:?}"
            );
        };

        let mut tampered = original.clone();
        tampered.drive_id = playbook_core::DriveId::new("other-drive").unwrap();
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.play_kind = PlayKind::DiscoverCapabilities;
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.down = playbook_core::Down::SecondDownGain;
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.capability_id = playbook_core::CapabilityId::new("elci.other").unwrap();
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.side_effect = SideEffectClass::LocalMutation;
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.expected_evidence = vec![if original.expected_evidence.first()
            == Some(&EvidenceClass::EnvironmentSnapshot)
        {
            EvidenceClass::CompilerResult
        } else {
            EvidenceClass::EnvironmentSnapshot
        }];
        rejected(tampered);
        let mut tampered = original.clone();
        tampered.expected_ytg_reduction_milli =
            original.expected_ytg_reduction_milli.saturating_add(1);
        rejected(tampered);
        let mut tampered = original;
        tampered.survey = !tampered.survey;
        rejected(tampered);
    }
}
