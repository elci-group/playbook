//! Fold drive events into state.
//!
//! Audit mode checks witnesses against the fold. Resimulate mode recomputes
//! downs under another policy and keeps historical sealed packages.

use playbook_budget::consume;
use playbook_core::{
    CapabilityRequirement, Disposition, Down, DriveBudget, DriveStatus, Evidence, EvidenceClass,
    IdempotencyState, PlayStatus, PlaybookError, ProvisionStrategy, SideEffectNote, Waiver,
    PPM_SCALE,
};
use playbook_events::DriveEvent;
use playbook_gates::{apply_evidence, invalidate_evidence, is_complete, waive};
use playbook_lastdown::{evaluate, DoctrineInput};
use playbook_policy::Policy;
use playbook_routing::admit;
use playbook_security::authorize;

use crate::{classify_measurement, measurement_between, DriveState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyMode {
    Audit,
    Resimulate,
}

/// Serialize a drive event log as JSON.
pub fn export_log(events: &[DriveEvent]) -> Result<String, PlaybookError> {
    serde_json::to_string(events).map_err(PlaybookError::json)
}

/// Parse a JSON drive event log.
pub fn import_log(json: &str) -> Result<Vec<DriveEvent>, PlaybookError> {
    serde_json::from_str(json).map_err(PlaybookError::json)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrajectoryPoint {
    pub index: usize,
    pub event: String,
    pub down: Down,
    pub ytg_milli: u32,
    pub budget_remaining_ppm: u32,
    pub completion_ppm: u32,
    pub status: DriveStatus,
}

pub fn fold(
    events: &[DriveEvent],
    policy: &Policy,
    mode: ApplyMode,
) -> Result<DriveState, PlaybookError> {
    let mut state = None;
    for event in events {
        state = Some(apply(state.as_ref(), event, policy, mode)?);
    }
    state.ok_or(PlaybookError::Incomplete)
}

pub fn trajectory(
    events: &[DriveEvent],
    policy: &Policy,
    mode: ApplyMode,
) -> Result<Vec<TrajectoryPoint>, PlaybookError> {
    let mut state = None;
    let mut points = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let next = apply(state.as_ref(), event, policy, mode)?;
        points.push(TrajectoryPoint {
            index,
            event: event.name().to_string(),
            down: next.down(),
            ytg_milli: next.ytg_milli(),
            budget_remaining_ppm: next.budget_remaining_ppm(policy),
            completion_ppm: next.completion_ppm(),
            status: next.status(),
        });
        state = Some(next);
    }
    Ok(points)
}

pub fn apply(
    existing: Option<&DriveState>,
    event: &DriveEvent,
    policy: &Policy,
    mode: ApplyMode,
) -> Result<DriveState, PlaybookError> {
    match event {
        DriveEvent::DriveStarted {
            drive_id,
            task,
            budget,
            at_ms,
        } => {
            if existing.is_some() {
                return Err(PlaybookError::Closed);
            }
            playbook_core::accept_schema(playbook_core::SCHEMA_VERSION)?;
            let mut next = DriveState::fresh(drive_id.clone(), task.clone(), *budget, *at_ms);
            let before = next.clone();
            close_cycle(&before, &mut next, policy);
            Ok(next)
        }
        other => {
            let Some(current) = existing else {
                return Err(PlaybookError::Incomplete);
            };
            apply_open(current, other, policy, mode)
        }
    }
}

fn apply_open(
    current: &DriveState,
    event: &DriveEvent,
    policy: &Policy,
    mode: ApplyMode,
) -> Result<DriveState, PlaybookError> {
    let before = current.clone();
    let mut next = current.clone();
    match event {
        DriveEvent::DriveStarted { .. } => return Err(PlaybookError::Closed),
        DriveEvent::GateDeclared { gate } => {
            if next.gates.iter().any(|existing| existing.id == gate.id) {
                return Err(PlaybookError::UnknownGate(gate.id.to_string()));
            }
            next.gates.push(gate.clone());
            next.refresh_ytg();
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::CapabilityObserved {
            capability,
            generation,
        } => {
            if *generation < next.capability_generation {
                return Err(PlaybookError::StaleCapability);
            }
            next.capability_generation = *generation;
            if let Some(slot) = next
                .capabilities
                .iter_mut()
                .find(|existing| existing.id == capability.id)
            {
                *slot = capability.clone();
            } else {
                next.capabilities.push(capability.clone());
            }
        }
        DriveEvent::PreflightCompleted {
            missing,
            generation,
        } => {
            if *generation != next.capability_generation || *generation == 0 {
                return Err(PlaybookError::StaleCapability);
            }
            next.missing_evidence = missing.clone();
            next.preflight_complete = true;
            next.cycle_failure = false;
            next.failure_class = None;
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::PlaySelected { decision } => {
            let definition = policy.play(&decision.selected_operation).ok_or_else(|| {
                PlaybookError::Inadmissible(
                    decision.selected_operation.clone(),
                    "unknown play".into(),
                )
            })?;
            let personnel = decision
                .selected_personnel
                .clone()
                .ok_or(PlaybookError::NoPlay)?;
            admit_selected(&next, definition, &personnel, policy)?;
            next.decisions.push((**decision).clone());
            next.last_operation = Some(decision.selected_operation.clone());
        }
        DriveEvent::PersonnelChanged { personnel } => {
            next.personnel = Some(personnel.clone());
        }
        DriveEvent::ToolInvoked { execution } => {
            ensure_active(&next)?;
            ensure_no_open_execution(&next)?;
            if execution.idempotency_key.is_none() {
                return Err(PlaybookError::Inadmissible(
                    execution.operation.clone(),
                    "invocation requires an idempotency key".into(),
                ));
            }
            let definition = policy
                .play(&execution.operation)
                .ok_or(PlaybookError::NoPlay)?;
            let contract_mismatch = execution.drive_id != next.drive_id
                || execution.play_kind != definition.kind
                || execution.capability_id.as_str() != definition.capability_id.as_str()
                || execution.side_effect != definition.side_effect
                || execution.expected_evidence != [definition.evidence_class]
                || execution.expected_ytg_reduction_milli
                    != definition.expected_ytg_reduction_milli
                || execution.survey != definition.survey
                || next.personnel.as_ref().map(|person| &person.id)
                    != Some(&execution.personnel_id)
                || (mode == ApplyMode::Audit && execution.down != next.down);
            if contract_mismatch {
                return Err(PlaybookError::Inadmissible(
                    execution.operation.clone(),
                    "invoke does not match the drive, selected play, or policy contract".into(),
                ));
            }
            if next.last_operation.as_deref() != Some(execution.operation.as_str()) {
                return Err(PlaybookError::Inadmissible(
                    execution.operation.clone(),
                    "invoke does not match the selected play".into(),
                ));
            }
            if execution.capability_generation != next.capability_generation {
                return Err(PlaybookError::StaleCapability);
            }
            if !next
                .capabilities
                .iter()
                .any(|capability| capability.id == execution.capability_id)
            {
                return Err(PlaybookError::StaleCapability);
            }
            authorize(execution.side_effect, &policy.allowed_side_effects)?;
            admit_selected(&next, definition, &execution.personnel_id, policy)?;
            note_idempotency_start(&mut next, execution.idempotency_key.as_deref())?;
            if execution.expected_ytg_reduction_milli > 0 {
                next.attempts = next.attempts.saturating_add(1);
            }
            next.open_execution = Some(execution.clone());
            next.last_operation = Some(execution.operation.clone());
        }
        DriveEvent::ToolSucceeded { result } => {
            ensure_active(&next)?;
            let execution = next.open_execution.clone().ok_or(PlaybookError::NoPlay)?;
            if result.status != PlayStatus::Succeeded
                || result.play_id != execution.play_id
                || result.operation != execution.operation
            {
                return Err(PlaybookError::Inadmissible(
                    result.operation.clone(),
                    "a failed result cannot be recorded as success".into(),
                ));
            }
            validate_effect_keys(
                execution.idempotency_key.as_deref(),
                &result.side_effects,
                &execution.operation,
            )?;
            if !result.cost.fits_within(execution.budget_ceiling) {
                return Err(PlaybookError::BudgetCeiling);
            }
            next.budget_consumed = consume(next.budget_initial, next.budget_consumed, result.cost)?;
            next.last_cost = result.cost;
            next.cycle_failure = false;
            next.failure_class = None;
            next.count_regression = false;
            for evidence in &result.evidence {
                ingest(&mut next, evidence)?;
            }
            settle_effects(
                &mut next,
                execution.idempotency_key.as_deref(),
                &result.side_effects,
            );
            if execution.survey {
                next.field_surveyed = true;
            }
            next.last_result = Some(PlayStatus::Succeeded);
            next.open_execution = None;
            next.at_ms = result.at_ms;
            close_cycle(&before, &mut next, policy);
            // Phase 8: the outcome enters the historical table only through a
            // measured yards-to-go drop while the play ran. Claims never do.
            let realized = before.ytg_milli.saturating_sub(next.ytg_milli);
            next.priors.record(&execution.personnel_id, realized);
        }
        DriveEvent::ToolFailed { failure } => {
            ensure_active(&next)?;
            let execution = next.open_execution.clone().ok_or(PlaybookError::NoPlay)?;
            if failure.play_id != execution.play_id
                || failure.operation != execution.operation
                || failure.play_kind != execution.play_kind
                || failure.personnel_id != execution.personnel_id
                || failure.capability_id != execution.capability_id
                || failure.idempotency_key != execution.idempotency_key
            {
                return Err(PlaybookError::Inadmissible(
                    failure.operation.clone(),
                    "failure does not match the open play".into(),
                ));
            }
            validate_effect_keys(
                execution.idempotency_key.as_deref(),
                &failure.side_effects,
                &execution.operation,
            )?;
            if !failure.cost.fits_within(execution.budget_ceiling) {
                return Err(PlaybookError::BudgetCeiling);
            }
            next.budget_consumed =
                consume(next.budget_initial, next.budget_consumed, failure.cost)?;
            next.last_cost = failure.cost;
            let evidence = Evidence::seal(
                EvidenceClass::ToolInvocation,
                serde_json::json!({
                    "status": "failed",
                    "kind": failure.kind,
                    "operation": failure.operation,
                }),
                failure.at_ms,
            )?;
            next.failure_refs.push(evidence.id.clone());
            next.evidence.push(evidence);
            next.cycle_failure = true;
            next.failure_class = Some(failure.kind.failure_class());
            next.last_failure_kind = Some(failure.kind);
            next.last_failed_operation = Some(failure.operation.clone());
            next.last_failed_personnel = Some(failure.personnel_id.clone());
            next.last_result = Some(PlayStatus::Failed);
            settle_effects(
                &mut next,
                failure.idempotency_key.as_deref(),
                &failure.side_effects,
            );
            next.failure_fingerprint = Some(next.fingerprint());
            next.open_execution = None;
            next.at_ms = failure.at_ms;
            if failure.kind == playbook_core::ToolFailureKind::Unsupported {
                next.acquisition_count = next.acquisition_count.saturating_add(1);
            }
            close_cycle(&before, &mut next, policy);
            // A failure is a measured outcome of zero progress.
            next.priors.record(&failure.personnel_id, 0);
        }
        DriveEvent::ToolOutcomeRejected {
            result,
            failure,
            reason,
        } => {
            let execution = next.open_execution.clone().ok_or(PlaybookError::NoPlay)?;
            if reason.trim().is_empty() || (result.is_some() && failure.is_some()) {
                return Err(PlaybookError::Inadmissible(
                    execution.operation.clone(),
                    "rejected outcome has an empty reason or conflicting observations".into(),
                ));
            }
            let (cost, effects) = if let Some(result) = result {
                (result.cost, result.side_effects.as_slice())
            } else if let Some(failure) = failure {
                (failure.cost, failure.side_effects.as_slice())
            } else {
                (DriveBudget::zero(), &[][..])
            };
            let Some(key) = execution.idempotency_key.as_deref() else {
                return Err(PlaybookError::Reconcile(execution.play_id.to_string()));
            };
            if next.idempotency.get(key) != Some(&IdempotencyState::Started) {
                return Err(PlaybookError::Reconcile(key.to_string()));
            }

            // The work already ran, so its observed cost is real even when it
            // exceeded the ceiling or its evidence could not be accepted.
            // Saturation avoids turning an accounting overflow into a loss of
            // the invocation audit record.
            next.budget_consumed = saturating_add_budget(next.budget_consumed, cost);
            next.last_cost = cost;
            next.idempotency
                .insert(key.to_string(), IdempotencyState::Unknown);
            for effect in effects {
                if !effect.resolved {
                    let mut effect = effect.clone();
                    // Keep reconciliation addressable by the invocation key;
                    // the raw mismatched note remains in the rejected event.
                    effect.idempotency_key = key.to_string();
                    next.unresolved_effects.push(effect);
                }
            }
            if !next
                .unresolved_effects
                .iter()
                .any(|effect| effect.idempotency_key == key && !effect.resolved)
            {
                next.unresolved_effects.push(SideEffectNote {
                    class: execution.side_effect,
                    idempotency_key: key.to_string(),
                    summary: format!("outcome rejected: {reason}"),
                    resolved: false,
                    occurred: None,
                });
            }
        }
        DriveEvent::EvidenceAccepted { evidence } => {
            ensure_active(&next)?;
            next.cycle_failure = false;
            next.failure_class = None;
            next.count_regression = false;
            ingest(&mut next, evidence)?;
            next.at_ms = evidence.observed_at_ms;
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::ArtefactProduced { artefact } => {
            next.artefacts.push(artefact.clone());
        }
        DriveEvent::ArtefactValidated {
            artefact_id,
            evidence_id,
        } => {
            let artefact = next
                .artefacts
                .iter_mut()
                .find(|artefact| &artefact.id == artefact_id)
                .ok_or_else(|| PlaybookError::UnknownGate(artefact_id.to_string()))?;
            if !next.evidence.iter().any(|item| &item.id == evidence_id) {
                return Err(PlaybookError::Evidence(
                    "validation evidence is missing".into(),
                ));
            }
            artefact.validated = true;
            artefact.evidence_id = Some(evidence_id.clone());
        }
        DriveEvent::ArtefactRejected {
            artefact_id,
            evidence_id,
        } => {
            let artefact = next
                .artefacts
                .iter_mut()
                .find(|artefact| &artefact.id == artefact_id)
                .ok_or_else(|| PlaybookError::UnknownGate(artefact_id.to_string()))?;
            artefact.rejected = true;
            artefact.validated = false;
            let failed = invalidate_evidence(&mut next.gates, evidence_id);
            if !failed.is_empty() {
                next.count_regression = true;
                next.material_failure = true;
            }
            next.refresh_ytg();
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::BudgetConsumed { cost, at_ms } => {
            ensure_active(&next)?;
            next.cycle_failure = false;
            next.failure_class = None;
            next.count_regression = false;
            next.budget_consumed = consume(next.budget_initial, next.budget_consumed, *cost)?;
            next.last_cost = *cost;
            next.at_ms = *at_ms;
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::WaiverGranted {
            gate_id,
            authority,
            reason,
        } => {
            ensure_active(&next)?;
            let waiver = Waiver::new(authority, reason)?;
            waive(&mut next.gates, gate_id, waiver)?;
            next.refresh_ytg();
            next.cycle_failure = false;
            next.failure_class = None;
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::YtgRecalculated {
            ytg_milli,
            previous_milli,
        } => {
            if mode == ApplyMode::Resimulate {
                return Ok(next);
            }
            if next.ytg_milli != *ytg_milli || next.cycle_previous_ytg != *previous_milli {
                return Err(PlaybookError::ReplayDivergence(format!(
                    "ytg witness {previous_milli} → {ytg_milli}, state {} → {}",
                    next.cycle_previous_ytg, next.ytg_milli
                )));
            }
        }
        DriveEvent::DownChanged { down, reasons } => {
            if mode == ApplyMode::Resimulate {
                return Ok(next);
            }
            if next.down != *down || next.explanation != *reasons {
                return Err(PlaybookError::ReplayDivergence(format!(
                    "down witness {} does not match {}",
                    down.machine_id(),
                    next.down.machine_id()
                )));
            }
        }
        DriveEvent::FourthDownDeclared {
            probability_ppm,
            threshold_ppm,
            disposition,
        } => {
            if mode == ApplyMode::Resimulate {
                return Ok(next);
            }
            let expected = policy.conversion_threshold(&next.task.workload_class);
            if next.down != Down::FourthDown
                || next.completion_ppm != *probability_ppm
                || expected != *threshold_ppm
                || next.disposition != Some(*disposition)
            {
                return Err(PlaybookError::ReplayDivergence(
                    "fourth-down witness does not match the doctrine".into(),
                ));
            }
        }
        DriveEvent::CapabilityRequirementIssued { requirement } => {
            if mode == ApplyMode::Resimulate {
                return Ok(next);
            }
            if next.requirement.as_ref() != Some(requirement) {
                return Err(PlaybookError::ReplayDivergence(
                    "capability requirement witness does not match".into(),
                ));
            }
        }
        DriveEvent::FieldGoalSelected { gate_id } => {
            ensure_no_open_execution(&next)?;
            if !next.gates.iter().any(|gate| &gate.id == gate_id) {
                return Err(PlaybookError::UnknownGate(gate_id.to_string()));
            }
            next.field_goal_gate = Some(gate_id.clone());
            next.disposition = Some(Disposition::FieldGoal);
            score_field_goal(&mut next);
        }
        DriveEvent::PuntIssued { package } => {
            ensure_no_open_execution(&next)?;
            if next.down != Down::FourthDown || next.disposition != Some(Disposition::Punt) {
                return Err(PlaybookError::PuntNotReady);
            }
            if playbook_core::leaks_competitive_metadata(
                &serde_json::to_vec(package).map_err(PlaybookError::json)?,
            ) {
                return Err(PlaybookError::AmnesiaLeak);
            }
            if mode == ApplyMode::Audit {
                let sealed = next.seal_package(policy)?;
                if sealed != **package {
                    return Err(PlaybookError::ReplayDivergence(
                        "punt package does not match the sealed field".into(),
                    ));
                }
            }
            next.package = Some((**package).clone());
            next.status = DriveStatus::Punted;
        }
        DriveEvent::PossessionStarted {
            personnel,
            budget,
            at_ms,
        } => {
            ensure_no_open_execution(&next)?;
            if next.status != DriveStatus::Punted && next.disposition != Some(Disposition::Reset) {
                return Err(PlaybookError::PuntNotReady);
            }
            next.status = DriveStatus::Active;
            next.possession_generation = next.possession_generation.saturating_add(1);
            next.budget_initial = *budget;
            next.budget_consumed = DriveBudget::zero();
            next.last_cost = DriveBudget::zero();
            next.personnel = Some(personnel.clone());
            next.preflight_complete = false;
            next.possession_restarted = true;
            next.stall_cycles = 0;
            next.cycle_failure = false;
            next.failure_class = None;
            next.last_failed_operation = None;
            next.failure_fingerprint = None;
            next.origin_ytg_milli = next.ytg_milli;
            next.at_ms = *at_ms;
            close_cycle(&before, &mut next, policy);
        }
        DriveEvent::AssessmentsRecorded { personnel } => {
            next.assessments = personnel.clone();
        }
        DriveEvent::PriorsLoaded { priors } => {
            // Phase 9: imported history is authoritative input, like the
            // drive contract itself. It replaces whatever the table held;
            // measured outcomes recorded afterwards accrue on top. The same
            // event folds identically in audit and resimulate modes.
            next.priors = priors.clone();
        }
        DriveEvent::SideEffectReconciled { key, occurred } => {
            if !matches!(
                next.idempotency.get(key),
                Some(IdempotencyState::Unknown | IdempotencyState::Started)
            ) {
                return Err(PlaybookError::Reconcile(key.clone()));
            }
            for effect in &mut next.unresolved_effects {
                if &effect.idempotency_key == key {
                    effect.occurred = Some(*occurred);
                    effect.resolved = true;
                }
            }
            next.unresolved_effects
                .retain(|effect| effect.idempotency_key != *key || !effect.resolved);
            if *occurred {
                next.idempotency
                    .insert(key.clone(), IdempotencyState::Committed);
            } else {
                next.idempotency.remove(key);
            }
            if next
                .open_execution
                .as_ref()
                .and_then(|execution| execution.idempotency_key.as_ref())
                .is_some_and(|open_key| open_key == key)
            {
                next.open_execution = None;
            }
        }
        DriveEvent::CompletionRejected { at_ms, reason } => {
            if reason.trim().is_empty() {
                return Err(PlaybookError::Incomplete);
            }
            next.false_completion_claims = next.false_completion_claims.saturating_add(1);
            next.at_ms = *at_ms;
        }
        DriveEvent::DriveCompleted { at_ms } => {
            ensure_no_open_execution(&next)?;
            if !is_complete(&next.gates) || next.ytg_milli != 0 {
                return Err(PlaybookError::Incomplete);
            }
            // A sealed contract is the only authority on what a win means.
            if let Some(contract) = &next.game_contract {
                let verdict = playbook_core::verify_win_conditions(
                    contract,
                    &next.gates,
                    &next.evidence,
                    &next.artefacts,
                    &next.environment_snapshot,
                );
                if !verdict.verified_win {
                    return Err(PlaybookError::Incomplete);
                }
            }
            next.status = DriveStatus::Completed;
            next.at_ms = *at_ms;
        }
        DriveEvent::AbandonIssued { reason } => {
            ensure_no_open_execution(&next)?;
            let decision = doctrine(&next, policy);
            if next.task.integrity == playbook_core::TaskIntegrity::Sound
                && decision.chosen != Disposition::Abandon
            {
                return Err(PlaybookError::DispositionMismatch);
            }
            if reason.trim().is_empty() {
                return Err(PlaybookError::DispositionMismatch);
            }
            next.status = DriveStatus::Abandoned;
            next.disposition = Some(Disposition::Abandon);
        }
        DriveEvent::BenchDecision {
            personnel_id,
            benched,
            reason_code,
        } => {
            ensure_active(&next)?;
            if reason_code.trim().is_empty() {
                return Err(PlaybookError::Inadmissible(
                    "bench".into(),
                    "bench verdict requires a reason code".into(),
                ));
            }
            if *benched {
                if next
                    .open_execution
                    .as_ref()
                    .is_some_and(|execution| &execution.personnel_id == personnel_id)
                {
                    return Err(PlaybookError::Inadmissible(
                        "bench".into(),
                        "cannot bench personnel with an open execution".into(),
                    ));
                }
                if !next.benched.contains(personnel_id) {
                    next.benched.push(personnel_id.clone());
                }
            } else {
                next.benched.retain(|id| id != personnel_id);
            }
        }
        DriveEvent::HuddleRequested { .. }
        | DriveEvent::HuddleAdmitted { .. }
        | DriveEvent::HuddleRejected { .. }
        | DriveEvent::HuddleOpened { .. }
        | DriveEvent::HuddleParticipantInvited { .. }
        | DriveEvent::HuddleReportSubmitted { .. }
        | DriveEvent::HuddleContradictionDetected { .. }
        | DriveEvent::HuddleBudgetConsumed { .. }
        | DriveEvent::HuddleOutcomeCommitted { .. }
        | DriveEvent::HuddleEscalationRequested { .. }
        | DriveEvent::HuddleEscalated { .. }
        | DriveEvent::HuddleClosed { .. } => {
            let huddle_event =
                project_huddle(event).expect("huddle projection covers huddle events");
            let view = playbook_huddle::DriveView {
                active: !matches!(
                    next.status,
                    DriveStatus::Completed
                        | DriveStatus::Abandoned
                        | DriveStatus::FieldGoalScored
                        | DriveStatus::Punted
                ),
                down: next.down,
                ytg_milli: next.ytg_milli,
                field_fingerprint: next.fingerprint(),
                evidence_ids: next.evidence.iter().map(|item| item.id.clone()).collect(),
            };
            playbook_huddle::apply_huddle(&mut next.huddles, &huddle_event, &view, policy)?;
            if let DriveEvent::HuddleRequested { at_ms, .. } = event {
                next.at_ms = *at_ms;
            }
        }
        DriveEvent::BlitzProposed { .. }
        | DriveEvent::BlitzAuthorised { .. }
        | DriveEvent::BlitzRejected { .. }
        | DriveEvent::BlitzTargetFrozen { .. }
        | DriveEvent::BlitzWorkerDispatched { .. }
        | DriveEvent::BlitzWorkerStarted { .. }
        | DriveEvent::BlitzWorkerExited { .. }
        | DriveEvent::BlitzWorkerSpent { .. }
        | DriveEvent::BlitzWaveClosed { .. }
        | DriveEvent::BlitzCandidateReceived { .. }
        | DriveEvent::BlitzVerificationStarted { .. }
        | DriveEvent::BlitzCandidateRejected { .. }
        | DriveEvent::BlitzCandidateVerified { .. }
        | DriveEvent::BlitzVerificationUnavailable { .. }
        | DriveEvent::BlitzHumanConfirmed { .. }
        | DriveEvent::BlitzWinnerSelected { .. }
        | DriveEvent::BlitzCancelStarted { .. }
        | DriveEvent::BlitzWorkerCancelled { .. }
        | DriveEvent::BlitzCancelComplete { .. }
        | DriveEvent::BlitzRecoveryConfirmed { .. }
        | DriveEvent::BlitzBudgetConsumed { .. }
        | DriveEvent::BlitzExhausted { .. }
        | DriveEvent::BlitzEscalated { .. }
        | DriveEvent::BlitzSuperseded { .. }
        | DriveEvent::BlitzFailed { .. }
        | DriveEvent::BlitzAborted { .. } => {
            let blitz_event = project_blitz(event).expect("blitz projection covers blitz events");
            let remaining = next.budget_initial.remaining(next.budget_consumed);
            let view = playbook_blitz::BlitzView {
                active: !matches!(
                    next.status,
                    DriveStatus::Completed
                        | DriveStatus::Abandoned
                        | DriveStatus::FieldGoalScored
                        | DriveStatus::Punted
                ),
                remaining_tokens: remaining.tokens,
                remaining_cost_microunits: remaining.monetary_cost_micros,
            };
            playbook_blitz::apply_blitz(&mut next.blitzes, &blitz_event, &view, policy)?;
            if let DriveEvent::BlitzProposed { at_ms, .. } = event {
                next.at_ms = *at_ms;
            }
        }
        DriveEvent::GameContractSealed { contract, at_ms }
        | DriveEvent::GameContractAmended { contract, at_ms } => {
            ensure_active(&next)?;
            contract.verify()?;
            if let Some(existing) = &next.game_contract {
                // Immutability: the only lawful successor names its
                // predecessor. Silent mutation is a replay divergence.
                if contract.predecessor_hash.as_deref() != Some(existing.contract_hash.as_str()) {
                    return Err(PlaybookError::Inadmissible(
                        "contract".into(),
                        "amendment does not name the seated predecessor".into(),
                    ));
                }
            } else if contract.predecessor_hash.is_some() {
                return Err(PlaybookError::Inadmissible(
                    "contract".into(),
                    "the first contract has no predecessor".into(),
                ));
            }
            next.game_contract = Some((**contract).clone());
            next.at_ms = *at_ms;
        }
        DriveEvent::EvaluationDecided {
            at_ms,
            branch,
            reasons,
        } => {
            ensure_active(&next)?;
            if reasons.is_empty() || reasons.iter().any(|reason| reason.trim().is_empty()) {
                return Err(PlaybookError::Inadmissible(
                    "evaluation".into(),
                    "every branch decision carries its reasons".into(),
                ));
            }
            next.last_evaluation = Some(crate::EvaluationRecord {
                branch: *branch,
                reasons: reasons.clone(),
                at_ms: *at_ms,
            });
            next.at_ms = *at_ms;
        }
        DriveEvent::PlayerSubstituted {
            at_ms,
            class,
            out,
            replacement,
            reasons,
        } => {
            ensure_active(&next)?;
            if !matches!(
                class.as_str(),
                "tactical" | "performance" | "availability" | "economic" | "emergency"
            ) {
                return Err(PlaybookError::Inadmissible(
                    "bench".into(),
                    "unknown substitution class".into(),
                ));
            }
            if reasons.is_empty() || reasons.iter().any(|reason| reason.trim().is_empty()) {
                return Err(PlaybookError::Inadmissible(
                    "bench".into(),
                    "every substitution carries its reasons".into(),
                ));
            }
            next.substitutions.push(crate::SubstitutionRecord {
                class: class.clone(),
                out: out.clone(),
                replacement: replacement.clone(),
                reasons: reasons.clone(),
                at_ms: *at_ms,
            });
            next.at_ms = *at_ms;
        }
        DriveEvent::CersRecovered {
            artefact_id,
            source,
            confirmed_facts,
            failed_assumptions,
            reuse_hint,
            at_ms,
        } => {
            ensure_active(&next)?;
            if *confirmed_facts + *failed_assumptions == 0 {
                return Err(PlaybookError::Evidence(
                    "a recovery artefact with no recoverable cognition is no artefact".into(),
                ));
            }
            if reuse_hint.trim().is_empty() {
                return Err(PlaybookError::Evidence(
                    "a recovery artefact names its reuse hint".into(),
                ));
            }
            next.cers_recovered_total = next.cers_recovered_total.saturating_add(1);
            if let Some(record) = next
                .cers_artefacts
                .iter_mut()
                .find(|record| record.artefact_id == *artefact_id)
            {
                record.times_reused = record.times_reused.saturating_add(1);
            } else {
                next.cers_artefacts.push(crate::CersRecord {
                    artefact_id: artefact_id.clone(),
                    source: source.clone(),
                    confirmed_facts: *confirmed_facts,
                    failed_assumptions: *failed_assumptions,
                    times_reused: 0,
                    at_ms: *at_ms,
                });
            }
            next.at_ms = *at_ms;
        }
        DriveEvent::DirectiveSealed { directive, at_ms } => {
            ensure_active(&next)?;
            let directive = directive.clone().seal()?;
            next.directive = Some(directive);
            next.at_ms = *at_ms;
        }
        DriveEvent::DirectiveExecuted { report, at_ms } => {
            ensure_active(&next)?;
            let directive = next.directive.as_ref().ok_or_else(|| {
                PlaybookError::Inadmissible("tt".into(), "no sealed directive to execute".into())
            })?;
            if report.directive_id != directive.id {
                return Err(PlaybookError::Inadmissible(
                    "tt".into(),
                    "report names another directive".into(),
                ));
            }
            // The Tinker implements the specification; it never rewrites it.
            if !directive.spec_matches(&report.specification_hash) {
                return Err(PlaybookError::ReplayDivergence(
                    "tinker reported against a rewritten specification".into(),
                ));
            }
            if !report.conformance && report.deviation.is_none() {
                return Err(PlaybookError::Inadmissible(
                    "tt".into(),
                    "a non-conformant report names its deviation".into(),
                ));
            }
            if report.conformance {
                next.tinker_conformances = next.tinker_conformances.saturating_add(1);
            } else {
                next.tinker_deviations = next.tinker_deviations.saturating_add(1);
                // Control returns to Playbook: the possession restarts.
                next.possession_restarted = true;
            }
            next.at_ms = *at_ms;
        }
    }
    Ok(next)
}

/// One-to-one projection from journal events onto the huddle fold.
pub(crate) fn project_huddle(event: &DriveEvent) -> Option<playbook_huddle::HuddleEvent> {
    Some(match event {
        DriveEvent::HuddleRequested {
            huddle_id,
            scope,
            trigger,
            at_ms,
            debt,
            rework_avoided,
        } => playbook_huddle::HuddleEvent::Requested {
            huddle_id: huddle_id.clone(),
            scope: *scope,
            trigger: trigger.clone(),
            at_ms: *at_ms,
            debt: *debt,
            rework_avoided: *rework_avoided,
        },
        DriveEvent::HuddleRejected {
            huddle_id,
            reason_code,
        } => playbook_huddle::HuddleEvent::Rejected {
            huddle_id: huddle_id.clone(),
            reason_code: reason_code.clone(),
        },
        DriveEvent::HuddleAdmitted {
            huddle_id,
            budget,
            barrier,
            participants,
        } => playbook_huddle::HuddleEvent::Admitted {
            huddle_id: huddle_id.clone(),
            budget: *budget,
            barrier: *barrier,
            participants: participants.clone(),
        },
        DriveEvent::HuddleOpened { huddle_id } => playbook_huddle::HuddleEvent::Opened {
            huddle_id: huddle_id.clone(),
        },
        DriveEvent::HuddleParticipantInvited {
            huddle_id,
            participant,
        } => playbook_huddle::HuddleEvent::ParticipantInvited {
            huddle_id: huddle_id.clone(),
            participant: participant.clone(),
        },
        DriveEvent::HuddleReportSubmitted { huddle_id, report } => {
            playbook_huddle::HuddleEvent::ReportSubmitted {
                huddle_id: huddle_id.clone(),
                report: (**report).clone(),
            }
        }
        DriveEvent::HuddleContradictionDetected {
            huddle_id,
            contradiction,
        } => playbook_huddle::HuddleEvent::ContradictionDetected {
            huddle_id: huddle_id.clone(),
            contradiction: contradiction.clone(),
        },
        DriveEvent::HuddleBudgetConsumed { huddle_id, cost } => {
            playbook_huddle::HuddleEvent::BudgetConsumed {
                huddle_id: huddle_id.clone(),
                cost: *cost,
            }
        }
        DriveEvent::HuddleOutcomeCommitted { huddle_id, outcome } => {
            playbook_huddle::HuddleEvent::OutcomeCommitted {
                huddle_id: huddle_id.clone(),
                outcome: (**outcome).clone(),
            }
        }
        DriveEvent::HuddleEscalationRequested {
            huddle_id,
            to_scope,
            reason_code,
        } => playbook_huddle::HuddleEvent::EscalationRequested {
            huddle_id: huddle_id.clone(),
            to_scope: *to_scope,
            reason_code: reason_code.clone(),
        },
        DriveEvent::HuddleEscalated {
            huddle_id,
            new_huddle_id,
            to_scope,
        } => playbook_huddle::HuddleEvent::Escalated {
            huddle_id: huddle_id.clone(),
            new_huddle_id: new_huddle_id.clone(),
            to_scope: *to_scope,
        },
        DriveEvent::HuddleClosed {
            huddle_id,
            reason_code,
        } => playbook_huddle::HuddleEvent::Closed {
            huddle_id: huddle_id.clone(),
            reason_code: reason_code.clone(),
        },
        _ => return None,
    })
}

/// One-to-one projection from journal events onto the blitz fold.
pub(crate) fn project_blitz(event: &DriveEvent) -> Option<playbook_blitz::BlitzEvent> {
    Some(match event {
        DriveEvent::BlitzProposed {
            blitz_id,
            trigger,
            target,
            pressure,
            at_ms,
        } => playbook_blitz::BlitzEvent::Proposed {
            blitz_id: blitz_id.clone(),
            trigger: trigger.clone(),
            target: Box::new(target.clone()),
            pressure: pressure.clone(),
            at_ms: *at_ms,
        },
        DriveEvent::BlitzAuthorised {
            blitz_id,
            budget,
            corroboration,
            resource_mode,
            at_ms,
        } => playbook_blitz::BlitzEvent::Authorised {
            blitz_id: blitz_id.clone(),
            budget: *budget,
            corroboration: *corroboration,
            resource_mode: *resource_mode,
            at_ms: *at_ms,
        },
        DriveEvent::BlitzRejected {
            blitz_id,
            reason_code,
        } => playbook_blitz::BlitzEvent::Rejected {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
        DriveEvent::BlitzTargetFrozen {
            blitz_id,
            target_hash,
        } => playbook_blitz::BlitzEvent::TargetFrozen {
            blitz_id: blitz_id.clone(),
            target_hash: target_hash.clone(),
        },
        DriveEvent::BlitzWorkerDispatched {
            blitz_id,
            worker,
            effective_parallelism_milli,
        } => playbook_blitz::BlitzEvent::WorkerDispatched {
            blitz_id: blitz_id.clone(),
            worker: worker.clone(),
            effective_parallelism_milli: *effective_parallelism_milli,
        },
        DriveEvent::BlitzWorkerStarted {
            blitz_id,
            worker_id,
            at_ms,
        } => playbook_blitz::BlitzEvent::WorkerStarted {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            at_ms: *at_ms,
        },
        DriveEvent::BlitzWorkerExited {
            blitz_id,
            worker_id,
            kind,
            at_ms,
        } => playbook_blitz::BlitzEvent::WorkerExited {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            kind: *kind,
            at_ms: *at_ms,
        },
        DriveEvent::BlitzWorkerSpent { blitz_id, spent } => {
            playbook_blitz::BlitzEvent::WorkerSpent {
                blitz_id: blitz_id.clone(),
                spent: spent.clone(),
            }
        }
        DriveEvent::BlitzWaveClosed {
            blitz_id,
            wave,
            diagnostics,
        } => playbook_blitz::BlitzEvent::WaveClosed {
            blitz_id: blitz_id.clone(),
            wave: *wave,
            diagnostics: diagnostics.clone(),
        },
        DriveEvent::BlitzCandidateReceived {
            blitz_id,
            candidate,
        } => playbook_blitz::BlitzEvent::CandidateReceived {
            blitz_id: blitz_id.clone(),
            candidate: candidate.clone(),
        },
        DriveEvent::BlitzVerificationStarted {
            blitz_id,
            candidate_id,
        } => playbook_blitz::BlitzEvent::VerificationStarted {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
        },
        DriveEvent::BlitzCandidateRejected {
            blitz_id,
            candidate_id,
            gate,
            reason_code,
        } => playbook_blitz::BlitzEvent::CandidateRejected {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            gate: *gate,
            reason_code: reason_code.clone(),
        },
        DriveEvent::BlitzCandidateVerified {
            blitz_id,
            candidate_id,
            passed,
        } => playbook_blitz::BlitzEvent::CandidateVerified {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            passed: passed.clone(),
        },
        DriveEvent::BlitzVerificationUnavailable {
            blitz_id,
            candidate_id,
        } => playbook_blitz::BlitzEvent::VerificationUnavailable {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
        },
        DriveEvent::BlitzHumanConfirmed {
            blitz_id,
            candidate_id,
            authority,
        } => playbook_blitz::BlitzEvent::HumanConfirmed {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            authority: authority.clone(),
        },
        DriveEvent::BlitzWinnerSelected {
            blitz_id,
            candidate_id,
            worker_id,
            at_ms,
        } => playbook_blitz::BlitzEvent::WinnerSelected {
            blitz_id: blitz_id.clone(),
            candidate_id: candidate_id.clone(),
            worker_id: worker_id.clone(),
            at_ms: *at_ms,
        },
        DriveEvent::BlitzCancelStarted { blitz_id, at_ms } => {
            playbook_blitz::BlitzEvent::CancelStarted {
                blitz_id: blitz_id.clone(),
                at_ms: *at_ms,
            }
        }
        DriveEvent::BlitzWorkerCancelled {
            blitz_id,
            worker_id,
            avoided_tokens,
            avoided_cost_microunits,
        } => playbook_blitz::BlitzEvent::WorkerCancelled {
            blitz_id: blitz_id.clone(),
            worker_id: worker_id.clone(),
            avoided_tokens: *avoided_tokens,
            avoided_cost_microunits: *avoided_cost_microunits,
        },
        DriveEvent::BlitzCancelComplete {
            blitz_id,
            cancelled,
            latency_ms,
            avoided_tokens,
            avoided_cost_microunits,
        } => playbook_blitz::BlitzEvent::CancelComplete {
            blitz_id: blitz_id.clone(),
            cancelled: *cancelled,
            latency_ms: *latency_ms,
            avoided_tokens: *avoided_tokens,
            avoided_cost_microunits: *avoided_cost_microunits,
        },
        DriveEvent::BlitzRecoveryConfirmed {
            blitz_id,
            evidence_refs,
            at_ms,
        } => playbook_blitz::BlitzEvent::RecoveryConfirmed {
            blitz_id: blitz_id.clone(),
            evidence_refs: evidence_refs.clone(),
            at_ms: *at_ms,
        },
        DriveEvent::BlitzBudgetConsumed { blitz_id, cost } => {
            playbook_blitz::BlitzEvent::BudgetConsumed {
                blitz_id: blitz_id.clone(),
                cost: *cost,
            }
        }
        DriveEvent::BlitzExhausted { blitz_id, failure } => playbook_blitz::BlitzEvent::Exhausted {
            blitz_id: blitz_id.clone(),
            failure: *failure,
        },
        DriveEvent::BlitzEscalated {
            blitz_id,
            recommendation,
        } => playbook_blitz::BlitzEvent::Escalated {
            blitz_id: blitz_id.clone(),
            recommendation: recommendation.clone(),
        },
        DriveEvent::BlitzSuperseded {
            blitz_id,
            successor_id,
        } => playbook_blitz::BlitzEvent::Superseded {
            blitz_id: blitz_id.clone(),
            successor_id: successor_id.clone(),
        },
        DriveEvent::BlitzFailed {
            blitz_id,
            reason_code,
        } => playbook_blitz::BlitzEvent::Failed {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
        DriveEvent::BlitzAborted {
            blitz_id,
            reason_code,
        } => playbook_blitz::BlitzEvent::Aborted {
            blitz_id: blitz_id.clone(),
            reason_code: reason_code.clone(),
        },
        _ => return None,
    })
}

fn close_cycle(before: &DriveState, next: &mut DriveState, policy: &Policy) {
    let spent = consumed_increased(before.budget_consumed, next.budget_consumed);
    if next.ytg_milli < before.ytg_milli {
        next.stall_cycles = 0;
        next.progress_cycles = next.progress_cycles.saturating_add(1);
        next.validated_progress = true;
    } else if spent {
        next.stall_cycles = next.stall_cycles.saturating_add(1);
    }
    if next.ytg_milli > before.ytg_milli && next.count_regression {
        next.material_failure = true;
        next.cycle_failure = true;
        next.failure_class = Some(playbook_core::FailureClass::TaskRegression);
    }
    if next
        .gates
        .iter()
        .any(|gate| gate.satisfaction_ppm > 0 || gate.is_terminal())
    {
        next.validated_progress = true;
    }
    let measurement = measurement_between(before, next, policy);
    let class = classify_measurement(&measurement, policy);
    next.cycle_previous_ytg = before.ytg_milli;
    next.down = class.down;
    next.explanation = class.reasons;
    next.completion_ppm = class.completion_ppm;
    next.possession_restarted = false;
    next.count_regression = false;
    if class.down == Down::FourthDown {
        next.disposition = Some(doctrine(next, policy).chosen);
    }
    if class.down == Down::ThirdDownPlayAction {
        next.requirement = requirement(next, policy);
    }
    score_field_goal(next);
}

fn ingest(state: &mut DriveState, evidence: &Evidence) -> Result<(), PlaybookError> {
    if state
        .evidence
        .iter()
        .any(|existing| existing.id == evidence.id)
    {
        return Ok(());
    }
    let outcome = apply_evidence(&mut state.gates, evidence)?;
    state.evidence.push(evidence.clone());
    state.gate_reopens = state.gate_reopens.saturating_add(outcome.reopened);
    if let Some(environment) = outcome.environment {
        state.environment_snapshot = environment;
    }
    let before = state.ytg_milli;
    state.refresh_ytg();
    if state.ytg_milli > before {
        state.count_regression = true;
    }
    if !outcome.failed.is_empty() {
        state.cycle_failure = true;
        state.material_failure = true;
        if state.failure_class.is_none() {
            state.failure_class = Some(playbook_core::FailureClass::ValidationFailure);
        }
    }
    Ok(())
}

fn doctrine(state: &DriveState, policy: &Policy) -> playbook_lastdown::DoctrineDecision {
    let full = state
        .gates
        .iter()
        .map(|gate| u64::from(gate.weight_ppm))
        .sum::<u64>()
        / 1_000;
    let banked = state
        .gates
        .iter()
        .map(|gate| {
            u64::from(gate.weight_ppm) * u64::from(gate.effective_satisfaction_ppm())
                / u64::from(PPM_SCALE)
        })
        .sum::<u64>()
        / 1_000;
    evaluate(&DoctrineInput {
        integrity: state.task.integrity,
        full_utility: full,
        banked_utility: banked,
        p_score: state.completion_ppm,
        p_preserve: policy.punt_preserve_ppm,
        p_finish_after_handoff: policy.receiver_finish_ppm,
        min_attempt: policy.min_attempt_ppm,
        reset_bonus_ppm: policy.reset_bonus_ppm,
        coaching_contested: coaching_contested(state),
    })
}

/// The most recent coordinator-scope reconciliation contested the conversion
/// plan: its outcome carried a strategy change. Fourth-down doctrine then
/// banks instead of gambling on another convert.
fn coaching_contested(state: &DriveState) -> bool {
    state
        .huddles()
        .closed()
        .iter()
        .rev()
        .find(|record| record.scope.rank() >= playbook_huddle::HuddleScope::Coordinator.rank())
        .and_then(|record| record.outcome.as_ref())
        .and_then(|outcome| outcome.strategy_change.as_ref())
        .is_some()
}

fn requirement(state: &DriveState, policy: &Policy) -> Option<CapabilityRequirement> {
    let gate = state.gates.iter().find(|gate| {
        gate.mandatory
            && !gate.is_terminal()
            && gate.required_evidence.iter().all(|class| {
                !state
                    .capabilities
                    .iter()
                    .any(|capability| capability.evidence_classes.contains(class))
            })
    })?;
    let class = *gate.required_evidence.first()?;
    let strategy = if state.acquisition_count >= policy.capability_acquisition_limit {
        ProvisionStrategy::Defer
    } else {
        ProvisionStrategy::DiscoverExisting
    };
    Some(CapabilityRequirement {
        capability_id: format!("missing.{class:?}"),
        capability_class: format!("{class:?}"),
        reason: format!("mandatory gate {} requires {class:?}", gate.id),
        blocks_gate: gate.id.clone(),
        provision_strategy: strategy,
        estimated_cost: DriveBudget {
            tokens: 10,
            ..DriveBudget::zero()
        },
        reusable: true,
    })
}

fn score_field_goal(state: &mut DriveState) {
    if let Some(gate_id) = &state.field_goal_gate {
        if state
            .gates
            .iter()
            .any(|gate| &gate.id == gate_id && gate.is_terminal())
        {
            state.status = DriveStatus::FieldGoalScored;
        }
    }
}

fn ensure_active(state: &DriveState) -> Result<(), PlaybookError> {
    if matches!(
        state.status,
        DriveStatus::Completed
            | DriveStatus::Abandoned
            | DriveStatus::FieldGoalScored
            | DriveStatus::Punted
    ) {
        Err(PlaybookError::Closed)
    } else {
        Ok(())
    }
}

fn note_idempotency_start(state: &mut DriveState, key: Option<&str>) -> Result<(), PlaybookError> {
    let Some(key) = key else {
        return Ok(());
    };
    match state.idempotency.get(key) {
        Some(IdempotencyState::Committed) => Err(PlaybookError::DuplicateEffect(key.to_string())),
        Some(IdempotencyState::Unknown | IdempotencyState::Started) => {
            Err(PlaybookError::Reconcile(key.to_string()))
        }
        None => {
            state
                .idempotency
                .insert(key.to_string(), IdempotencyState::Started);
            Ok(())
        }
    }
}

fn settle_effects(
    state: &mut DriveState,
    key: Option<&str>,
    effects: &[playbook_core::SideEffectNote],
) {
    let unresolved = effects.iter().any(|effect| !effect.resolved);
    if let Some(key) = key {
        let mark = if unresolved {
            IdempotencyState::Unknown
        } else {
            IdempotencyState::Committed
        };
        state.idempotency.insert(key.to_string(), mark);
    }
    for effect in effects {
        if !effect.resolved {
            state.unresolved_effects.push(effect.clone());
        }
    }
}

fn ensure_no_open_execution(state: &DriveState) -> Result<(), PlaybookError> {
    if let Some(execution) = &state.open_execution {
        return Err(PlaybookError::Reconcile(
            execution
                .idempotency_key
                .clone()
                .unwrap_or_else(|| execution.play_id.to_string()),
        ));
    }
    if let Some(effect) = state.unresolved_effects.first() {
        return Err(PlaybookError::Reconcile(effect.idempotency_key.clone()));
    }
    Ok(())
}

fn validate_effect_keys(
    key: Option<&str>,
    effects: &[SideEffectNote],
    operation: &str,
) -> Result<(), PlaybookError> {
    let Some(key) = key else {
        return Err(PlaybookError::Inadmissible(
            operation.to_string(),
            "execution has no idempotency key".into(),
        ));
    };
    if effects.iter().any(|effect| effect.idempotency_key != key) {
        return Err(PlaybookError::Inadmissible(
            operation.to_string(),
            "side effect note does not match the invocation idempotency key".into(),
        ));
    }
    Ok(())
}

fn saturating_add_budget(left: DriveBudget, right: DriveBudget) -> DriveBudget {
    DriveBudget {
        tokens: left.tokens.saturating_add(right.tokens),
        monetary_cost_micros: left
            .monetary_cost_micros
            .saturating_add(right.monetary_cost_micros),
        wall_clock_ms: left.wall_clock_ms.saturating_add(right.wall_clock_ms),
        tool_calls: left.tool_calls.saturating_add(right.tool_calls),
        inference_calls: left.inference_calls.saturating_add(right.inference_calls),
        retry_count: left.retry_count.saturating_add(right.retry_count),
        context_capacity: left.context_capacity.saturating_add(right.context_capacity),
        provider_quota: left.provider_quota.saturating_add(right.provider_quota),
    }
}

fn admit_selected(
    state: &DriveState,
    definition: &playbook_core::PlayDefinition,
    personnel: &playbook_core::PersonnelId,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    let person = playbook_core::Personnel {
        id: personnel.clone(),
        kind: definition.personnel_kind,
        display_name: personnel.to_string(),
        cost_tokens: 1,
        latency_ms: 1,
        capability_fit_ppm: PPM_SCALE,
        reliability_ppm: PPM_SCALE,
        quota_remaining_ppm: PPM_SCALE,
        evidence_quality_ppm: PPM_SCALE,
        own_ytg_milli: None,
    };
    let field = state.field_position(std::slice::from_ref(&person), policy)?;
    admit(&field, definition, personnel, policy)
}

fn consumed_increased(before: DriveBudget, after: DriveBudget) -> bool {
    after.tokens > before.tokens
        || after.monetary_cost_micros > before.monetary_cost_micros
        || after.wall_clock_ms > before.wall_clock_ms
        || after.tool_calls > before.tool_calls
        || after.inference_calls > before.inference_calls
        || after.retry_count > before.retry_count
        || after.context_capacity > before.context_capacity
        || after.provider_quota > before.provider_quota
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{
        DriveId, Evidence, EvidenceClass, Gate, GateId, TaskContract, TaskIntegrity,
    };
    use playbook_policy::Policy;
    use serde_json::json;

    #[test]
    fn authority_log_json_round_trips_and_audit_fold_matches_live_state() {
        let policy = Policy::builtin();
        let task = TaskContract {
            statement: "compile the implementation".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        };
        let mut authority = crate::Authority::start(
            DriveId::new("json-roundtrip").unwrap(),
            task,
            DriveBudget::default(),
            policy.clone(),
            Vec::new(),
            1,
        )
        .unwrap();
        let gate = Gate::declare(
            GateId::new("implementation").unwrap(),
            "compiler accepts the implementation",
            1_000_000,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap();
        authority.declare_gate(gate).unwrap();
        authority
            .accept_evidence(
                Evidence::seal(
                    EvidenceClass::CompilerResult,
                    json!({"gate_id": "implementation", "success": true}),
                    2,
                )
                .unwrap(),
            )
            .unwrap();

        let json = export_log(authority.log()).unwrap();
        let imported = import_log(&json).unwrap();
        assert_eq!(imported, authority.log());
        assert_eq!(
            fold(&imported, &policy, ApplyMode::Audit).unwrap(),
            *authority.state()
        );
    }

    #[test]
    fn priors_learn_from_measured_outcomes_and_replay_identically() {
        use playbook_core::{Personnel, PersonnelId, PersonnelKind, YTG_REFERENCE_MILLI};
        use playbook_elci::FixturePlane;

        let policy = Policy::builtin();
        let cargo = PersonnelId::new("cargo-toolchain").unwrap();
        let roster = vec![Personnel {
            id: cargo.clone(),
            kind: PersonnelKind::DeterministicProcedure,
            display_name: "cargo-toolchain".into(),
            cost_tokens: 40,
            latency_ms: 20,
            capability_fit_ppm: playbook_core::PPM_SCALE,
            reliability_ppm: 900_000,
            quota_remaining_ppm: playbook_core::PPM_SCALE,
            evidence_quality_ppm: playbook_core::PPM_SCALE,
            own_ytg_milli: None,
        }];
        let mut authority = crate::Authority::start(
            DriveId::new("drive-priors").unwrap(),
            TaskContract {
                statement: "compile the implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            crate::started_budget(1_000, 1_000),
            policy.clone(),
            roster,
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
        let mut plane = FixturePlane::mvp();
        authority.preflight(&mut plane).unwrap();
        let survey = authority.step(&mut plane, 100).unwrap();
        assert_eq!(survey.selected_operation, "inspect");
        let compile = authority.step(&mut plane, 200).unwrap();
        assert_eq!(compile.selected_operation, "compile");

        let priors = authority.state().priors();
        let row = priors.row(&cargo).unwrap();
        // The survey moved no yards; the compile removed them all.
        assert_eq!(row.samples, 2);
        assert_eq!(
            row.realized_reduction_sum_milli,
            u64::from(YTG_REFERENCE_MILLI)
        );
        assert_eq!(
            priors.prior_milli(&cargo, policy.routing.prior_min_samples),
            None
        );
        assert_eq!(priors.prior_milli(&cargo, 2), Some(YTG_REFERENCE_MILLI / 2));

        let events = authority.log().to_vec();
        let audit = fold(&events, &policy, ApplyMode::Audit).unwrap();
        let resim = fold(&events, &policy, ApplyMode::Resimulate).unwrap();
        assert_eq!(audit.priors(), authority.state().priors());
        assert_eq!(resim.priors(), authority.state().priors());
        assert_eq!(audit.priors(), resim.priors());
    }

    #[test]
    fn loaded_priors_flip_routing_across_drives() {
        use playbook_core::{Personnel, PersonnelId, PersonnelKind};
        use playbook_elci::FixturePlane;
        use playbook_routing::HistoricalPriorTable;

        let policy = Policy::builtin();
        let mut priors = HistoricalPriorTable::empty();
        let measured = PersonnelId::new("measured-tool").unwrap();
        for _ in 0..policy.routing.prior_min_samples {
            priors.record(&measured, 3_000);
        }

        let roster = || {
            vec![
                Personnel {
                    id: PersonnelId::new("fast-tool").unwrap(),
                    kind: PersonnelKind::DeterministicProcedure,
                    display_name: "fast-tool".into(),
                    cost_tokens: 40,
                    latency_ms: 20,
                    capability_fit_ppm: playbook_core::PPM_SCALE,
                    reliability_ppm: 900_000,
                    quota_remaining_ppm: playbook_core::PPM_SCALE,
                    evidence_quality_ppm: playbook_core::PPM_SCALE,
                    own_ytg_milli: None,
                },
                Personnel {
                    id: measured.clone(),
                    kind: PersonnelKind::DeterministicProcedure,
                    display_name: "measured-tool".into(),
                    cost_tokens: 40,
                    latency_ms: 20,
                    capability_fit_ppm: playbook_core::PPM_SCALE,
                    reliability_ppm: 900_000,
                    quota_remaining_ppm: playbook_core::PPM_SCALE,
                    evidence_quality_ppm: playbook_core::PPM_SCALE,
                    own_ytg_milli: None,
                },
            ]
        };
        let setup = |authority: &mut crate::Authority| {
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
            let mut plane = FixturePlane::mvp();
            authority.preflight(&mut plane).unwrap();
            authority.step(&mut plane, 100).unwrap();
            authority.step(&mut plane, 200).unwrap()
        };

        // Without history the tie breaks alphabetically.
        let mut fresh = crate::Authority::start(
            DriveId::new("drive-blind").unwrap(),
            TaskContract {
                statement: "compile the implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            crate::started_budget(1_000, 1_000),
            policy.clone(),
            roster(),
            1,
        )
        .unwrap();
        let blind = setup(&mut fresh);
        assert_eq!(
            blind.selected_personnel.as_ref().unwrap().as_str(),
            "fast-tool"
        );

        // The same drive with imported history routes by measured record.
        let mut schooled = crate::Authority::start(
            DriveId::new("drive-schooled").unwrap(),
            TaskContract {
                statement: "compile the implementation".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            crate::started_budget(1_000, 1_000),
            policy.clone(),
            roster(),
            1,
        )
        .unwrap();
        schooled.load_priors(priors.clone()).unwrap();
        let taught = setup(&mut schooled);
        assert_eq!(
            taught.selected_personnel.as_ref().unwrap().as_str(),
            "measured-tool"
        );

        // The load is part of the log, and the drive kept learning on top.
        let replay = fold(schooled.log(), &policy, ApplyMode::Audit).unwrap();
        assert_eq!(replay, *schooled.state());
        let row = schooled.state().priors().row(&measured).unwrap();
        assert_eq!(row.samples, priors.row(&measured).unwrap().samples + 2);
    }
}
