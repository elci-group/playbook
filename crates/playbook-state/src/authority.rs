//! Single writer for a drive.
//!
//! Workers return evidence. This authority commits it. Parallel agents do not
//! mutate the drive struct themselves.

use playbook_core::{
    DecisionRecord, Disposition, Down, DriveBudget, DriveId, EventId, Evidence, EvidenceId, Gate,
    HuddleId, Personnel, PersonnelId, PlayExecution, PlayId, PlaybookError, RetryPolicy,
    TaskContract,
};
use playbook_elci::{missing_evidence, CapabilityPlane, HuddleDispatch, PlaneResult};
use playbook_events::DriveEvent;
use playbook_gates::is_complete;
use playbook_huddle::{
    admission, detect_contradictions, escalation_target, forced_synthesis,
    participants as huddle_participants, Assignment, DefaultRegulator, HuddleCost, HuddleOutcome,
    HuddleParticipant, HuddleRegulator, HuddleReport, HuddleScope, HuddleTrigger,
    HuddleTriggerKind, HuddleValue, ParticipantCandidate,
};
use playbook_policy::Policy;

use crate::apply::ApplyMode;
use crate::{apply, DriveState, EventSink};

/// Inputs for opening a huddle through the authority. The authority owns
/// admission economics and participant selection; callers describe the
/// trigger and the relevance of the roster.
pub struct HuddleRequest {
    pub scope: HuddleScope,
    pub trigger_kind: HuddleTriggerKind,
    pub reason_code: String,
    pub evidence_refs: Vec<EvidenceId>,
    pub manual: bool,
    pub value: HuddleValue,
    pub candidates: Vec<ParticipantCandidate>,
    /// The caller's coherence-debt reading at request time.
    pub debt: u32,
    /// The caller's estimated rework-avoided value.
    pub rework_avoided: u32,
}

/// The result of one ELCI-backed collection pass. Failed participants are
/// isolated: their reports are absent from the journal, deterministically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionSummary {
    pub collected: Vec<PersonnelId>,
    pub failed: Vec<(PersonnelId, String)>,
    pub synthesized: bool,
}

/// The live coherence reading an orchestrator monitors: the measured debt
/// state and the trigger currently firing on it, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoherenceReading {
    pub state: playbook_huddle::CoherenceState,
    pub condition: Option<(HuddleScope, HuddleTriggerKind, String)>,
}

impl HuddleRequest {
    /// Derive a request from an HTTP body: every roster member becomes a
    /// candidate with neutral relevance, and selection still applies the
    /// policy floor, the envelope cap, and measured disclosure profiles.
    pub #[tracing::instrument]
fn from_body(body: playbook_huddle::HuddleRequestBody, roster: &[Personnel]) -> Self {
        Self {
            scope: body.scope,
            trigger_kind: body.trigger_kind,
            reason_code: body.reason_code,
            evidence_refs: body.evidence_refs,
            manual: body.manual,
            value: body.value,
            debt: body.debt,
            rework_avoided: body.rework_avoided,
            candidates: roster
                .iter()
                .map(|person| ParticipantCandidate {
                    personnel_id: person.id.clone(),
                    dependency_overlap: 500,
                    contradiction_relevance: 500,
                    ownership_weight: 500,
                    downstream_impact: 500,
                    specialist_value: 500,
                    context_cost: 10,
                })
                .collect(),
        }
    }
}

/// The result of running admission on a requested huddle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HuddleAdmission {
    Open(HuddleId),
    Refused {
        huddle_id: HuddleId,
        reason_code: String,
    },
}

pub struct Authority {
    state: DriveState,
    events: Vec<DriveEvent>,
    policy: Policy,
    roster: Vec<Personnel>,
    journal: Option<Box<dyn EventSink>>,
}

impl Authority {
    pub #[tracing::instrument]
fn start(
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
            journal: None,
        };
        authority.witnesses()?;
        Ok(authority)
    }

    /// Start a drive with a durable journal. Every event is journaled before
    /// it joins the log.
    pub #[tracing::instrument]
fn start_journaled(
        drive_id: DriveId,
        task: TaskContract,
        budget: DriveBudget,
        policy: Policy,
        roster: Vec<Personnel>,
        at_ms: u64,
        mut journal: Box<dyn EventSink>,
    ) -> Result<Self, PlaybookError> {
        let event = DriveEvent::DriveStarted {
            drive_id,
            task,
            budget,
            at_ms,
        };
        let state = apply(None, &event, &policy, ApplyMode::Audit)?;
        journal.record(&event)?;
        let mut authority = Self {
            state,
            events: vec![event],
            policy,
            roster,
            journal: Some(journal),
        };
        authority.witnesses()?;
        Ok(authority)
    }

    /// Restore an authority from its validated event history. The caller is
    /// responsible for durably saving `ToolInvoked` before calling the plane;
    /// attach a durable sink (see `attach_journal`) to obtain the write-ahead
    /// guarantee.
    pub #[tracing::instrument]
fn resume(
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
            journal: None,
        })
    }

    /// Attach a durable sink, intended for `Authority::resume`. Events already
    /// in the log are not retro-journaled.
    pub #[tracing::instrument]
fn attach_journal(&mut self, journal: Box<dyn EventSink>) {
        self.journal = Some(journal);
    }

    pub #[tracing::instrument]
fn state(&self) -> &DriveState {
        &self.state
    }

    pub #[tracing::instrument]
fn log(&self) -> &[DriveEvent] {
        &self.events
    }

    pub #[tracing::instrument]
fn policy(&self) -> &Policy {
        &self.policy
    }

    pub #[tracing::instrument]
fn roster(&self) -> &[Personnel] {
        &self.roster
    }

    pub #[tracing::instrument]
fn declare_gate(&mut self, gate: Gate) -> Result<(), PlaybookError> {
        self.push(DriveEvent::GateDeclared { gate })?;
        self.witnesses()
    }

    pub #[tracing::instrument]
fn preflight(&mut self, plane: &mut dyn CapabilityPlane) -> Result<(), PlaybookError> {
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

    pub #[tracing::instrument]
fn step(
        &mut self,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        self.ensure_no_open_execution()?;
        let field = self.state.field_position(&self.roster, &self.policy)?;
        let decision = playbook_routing::select(&field, &self.policy)?;
        self.run_decision(decision, plane, at_ms)
    }

    pub #[tracing::instrument]
fn try_operation(
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

    pub #[tracing::instrument]
fn pressure(&mut self, cost: DriveBudget, at_ms: u64) -> Result<(), PlaybookError> {
        self.push(DriveEvent::BudgetConsumed { cost, at_ms })?;
        self.witnesses()
    }

    pub #[tracing::instrument]
fn punt(&mut self) -> Result<(), PlaybookError> {
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

    pub #[tracing::instrument]
fn receive(
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

    pub #[tracing::instrument]
fn record_assessments(&mut self, personnel: Vec<Personnel>) -> Result<(), PlaybookError> {
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
    pub #[tracing::instrument]
fn load_priors(
        &mut self,
        priors: playbook_core::HistoricalPriorTable,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::PriorsLoaded { priors })
    }

    pub #[tracing::instrument]
fn accept_evidence(&mut self, evidence: Evidence) -> Result<(), PlaybookError> {
        self.push(DriveEvent::EvidenceAccepted { evidence })?;
        self.witnesses()
    }

    pub #[tracing::instrument]
fn claim_complete(&mut self, at_ms: u64) -> Result<bool, PlaybookError> {
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

    pub #[tracing::instrument]
fn waive(
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

    pub #[tracing::instrument]
fn select_field_goal(
        &mut self,
        gate_id: playbook_core::GateId,
    ) -> Result<(), PlaybookError> {
        self.ensure_no_open_execution()?;
        self.push(DriveEvent::FieldGoalSelected { gate_id })
    }

    pub #[tracing::instrument]
fn abandon(&mut self, reason: &str) -> Result<(), PlaybookError> {
        self.ensure_no_open_execution()?;
        self.push(DriveEvent::AbandonIssued {
            reason: reason.to_string(),
        })
    }

    pub #[tracing::instrument]
fn produce_artefact(
        &mut self,
        artefact: playbook_core::ArtefactRef,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::ArtefactProduced { artefact })
    }

    pub #[tracing::instrument]
fn validate_artefact(
        &mut self,
        artefact_id: playbook_core::ArtefactId,
        evidence_id: playbook_core::EvidenceId,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::ArtefactValidated {
            artefact_id,
            evidence_id,
        })
    }

    pub #[tracing::instrument]
fn reject_artefact(
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

    pub #[tracing::instrument]
fn reconcile(&mut self, key: &str, occurred: bool) -> Result<(), PlaybookError> {
        self.push(DriveEvent::SideEffectReconciled {
            key: key.to_string(),
            occurred,
        })
    }

    /// Request, admit, and open a huddle in one tactical step. Admission
    /// economics and participant selection run here, deterministically; a
    /// refusal is journaled like any other event.
    pub #[tracing::instrument]
fn open_huddle(
        &mut self,
        request: HuddleRequest,
        at_ms: u64,
    ) -> Result<HuddleAdmission, PlaybookError> {
        let huddle_id = HuddleId::new(format!(
            "huddle-{}",
            self.state.huddles().requested_total().saturating_add(1)
        ))?;
        let originating_event = EventId::new(format!("event-{}", self.events.len()))?;
        let trigger = HuddleTrigger {
            kind: request.trigger_kind,
            reason_code: request.reason_code.clone(),
            evidence_refs: request.evidence_refs.clone(),
            originating_event,
        };
        // Admission runs against the field as it stands, before the request
        // itself is journaled, so the economics never observe their own
        // candidate.
        let field_fingerprint = self.state.fingerprint();
        let remaining_drive_budget = self
            .state
            .budget_initial()
            .remaining(self.state.budget_consumed());
        let decision = admission::decide(
            &admission::AdmissionInputs {
                scope: request.scope,
                trigger: &trigger,
                debt: 0,
                field_fingerprint: &field_fingerprint,
                ledger: self.state.huddles(),
                value: request.value,
                remaining_drive_budget,
                manual: request.manual,
            },
            &self.policy.huddle,
        )?;
        self.push(DriveEvent::HuddleRequested {
            huddle_id: huddle_id.clone(),
            scope: request.scope,
            trigger: trigger.clone(),
            at_ms,
            debt: request.debt,
            rework_avoided: request.rework_avoided,
        })?;
        match decision {
            admission::AdmissionDecision::Admit { budget, barrier } => {
                let participants =
                    self.select_huddle_participants(&huddle_id, &trigger, &request.candidates)?;
                self.push(DriveEvent::HuddleAdmitted {
                    huddle_id: huddle_id.clone(),
                    budget,
                    barrier,
                    participants,
                })?;
                self.push(DriveEvent::HuddleOpened {
                    huddle_id: huddle_id.clone(),
                })?;
                Ok(HuddleAdmission::Open(huddle_id))
            }
            admission::AdmissionDecision::Reject { reason_code } => {
                self.push(DriveEvent::HuddleRejected {
                    huddle_id: huddle_id.clone(),
                    reason_code: reason_code.clone(),
                })?;
                Ok(HuddleAdmission::Refused {
                    huddle_id,
                    reason_code,
                })
            }
        }
    }

    #[tracing::instrument]
fn select_huddle_participants(
        &self,
        huddle_id: &HuddleId,
        trigger: &HuddleTrigger,
        candidates: &[ParticipantCandidate],
    ) -> Result<Vec<HuddleParticipant>, PlaybookError> {
        let envelope = self.policy.huddle.budget(admission::scope_key(
            self.state
                .huddles()
                .active_by_id(huddle_id)
                .ok_or(PlaybookError::NoPlay)?
                .scope,
        ));
        let picked = huddle_participants::select(
            candidates,
            &self.policy.huddle.participant_weights,
            envelope.max_participants,
            self.policy.huddle.min_participant_score,
        );
        let regulator = DefaultRegulator;
        picked
            .iter()
            .map(|personnel_id| {
                let person = self
                    .roster
                    .iter()
                    .find(|person| &person.id == personnel_id)
                    .ok_or(PlaybookError::NoPlay)?;
                Ok(HuddleParticipant {
                    personnel_id: personnel_id.clone(),
                    role: huddle_participants::role_for(person),
                    disclosure_profile: regulator.build_disclosure(person, trigger),
                })
            })
            .collect()
    }

    /// Invite one roster member into an open huddle.
    pub #[tracing::instrument]
fn invite_to_huddle(
        &mut self,
        huddle_id: &HuddleId,
        personnel_id: &PersonnelId,
    ) -> Result<(), PlaybookError> {
        let person = self
            .roster
            .iter()
            .find(|person| &person.id == personnel_id)
            .ok_or(PlaybookError::NoPlay)?
            .clone();
        let trigger = self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .ok_or(PlaybookError::NoPlay)?
            .trigger
            .clone();
        let regulator = DefaultRegulator;
        let participant = HuddleParticipant {
            personnel_id: person.id.clone(),
            role: huddle_participants::role_for(&person),
            disclosure_profile: regulator.build_disclosure(&person, &trigger),
        };
        self.push(DriveEvent::HuddleParticipantInvited {
            huddle_id: huddle_id.clone(),
            participant,
        })
    }

    /// Submit one schema-valid report, then journal any contradictions the
    /// accumulated reports expose. Returns the contradictions detected by
    /// this submission.
    pub #[tracing::instrument]
fn submit_huddle_report(
        &mut self,
        huddle_id: &HuddleId,
        report: HuddleReport,
    ) -> Result<Vec<playbook_huddle::Contradiction>, PlaybookError> {
        self.push(DriveEvent::HuddleReportSubmitted {
            huddle_id: huddle_id.clone(),
            report: Box::new(report),
        })?;
        let active = self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .ok_or_else(|| PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into()))?;
        let reports = active.reports.clone();
        let known: Vec<playbook_huddle::Contradiction> = active.contradictions.clone();
        let mut detected = Vec::new();
        for contradiction in detect_contradictions(&reports) {
            match known.iter().find(|item| item.id == contradiction.id) {
                Some(earlier) if earlier.claims.len() >= contradiction.claims.len() => continue,
                _ => {}
            }
            self.push(DriveEvent::HuddleContradictionDetected {
                huddle_id: huddle_id.clone(),
                contradiction: contradiction.clone(),
            })?;
            detected.push(contradiction);
        }
        Ok(detected)
    }

    /// Journal huddle-side consumption against the huddle's own envelope.
    pub #[tracing::instrument]
fn consume_huddle_budget(
        &mut self,
        huddle_id: &HuddleId,
        cost: HuddleCost,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::HuddleBudgetConsumed {
            huddle_id: huddle_id.clone(),
            cost,
        })
    }

    /// Commit a reconciled outcome. The fold validates every contract:
    /// evidence-grounded resolutions, no field rewrites, no unsafe
    /// progression past a blocking contradiction.
    pub #[tracing::instrument]
fn commit_huddle_outcome(
        &mut self,
        huddle_id: &HuddleId,
        outcome: HuddleOutcome,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::HuddleOutcomeCommitted {
            huddle_id: huddle_id.clone(),
            outcome: Box::new(outcome),
        })
    }

    /// Synthesize from accumulated reports after budget exhaustion.
    pub #[tracing::instrument]
fn force_huddle_synthesis(&mut self, huddle_id: &HuddleId) -> Result<(), PlaybookError> {
        let active = self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .ok_or_else(|| PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into()))?
            .clone();
        let mut outcome = forced_synthesis(&active);
        outcome.huddle_id = active.id.clone();
        self.commit_huddle_outcome(huddle_id, outcome)
    }

    /// Phase 4: ELCI-backed participant execution. Dispatch one scoped
    /// context packet per participant through the plane, journal every
    /// schema-valid report, and charge the stated cost against the huddle
    /// envelope. A participant that times out, cancels, or answers with
    /// malformed output simply leaves no report; the pass continues with
    /// the rest, deterministically. Budget exhaustion terminates the pass
    /// and forces synthesis from the accumulated reports.
    pub #[tracing::instrument]
fn collect_huddle_reports(
        &mut self,
        huddle_id: &HuddleId,
        plane: &mut dyn HuddleDispatch,
        report_cost: HuddleCost,
    ) -> Result<CollectionSummary, PlaybookError> {
        let mut summary = CollectionSummary {
            collected: Vec::new(),
            failed: Vec::new(),
            synthesized: false,
        };
        let participants = self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .ok_or_else(|| PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into()))?
            .participants
            .clone();
        let task_fragment = self.state.task().statement.clone();
        let gates = self.state.gates().to_vec();
        let evidence = self.state.evidence.clone();
        let assignments = self.state.huddles().assignments().clone();
        for participant in &participants {
            let Some(active) = self.state.huddles().active_by_id(huddle_id) else {
                break;
            };
            if active.exhausted {
                break;
            }
            let assignment = assignments
                .get(participant.personnel_id.as_str())
                .cloned()
                .unwrap_or_else(|| Assignment {
                    personnel_id: participant.personnel_id.clone(),
                    operation: String::new(),
                    target_gate: None,
                });
            let packet = huddle_participants::build_packet(
                active,
                participant,
                &task_fragment,
                &gates,
                &evidence,
                assignment,
            );
            let report = match huddle_participants::dispatch_packet(plane, &packet) {
                Ok(mut report) => {
                    report.participant = participant.personnel_id.clone();
                    report
                }
                Err(error) => {
                    summary
                        .failed
                        .push((participant.personnel_id.clone(), error.to_string()));
                    continue;
                }
            };
            match self.submit_huddle_report(huddle_id, report) {
                Ok(_) => {
                    self.consume_huddle_budget(huddle_id, report_cost)?;
                    summary.collected.push(participant.personnel_id.clone());
                }
                Err(error) => summary
                    .failed
                    .push((participant.personnel_id.clone(), error.to_string())),
            }
        }
        if self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .is_some_and(|huddle| huddle.exhausted)
        {
            self.force_huddle_synthesis(huddle_id)?;
            summary.synthesized = true;
        }
        Ok(summary)
    }

    /// Live coherence measurement for a hosted drive: the deterministic
    /// debt score with its band, the ledger counters, and the trigger the
    /// engine currently fires on this exact state, if any. This is the
    /// Phase 7 seam an orchestrator monitors. Every component is measured
    /// from the journal and the fold, never reported by callers:
    /// assumption divergence counts subjects whose active reports disagree,
    /// contradiction load counts live contradiction objects, turnover
    /// counts benched personnel, blockers count active report blockers, and
    /// context age counts journal events since the last committed
    /// reconciliation. Components the drive does not yet track (dependency
    /// staleness, cross-unit change impact) read zero; they are never
    /// guessed.
    pub #[tracing::instrument]
fn coherence(&self) -> CoherenceReading {
        let ledger = self.state.huddles();
        let mut divergent_subjects: Vec<&str> = Vec::new();
        let mut blockers = 0u32;
        for huddle in ledger.active() {
            let mut per_subject: std::collections::BTreeMap<&str, Vec<&str>> =
                std::collections::BTreeMap::new();
            for report in &huddle.reports {
                for delta in &report.changed_assumptions {
                    let values = per_subject.entry(delta.subject.as_str()).or_default();
                    if !values.contains(&delta.current.as_str()) {
                        values.push(delta.current.as_str());
                    }
                }
                blockers += report.blockers.len() as u32;
            }
            divergent_subjects.extend(
                per_subject
                    .into_values()
                    .filter(|values| values.len() > 1)
                    .map(|values| values[0]),
            );
        }
        let unresolved: u32 = ledger
            .active()
            .iter()
            .map(|huddle| huddle.contradictions.len() as u32)
            .sum::<u32>()
            + ledger
                .closed()
                .iter()
                .filter_map(|record| record.outcome.as_ref())
                .map(|outcome| outcome.unresolved_contradictions.len() as u32)
                .sum::<u32>();
        let last_reconciliation = self
            .log()
            .iter()
            .rposition(|event| matches!(event, DriveEvent::HuddleOutcomeCommitted { .. }))
            .map(|index| index + 1)
            .unwrap_or(0);
        let inputs = playbook_huddle::CoherenceInputs {
            assumption_divergence: (divergent_subjects.len() as u32 * 500).min(1_000),
            dependency_staleness: 0,
            contradiction_load: (unresolved * 250).min(1_000),
            personnel_turnover: (self.state.benched().len() as u32 * 500).min(1_000),
            cross_unit_change_impact: 0,
            unresolved_blockers: (blockers * 250).min(1_000),
            context_age: (self.log().len() as u32 - last_reconciliation as u32).min(1_000),
        };
        let debt = playbook_huddle::coherence_debt(&inputs, &self.policy.huddle.debt_weights);
        CoherenceReading {
            state: playbook_huddle::coherence_state(ledger, &inputs, &self.policy),
            condition: self.huddle_condition(debt),
        }
    }

    /// The huddle trigger the engine currently fires on this live state.
    #[tracing::instrument]
fn huddle_condition(&self, debt: u32) -> Option<(HuddleScope, HuddleTriggerKind, String)> {
        let log = self.log();
        let last_failure = log
            .iter()
            .rposition(|event| matches!(event, DriveEvent::ToolFailed { .. }));
        let (failed_attempts, field_changed, gate_completed) = match last_failure {
            None => (0, true, false),
            Some(index) => {
                let after = &log[index + 1..];
                let moved = |event: &DriveEvent| {
                    matches!(
                        event,
                        DriveEvent::YtgRecalculated { .. }
                            | DriveEvent::DownChanged { .. }
                            | DriveEvent::FourthDownDeclared { .. }
                            | DriveEvent::FieldGoalSelected { .. }
                            | DriveEvent::PuntIssued { .. }
                            | DriveEvent::PossessionStarted { .. }
                            | DriveEvent::GateDeclared { .. }
                            | DriveEvent::EvidenceAccepted { .. }
                            | DriveEvent::DriveCompleted { .. }
                            | DriveEvent::AbandonIssued { .. }
                    )
                };
                let mut attempts = 0u32;
                for event in log.iter().rev() {
                    match event {
                        DriveEvent::ToolFailed { .. } => attempts += 1,
                        DriveEvent::YtgRecalculated { .. }
                        | DriveEvent::DownChanged { .. }
                        | DriveEvent::FourthDownDeclared { .. } => continue,
                        _ => break,
                    }
                }
                (
                    attempts,
                    after.iter().any(moved),
                    after
                        .iter()
                        .any(|event| matches!(event, DriveEvent::EvidenceAccepted { .. })),
                )
            }
        };
        let trigger = playbook_huddle::evaluate_triggers(
            &playbook_huddle::TriggerInputs {
                failed_attempts,
                field_changed_since_failure: field_changed,
                mandatory_gate_completed: gate_completed,
                dependent_active_personnel: self.roster.len() as u32,
                down: self.state.down(),
                debt,
                evidence_refs: self
                    .state
                    .failure_refs()
                    .last()
                    .cloned()
                    .into_iter()
                    .collect(),
                originating_event: EventId::new(format!("event-{}", log.len())).expect("event id"),
            },
            &self.policy.huddle,
        )?;
        Some((trigger.0, trigger.1.kind, trigger.1.reason_code))
    }

    /// Escalate one scope up the ladder. Returns the new huddle's id; the
    /// child still owes its own admission event.
    pub #[tracing::instrument]
fn escalate_huddle(
        &mut self,
        huddle_id: &HuddleId,
        manual: bool,
        reason_code: &str,
    ) -> Result<HuddleId, PlaybookError> {
        let active = self
            .state
            .huddles()
            .active_by_id(huddle_id)
            .ok_or_else(|| PlaybookError::Inadmissible("huddle".into(), "unknown huddle".into()))?
            .clone();
        let Some(to_scope) = escalation_target(&active, 0, manual, &self.policy.huddle)? else {
            return Err(PlaybookError::Inadmissible(
                "huddle".into(),
                "escalation lacks scope justification".into(),
            ));
        };
        let new_huddle_id = HuddleId::new(format!(
            "huddle-esc-{}",
            self.state.huddles().escalated_total().saturating_add(1)
        ))?;
        self.push(DriveEvent::HuddleEscalationRequested {
            huddle_id: huddle_id.clone(),
            to_scope,
            reason_code: reason_code.to_string(),
        })?;
        self.push(DriveEvent::HuddleEscalated {
            huddle_id: huddle_id.clone(),
            new_huddle_id: new_huddle_id.clone(),
            to_scope,
        })?;
        Ok(new_huddle_id)
    }

    /// Terminate a huddle that cannot reach synthesis.
    pub #[tracing::instrument]
fn close_huddle(
        &mut self,
        huddle_id: &HuddleId,
        reason_code: &str,
    ) -> Result<(), PlaybookError> {
        self.push(DriveEvent::HuddleClosed {
            huddle_id: huddle_id.clone(),
            reason_code: reason_code.to_string(),
        })
    }

    /// Bench review: turn committed substitution recommendations into
    /// journaled Bench verdicts. Benched personnel leave the selection pool
    /// until a later reversed verdict returns them. Returns the number of
    /// new verdicts.
    pub #[tracing::instrument]
fn bench_from_review(&mut self) -> Result<usize, PlaybookError> {
        let pending: Vec<(PersonnelId, String)> = self
            .state
            .huddles()
            .substitutions()
            .iter()
            .filter(|change| {
                change.action == playbook_huddle::PersonnelAction::SubstitutionRecommended
                    && !self.state.benched().contains(&change.personnel_id)
            })
            .map(|change| (change.personnel_id.clone(), change.reason.clone()))
            .collect();
        let mut verdicts = 0;
        for (personnel_id, reason) in pending {
            self.push(DriveEvent::BenchDecision {
                personnel_id,
                benched: true,
                reason_code: reason,
            })?;
            verdicts += 1;
        }
        Ok(verdicts)
    }

    #[tracing::instrument]
fn run_decision(
        &mut self,
        decision: DecisionRecord,
        plane: &mut dyn CapabilityPlane,
        at_ms: u64,
    ) -> Result<DecisionRecord, PlaybookError> {
        self.ensure_no_open_execution()?;
        let mark = self.events.len();
        let journal_mark = self.journal.as_ref().map(|sink| sink.mark());
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
                    if let (Some(sink), Some(mark)) = (self.journal.as_mut(), journal_mark) {
                        sink.rollback(mark)?;
                    }
                }
                Err(err)
            }
        }
    }

    #[tracing::instrument]
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

    #[tracing::instrument]
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

    #[tracing::instrument]
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

    #[tracing::instrument]
fn push(&mut self, event: DriveEvent) -> Result<(), PlaybookError> {
        let next = apply(Some(&self.state), &event, &self.policy, ApplyMode::Audit)?;
        if let Some(sink) = self.journal.as_mut() {
            sink.record(&event)?;
        }
        self.events.push(event);
        self.state = next;
        Ok(())
    }
}

/// Drive creation is the one event applied without a prior state.
pub #[tracing::instrument]
fn started_budget(tokens: u64, quota: u64) -> DriveBudget {
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
    use playbook_elci::{CapabilitySnapshot, FixtureHuddlePlane, FixturePlane};
    use playbook_huddle::{
        AssignmentChange, AssumptionDelta, ContradictionResolution, HuddleClosure, HuddleScope,
        PersonnelAction, PlayProposal, ResolutionState, SharedStateDelta,
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MemorySink {
        events: Arc<Mutex<Vec<DriveEvent>>>,
    }

    impl MemorySink {
        #[tracing::instrument]
fn recorded(&self) -> Vec<DriveEvent> {
            self.events.lock().expect("sink lock").clone()
        }
    }

    impl EventSink for MemorySink {
        #[tracing::instrument]
fn record(&mut self, event: &DriveEvent) -> Result<(), PlaybookError> {
            self.events.lock().expect("sink lock").push(event.clone());
            Ok(())
        }

        #[tracing::instrument]
fn mark(&self) -> u64 {
            self.events.lock().expect("sink lock").len() as u64
        }

        #[tracing::instrument]
fn rollback(&mut self, mark: u64) -> Result<(), PlaybookError> {
            self.events
                .lock()
                .expect("sink lock")
                .truncate(mark as usize);
            Ok(())
        }
    }

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
        #[tracing::instrument]
fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
            self.inner.discover()
        }

        #[tracing::instrument]
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

    #[tracing::instrument]
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

    #[tracing::instrument]
fn compile(authority: &mut Authority, plane: &mut MutatingPlane) -> PlaybookError {
        authority.try_operation("compile", plane, 200).unwrap_err()
    }

    #[tracing::instrument]
fn journaled_authority() -> (Authority, MutatingPlane, MemorySink) {
        let sink = MemorySink::default();
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
        let mut authority = Authority::start_journaled(
            playbook_core::DriveId::new("uncertain-execution").unwrap(),
            TaskContract {
                statement: "compile implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            started_budget(1_000, 1_000),
            Policy::builtin(),
            vec![personnel],
            1,
            Box::new(sink.clone()),
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
        (authority, plane, sink)
    }

    #[test]
    #[tracing::instrument]
fn journaled_drive_records_every_committed_event() {
        let (authority, _plane, sink) = journaled_authority();
        assert_eq!(sink.recorded(), authority.log().to_vec());
    }

    #[test]
    #[tracing::instrument]
fn plane_error_keeps_journaled_invocation() {
        let (mut authority, mut plane, sink) = journaled_authority();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        let recorded = sink.recorded();
        assert!(recorded
            .iter()
            .any(|event| matches!(event, DriveEvent::PlaySelected { .. })));
        assert!(recorded
            .iter()
            .any(|event| matches!(event, DriveEvent::ToolInvoked { .. })));
        assert!(matches!(
            recorded.last(),
            Some(DriveEvent::ToolOutcomeRejected {
                result: None,
                failure: None,
                ..
            })
        ));
        assert_eq!(recorded.len(), authority.log().len());
    }

    #[test]
    #[tracing::instrument]
fn attach_journal_continues_a_resumed_drive() {
        let (authority, _plane) = ready_authority();
        let resumed_from = authority.log().len();
        let events = authority.log().to_vec();
        let mut resumed =
            Authority::resume(events, authority.policy.clone(), authority.roster.clone()).unwrap();
        let sink = MemorySink::default();
        resumed.attach_journal(Box::new(sink.clone()));
        resumed.pressure(DriveBudget::zero(), 150).unwrap();
        assert_eq!(sink.recorded(), resumed.log()[resumed_from..].to_vec());
        assert!(sink
            .recorded()
            .iter()
            .any(|event| matches!(event, DriveEvent::BudgetConsumed { .. })));
    }

    #[test]
    #[tracing::instrument]
fn invalid_events_never_reach_the_sink() {
        let (mut authority, _plane, sink) = journaled_authority();
        let committed = sink.recorded().len();
        assert!(matches!(
            authority.waive(
                GateId::new("unknown-gate").unwrap(),
                "reviewer",
                "gate is not required"
            ),
            Err(PlaybookError::UnknownGate(_))
        ));
        assert_eq!(sink.recorded().len(), committed);
        assert_eq!(sink.recorded(), authority.log().to_vec());
    }

    #[test]
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
fn a_reconciled_not_occurred_key_is_reinvocable() {
        let (mut authority, mut plane) = ready_authority();
        assert_eq!(compile(&mut authority, &mut plane), PlaybookError::NoPlay);
        let invoke_end = authority
            .log()
            .iter()
            .rposition(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
            .unwrap()
            + 1;
        let key = match &authority.log()[invoke_end - 1] {
            DriveEvent::ToolInvoked { execution } => execution.idempotency_key.clone().unwrap(),
            _ => unreachable!(),
        };
        let mut resumed = Authority::resume(
            authority.log()[..invoke_end].to_vec(),
            authority.policy.clone(),
            authority.roster.clone(),
        )
        .unwrap();
        resumed.reconcile(&key, false).unwrap();
        plane
            .inner
            .resume_generation(resumed.state.capability_generation());
        let dispatched = resumed.try_operation("compile", &mut plane, 300);
        if !dispatched.is_ok() && dispatched != Err(PlaybookError::NoPlay) {
            panic!("dispatch returned {dispatched:?}");
        }
        assert_eq!(plane.mutations, 2);
    }

    #[test]
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
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

    #[tracing::instrument]
fn huddle_person(id: &str, kind: PersonnelKind) -> Personnel {
        Personnel {
            id: PersonnelId::new(id).unwrap(),
            kind,
            display_name: id.into(),
            cost_tokens: 40,
            latency_ms: 20,
            capability_fit_ppm: playbook_core::PPM_SCALE,
            reliability_ppm: 900_000,
            quota_remaining_ppm: playbook_core::PPM_SCALE,
            evidence_quality_ppm: playbook_core::PPM_SCALE,
            own_ytg_milli: None,
        }
    }

    #[tracing::instrument]
fn huddle_authority() -> Authority {
        let mut authority = Authority::start(
            DriveId::new("drive-huddle").unwrap(),
            TaskContract {
                statement: "coordinate the huddle".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            started_budget(10_000, 1_000),
            Policy::builtin(),
            vec![
                huddle_person("worker-a", PersonnelKind::DeterministicProcedure),
                huddle_person("worker-b", PersonnelKind::DeterministicProcedure),
                huddle_person("model-a", PersonnelKind::Model),
            ],
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
        authority
    }

    #[tracing::instrument]
fn candidate(id: &str, relevance: u32) -> ParticipantCandidate {
        ParticipantCandidate {
            personnel_id: PersonnelId::new(id).unwrap(),
            dependency_overlap: relevance,
            contradiction_relevance: relevance,
            ownership_weight: relevance,
            downstream_impact: relevance,
            specialist_value: relevance,
            context_cost: 10,
        }
    }

    #[tracing::instrument]
fn huddle_value() -> HuddleValue {
        HuddleValue {
            predicted_rework_avoided: 600,
            expected_failure_reduction: 300,
            expected_coordination_gain: 200,
            expected_budget_preservation: 100,
            inference_cost: 100,
            latency_cost: 50,
            context_cost: 50,
            opportunity_cost: 0,
        }
    }

    #[tracing::instrument]
fn huddle_request() -> HuddleRequest {
        HuddleRequest {
            scope: HuddleScope::Position,
            trigger_kind: HuddleTriggerKind::Failure,
            reason_code: "repeated_failed_play".into(),
            evidence_refs: Vec::new(),
            manual: false,
            value: huddle_value(),
            candidates: vec![
                candidate("worker-a", 900),
                candidate("worker-b", 900),
                candidate("model-a", 100),
            ],
            debt: 0,
            rework_avoided: 600,
        }
    }

    #[tracing::instrument]
fn drive_evidence(authority: &mut Authority, at_ms: u64) -> EvidenceId {
        let evidence = Evidence::seal(
            EvidenceClass::EnvironmentSnapshot,
            json!({"host": "fixture", "note": "huddle observation"}),
            at_ms,
        )
        .unwrap();
        let id = evidence.id.clone();
        authority.accept_evidence(evidence).unwrap();
        id
    }

    #[tracing::instrument]
fn huddle_report(participant: &str, current: &str, evidence: &EvidenceId) -> HuddleReport {
        HuddleReport {
            participant: PersonnelId::new(participant).unwrap(),
            verified_progress: vec![evidence.clone()],
            blockers: Vec::new(),
            changed_assumptions: vec![AssumptionDelta {
                subject: "interface".into(),
                previous: "v1".into(),
                current: current.into(),
                evidence_ref: None,
            }],
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            proposed_action: None,
            confidence_milli: 9_000,
        }
    }

    #[test]
    #[tracing::instrument]
fn a_huddle_lifecycle_journals_replays_and_moves_assignments() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let admission = authority.open_huddle(huddle_request(), 100).unwrap();
        let HuddleAdmission::Open(huddle_id) = admission else {
            panic!("economics should admit this huddle")
        };
        // Only the two strong workers clear the policy floor; the weak
        // model stays out by default exclusion.
        let active = authority
            .state()
            .huddles()
            .active_by_id(&huddle_id)
            .unwrap();
        assert_eq!(active.participants.len(), 2);
        assert_eq!(active.barrier, playbook_core::BarrierMode::Soft);

        authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-a", "v1", &evidence))
            .unwrap();
        authority
            .consume_huddle_budget(
                &huddle_id,
                HuddleCost {
                    tokens: 100,
                    cost_microunits: 10_000,
                    duration_ms: 100,
                    rounds: 0,
                },
            )
            .unwrap();
        let detected = authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-b", "v2", &evidence))
            .unwrap();
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].id.as_str(), "con-000");

        let outcome = HuddleOutcome {
            huddle_id: huddle_id.clone(),
            reconciled_state: SharedStateDelta {
                verified_evidence: vec![evidence.clone()],
                gate_snapshots: Vec::new(),
                assumptions: Vec::new(),
            },
            resolved_contradictions: vec![ContradictionResolution {
                contradiction_id: detected[0].id.clone(),
                state: ResolutionState::ResolvedByEvidence,
                evidence_ref: Some(evidence.clone()),
                authority: None,
            }],
            unresolved_contradictions: Vec::new(),
            assignment_changes: vec![AssignmentChange {
                personnel_id: PersonnelId::new("worker-b").unwrap(),
                from: Some("inspect".into()),
                to: "test.unit".into(),
            }],
            strategy_change: None,
            personnel_changes: vec![playbook_huddle::PersonnelChange {
                personnel_id: PersonnelId::new("model-a").unwrap(),
                action: PersonnelAction::SubstitutionRecommended,
                reason: "weakest relevance on the failing play".into(),
            }],
            revised_ytg_milli: Some(authority.state().ytg_milli()),
            debt_after: None,
            next_play: Some(PlayProposal {
                operation: "test.unit".into(),
                rationale: "unit coverage is the open dependency".into(),
                target_gate: Some(GateId::new("unit_tests").unwrap()),
            }),
            next_huddle_conditions: Vec::new(),
            escalated_to: None,
        };
        let gates_before = authority.state().gates().to_vec();
        let evidence_count = authority.state().huddles().requested_total();
        authority
            .commit_huddle_outcome(&huddle_id, outcome)
            .unwrap();

        let ledger = authority.state().huddles();
        assert!(ledger.active().is_empty());
        assert_eq!(ledger.committed_total(), 1);
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::Committed);
        assert_eq!(ledger.assignments().len(), 1);
        assert_eq!(ledger.substitutions().len(), 1);
        assert_eq!(ledger.proposals().len(), 1);
        // A huddle moves assignments and recommendations; it never touches
        // gates, evidence, or the measured field.
        assert_eq!(authority.state().gates(), &gates_before[..]);
        assert_eq!(
            authority.state().huddles().requested_total(),
            evidence_count
        );
        assert_eq!(authority.state().ytg_milli(), 10_000);

        // Crash recovery and journal replay reproduce the committed outcome.
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[test]
    #[tracing::instrument]
fn a_second_huddle_on_an_unchanged_field_is_refused() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let admission = authority.open_huddle(huddle_request(), 100).unwrap();
        let HuddleAdmission::Open(huddle_id) = admission else {
            panic!("first huddle should open")
        };
        // Reports cite evidence the field already held; the field itself
        // does not move during the huddle.
        authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-a", "v1", &evidence))
            .unwrap();
        authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-b", "v1", &evidence))
            .unwrap();
        let mut outcome = playbook_huddle::HuddleOutcome {
            huddle_id: huddle_id.clone(),
            reconciled_state: SharedStateDelta {
                verified_evidence: Vec::new(),
                gate_snapshots: Vec::new(),
                assumptions: Vec::new(),
            },
            resolved_contradictions: Vec::new(),
            unresolved_contradictions: Vec::new(),
            assignment_changes: Vec::new(),
            strategy_change: None,
            personnel_changes: Vec::new(),
            revised_ytg_milli: None,
            debt_after: None,
            next_play: None,
            next_huddle_conditions: Vec::new(),
            escalated_to: None,
        };
        outcome.reconciled_state.verified_evidence.clear();
        authority
            .commit_huddle_outcome(&huddle_id, outcome)
            .unwrap();

        let second = authority.open_huddle(huddle_request(), 200).unwrap();
        match second {
            HuddleAdmission::Refused { reason_code, .. } => {
                assert_eq!(reason_code, "unchanged_field")
            }
            HuddleAdmission::Open(_) => panic!("a huddle storm must be refused"),
        }
        assert_eq!(authority.state().huddles().rejected_total(), 1);
    }

    #[test]
    #[tracing::instrument]
fn unresolved_contradictions_escalate_scope_and_spawn_a_child_huddle() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-a", "v1", &evidence))
            .unwrap();
        let detected = authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-b", "v2", &evidence))
            .unwrap();
        assert_eq!(detected.len(), 1);
        // A third voice turns the split into a High-severity contradiction,
        // which is what justifies a scope escalation.
        authority
            .invite_to_huddle(&huddle_id, &PersonnelId::new("model-a").unwrap())
            .unwrap();
        let detected = authority
            .submit_huddle_report(&huddle_id, huddle_report("model-a", "v3", &evidence))
            .unwrap();
        assert_eq!(detected.len(), 1);
        assert_eq!(
            detected[0].severity,
            playbook_huddle::ContradictionSeverity::High
        );

        let child = authority
            .escalate_huddle(&huddle_id, false, "unresolved_interface_contradiction")
            .unwrap();
        let ledger = authority.state().huddles();
        assert!(ledger.active_by_id(&huddle_id).is_none());
        let escalated = ledger.active_by_id(&child).unwrap();
        assert_eq!(escalated.scope, HuddleScope::Unit);
        assert_eq!(ledger.escalated_total(), 1);
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[test]
    #[tracing::instrument]
fn bench_review_follows_committed_huddle_recommendations() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        for author in ["worker-a", "worker-b"] {
            authority
                .submit_huddle_report(&huddle_id, huddle_report(author, "v1", &evidence))
                .unwrap();
        }
        let committed = outcome_for_bench_test(&huddle_id, authority.state().ytg_milli());
        authority
            .commit_huddle_outcome(&huddle_id, committed)
            .unwrap();

        // The Bench verdict cites the committed recommendation and benches
        // the flagged personnel.
        let verdicts = authority.bench_from_review().unwrap();
        assert_eq!(verdicts, 1);
        assert!(authority
            .state()
            .benched()
            .iter()
            .any(|id| id.as_str() == "worker-b"));
        // Replaying the review does not double-bench.
        assert_eq!(authority.bench_from_review().unwrap(), 0);

        // The selection pool excludes the benched personnel: the next play
        // runs through the replacement.
        let mut plane = FixturePlane::ready();
        authority.preflight(&mut plane).unwrap();
        let selection = authority.try_operation("inspect", &mut plane, 400).unwrap();
        assert_eq!(selection.selected_personnel.unwrap().as_str(), "worker-a");

        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[tracing::instrument]
fn outcome_for_bench_test(huddle_id: &playbook_core::HuddleId, ytg: u32) -> HuddleOutcome {
        playbook_huddle::HuddleOutcome {
            huddle_id: huddle_id.clone(),
            reconciled_state: SharedStateDelta {
                verified_evidence: Vec::new(),
                gate_snapshots: Vec::new(),
                assumptions: Vec::new(),
            },
            resolved_contradictions: Vec::new(),
            unresolved_contradictions: Vec::new(),
            assignment_changes: Vec::new(),
            strategy_change: None,
            personnel_changes: vec![playbook_huddle::PersonnelChange {
                personnel_id: PersonnelId::new("worker-b").unwrap(),
                action: PersonnelAction::SubstitutionRecommended,
                reason: "repeated failures under the fielded contract".into(),
            }],
            revised_ytg_milli: Some(ytg),
            debt_after: None,
            next_play: None,
            next_huddle_conditions: Vec::new(),
            escalated_to: None,
        }
    }

    #[test]
    #[tracing::instrument]
fn budget_exhaustion_forces_synthesis_from_accumulated_reports() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        authority
            .submit_huddle_report(&huddle_id, huddle_report("worker-a", "v1", &evidence))
            .unwrap();
        authority
            .consume_huddle_budget(
                &huddle_id,
                HuddleCost {
                    tokens: 2_000,
                    cost_microunits: 200_000,
                    duration_ms: 5_000,
                    rounds: 1,
                },
            )
            .unwrap();
        authority.force_huddle_synthesis(&huddle_id).unwrap();
        let ledger = authority.state().huddles();
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::ForcedSynthesis);
        assert!(ledger
            .conditions()
            .iter()
            .any(|condition| condition.reason_code == "post_synthesis_review"));
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[test]
    #[tracing::instrument]
fn coherence_reads_healthy_before_any_huddle() {
        let authority = huddle_authority();
        let reading = authority.coherence();
        assert_eq!(reading.state.debt, 0);
        assert_eq!(reading.state.band, playbook_huddle::DebtBand::Healthy);
        assert_eq!(reading.state.active_huddles, 0);
        assert!(reading.condition.is_none());
    }

    #[test]
    #[tracing::instrument]
fn coherence_withholds_the_failure_trigger_while_the_field_moves() {
        let mut authority = huddle_authority();
        let mut plane = FixturePlane::ready();
        plane.push_success(
            "inspect",
            Evidence::seal(
                EvidenceClass::EnvironmentSnapshot,
                json!({"host": "lab"}),
                10,
            )
            .unwrap(),
            DriveBudget {
                tokens: 1,
                monetary_cost_micros: 100,
                tool_calls: 1,
                ..DriveBudget::zero()
            },
        );
        plane.push_failure("compile", ToolFailureKind::Timeout);
        plane.push_failure("compile", ToolFailureKind::Timeout);
        authority.preflight(&mut plane).unwrap();
        authority.step(&mut plane, 100).unwrap();
        authority.step(&mut plane, 200).unwrap();
        authority.step(&mut plane, 300).unwrap();
        // Two trailing failures, but each moved the down: the unchanged-field
        // guard withholds the failure trigger at the live seam.
        assert!(authority.coherence().condition.is_none());
    }

    #[test]
    #[tracing::instrument]
fn coherence_debt_climbs_with_unresolved_contradictions() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        let contradictory = |participant: &str, interface: &str, contract: &str| HuddleReport {
            participant: PersonnelId::new(participant).unwrap(),
            verified_progress: vec![evidence.clone()],
            blockers: Vec::new(),
            changed_assumptions: vec![
                AssumptionDelta {
                    subject: "interface".into(),
                    previous: "v1".into(),
                    current: interface.into(),
                    evidence_ref: None,
                },
                AssumptionDelta {
                    subject: "contract".into(),
                    previous: "v1".into(),
                    current: contract.into(),
                    evidence_ref: None,
                },
            ],
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            proposed_action: None,
            confidence_milli: 9_000,
        };
        for (participant, interface, contract) in
            [("worker-a", "v1", "v1"), ("worker-b", "v2", "v2")]
        {
            authority
                .submit_huddle_report(&huddle_id, contradictory(participant, interface, contract))
                .unwrap();
        }
        authority
            .invite_to_huddle(&huddle_id, &PersonnelId::new("model-a").unwrap())
            .unwrap();
        authority
            .submit_huddle_report(&huddle_id, contradictory("model-a", "v3", "v3"))
            .unwrap();
        let reading = authority.coherence();
        assert_eq!(reading.state.unresolved_contradictions, 2);
        // Two contradictions (load 500) plus two diverging assumption
        // subjects (divergence 1000) land the drive in the eligible band:
        // a position or unit huddle is now economically justified.
        assert_eq!(
            reading.state.band,
            playbook_huddle::DebtBand::HuddleEligible
        );
        assert!(reading.state.debt >= 500);
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[tracing::instrument]
fn dispatch_report(evidence: &EvidenceId, confidence: u64) -> serde_json::Value {
        json!({
            "participant": "dispatched",
            "verified_progress": [evidence.as_str()],
            "blockers": [],
            "changed_assumptions": [],
            "dependency_changes": [],
            "contradictions": [],
            "proposed_action": null,
            "confidence_milli": confidence
        })
    }

    #[test]
    #[tracing::instrument]
fn collect_huddle_reports_journals_dispatched_reports_and_charges_the_envelope() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        let mut plane = FixtureHuddlePlane::responding(vec![
            Ok(dispatch_report(&evidence, 9_000)),
            Ok(dispatch_report(&evidence, 8_000)),
        ]);
        let summary = authority
            .collect_huddle_reports(
                &huddle_id,
                &mut plane,
                HuddleCost {
                    tokens: 100,
                    cost_microunits: 10_000,
                    duration_ms: 50,
                    rounds: 0,
                },
            )
            .unwrap();
        assert_eq!(
            summary
                .collected
                .iter()
                .map(PersonnelId::as_str)
                .collect::<Vec<_>>(),
            vec!["worker-a", "worker-b"]
        );
        assert!(summary.failed.is_empty());
        assert!(!summary.synthesized);
        let active = authority
            .state()
            .huddles()
            .active_by_id(&huddle_id)
            .unwrap();
        assert_eq!(active.reports.len(), 2);
        assert_eq!(active.spent.tokens, 200);
        // A dispatched report is attributed to the invited participant, not
        // to whatever the response claimed.
        assert_eq!(active.reports[0].participant.as_str(), "worker-a");
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[test]
    #[tracing::instrument]
fn collect_huddle_reports_isolates_one_participant_failure() {
        let mut authority = huddle_authority();
        let evidence = drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        let mut plane = FixtureHuddlePlane::responding(vec![
            Ok(dispatch_report(&evidence, 9_000)),
            Err("participant timed out".into()),
        ]);
        let summary = authority
            .collect_huddle_reports(
                &huddle_id,
                &mut plane,
                HuddleCost {
                    tokens: 100,
                    cost_microunits: 10_000,
                    duration_ms: 50,
                    rounds: 0,
                },
            )
            .unwrap();
        assert_eq!(summary.collected.len(), 1);
        assert_eq!(summary.failed.len(), 1);
        assert_eq!(summary.failed[0].0.as_str(), "worker-b");
        assert!(summary.failed[0].1.contains("timed out"));
        // The huddle is still collecting: the absent report is the failure
        // mode, not an aborted huddle.
        let active = authority
            .state()
            .huddles()
            .active_by_id(&huddle_id)
            .unwrap();
        assert_eq!(active.phase, playbook_huddle::HuddlePhase::CollectReports);
        assert_eq!(active.reports.len(), 1);
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }

    #[test]
    #[tracing::instrument]
fn collect_huddle_reports_stops_on_exhaustion_and_forces_synthesis() {
        let mut authority = huddle_authority();
        drive_evidence(&mut authority, 50);
        let HuddleAdmission::Open(huddle_id) =
            authority.open_huddle(huddle_request(), 100).unwrap()
        else {
            panic!("huddle should open")
        };
        authority
            .consume_huddle_budget(
                &huddle_id,
                HuddleCost {
                    tokens: 2_000,
                    cost_microunits: 200_000,
                    duration_ms: 5_000,
                    rounds: 1,
                },
            )
            .unwrap();
        let mut plane = FixtureHuddlePlane::responding(vec![]);
        let summary = authority
            .collect_huddle_reports(
                &huddle_id,
                &mut plane,
                HuddleCost {
                    tokens: 100,
                    cost_microunits: 10_000,
                    duration_ms: 50,
                    rounds: 0,
                },
            )
            .unwrap();
        assert!(summary.collected.is_empty());
        assert!(summary.synthesized);
        let ledger = authority.state().huddles();
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::ForcedSynthesis);
        assert!(ledger.active_by_id(&huddle_id).is_none());
        let replay =
            crate::fold(authority.log(), authority.policy(), crate::ApplyMode::Audit).unwrap();
        assert_eq!(&replay, authority.state());
    }
}
