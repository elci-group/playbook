//! Huddle state machine.
//!
//! Idle → Requested → AdmissionCheck → Open → CollectReports →
//! Reconciliation → Closed. Every rule here is enforced at fold time, so a
//! replay of the journal reproduces the same huddle state or refuses the
//! tampered log.

use playbook_core::{BarrierMode, HuddleId, PlaybookError};
use playbook_policy::Policy;

use crate::admission::{refusal, rejected_closure};
use crate::budget;
use crate::domain::{
    ActiveHuddle, ContradictionSeverity, HuddleClosure, HuddleCost, HuddleLedger,
    HuddleParticipant, HuddlePhase, HuddleReport, HuddleScope, HuddleTrigger, ResolutionState,
};
use crate::escalation::is_upward;

/// The projection of a huddle event. `playbook-state` maps `DriveEvent`
/// variants onto this one-to-one and folds through `apply_huddle`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HuddleEvent {
    Requested {
        huddle_id: HuddleId,
        scope: HuddleScope,
        trigger: HuddleTrigger,
        at_ms: u64,
        debt: u32,
        rework_avoided: u32,
    },
    Rejected {
        huddle_id: HuddleId,
        reason_code: String,
    },
    Admitted {
        huddle_id: HuddleId,
        budget: playbook_core::HuddleBudget,
        barrier: BarrierMode,
        participants: Vec<HuddleParticipant>,
    },
    Opened {
        huddle_id: HuddleId,
    },
    ParticipantInvited {
        huddle_id: HuddleId,
        participant: HuddleParticipant,
    },
    ReportSubmitted {
        huddle_id: HuddleId,
        report: HuddleReport,
    },
    ContradictionDetected {
        huddle_id: HuddleId,
        contradiction: crate::domain::Contradiction,
    },
    BudgetConsumed {
        huddle_id: HuddleId,
        cost: HuddleCost,
    },
    OutcomeCommitted {
        huddle_id: HuddleId,
        outcome: crate::domain::HuddleOutcome,
    },
    EscalationRequested {
        huddle_id: HuddleId,
        to_scope: HuddleScope,
        reason_code: String,
    },
    Escalated {
        huddle_id: HuddleId,
        new_huddle_id: HuddleId,
        to_scope: HuddleScope,
    },
    Closed {
        huddle_id: HuddleId,
        reason_code: String,
    },
}

/// What the fold needs to know about the drive. No drive internals leak
/// into the huddle subsystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveView {
    pub active: bool,
    pub down: playbook_core::Down,
    pub ytg_milli: u32,
    pub field_fingerprint: String,
    pub evidence_ids: Vec<playbook_core::EvidenceId>,
}

impl DriveView {
    fn knows(&self, id: &playbook_core::EvidenceId) -> bool {
        self.evidence_ids.iter().any(|known| known == id)
    }

    fn knows_all(&self, ids: &[playbook_core::EvidenceId]) -> bool {
        ids.iter().all(|id| self.knows(id))
    }
}

pub fn fold_huddle(
    ledger: &mut HuddleLedger,
    events: &[HuddleEvent],
    view: &DriveView,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    for event in events {
        apply_huddle(ledger, event, view, policy)?;
    }
    Ok(())
}

pub fn apply_huddle(
    ledger: &mut HuddleLedger,
    event: &HuddleEvent,
    view: &DriveView,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    match event {
        HuddleEvent::Requested {
            huddle_id,
            scope,
            trigger,
            at_ms,
            debt,
            rework_avoided,
        } => {
            if !view.active {
                return Err(PlaybookError::Closed);
            }
            if ledger.known_id(huddle_id) {
                return Err(refusal("duplicate huddle id"));
            }
            if ledger.active_by_scope(*scope).is_some() {
                return Err(refusal("scope_busy"));
            }
            ledger.push_active(ActiveHuddle {
                id: huddle_id.clone(),
                scope: *scope,
                barrier: BarrierMode::Scope,
                phase: HuddlePhase::Requested,
                trigger: trigger.clone(),
                budget: playbook_core::HuddleBudget::zero(),
                spent: HuddleCost::zero(),
                participants: Vec::new(),
                reports: Vec::new(),
                contradictions: Vec::new(),
                exhausted: false,
                escalation: None,
                partition: Vec::new(),
                field_fingerprint: view.field_fingerprint.clone(),
                at_ms: *at_ms,
                debt_before: *debt,
                ytg_milli_before: view.ytg_milli,
                rework_avoided: *rework_avoided,
            });
            Ok(())
        }
        HuddleEvent::Rejected {
            huddle_id,
            reason_code,
        } => {
            {
                let huddle = ledger.active_mut(huddle_id)?;
                if huddle.phase != HuddlePhase::Requested {
                    return Err(refusal("only a requested huddle can be rejected"));
                }
            }
            ledger.count_rejected();
            ledger.close_active(huddle_id, rejected_closure(), reason_code.clone(), None)
        }
        HuddleEvent::Admitted {
            huddle_id,
            budget,
            barrier,
            participants,
        } => {
            if participants.is_empty() || participants.len() > usize::from(budget.max_participants)
            {
                return Err(refusal("participant count outside the envelope"));
            }
            let mut ids: Vec<&str> = participants
                .iter()
                .map(|participant| participant.personnel_id.as_str())
                .collect();
            let unique = ids.len();
            ids.sort_unstable();
            ids.dedup();
            if ids.len() != unique {
                return Err(refusal("duplicate participant"));
            }
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::Requested {
                return Err(refusal("only a requested huddle can be admitted"));
            }
            let mut partition = ids.iter().map(|id| (*id).to_string()).collect::<Vec<_>>();
            partition.extend(
                huddle
                    .trigger
                    .evidence_refs
                    .iter()
                    .map(|id| id.as_str().to_string()),
            );
            partition.sort_unstable();
            partition.dedup();
            for other in ledger.active() {
                if &other.id == huddle_id || other.phase == HuddlePhase::Requested {
                    continue;
                }
                if other.partition.iter().any(|key| partition.contains(key)) {
                    return Err(refusal("state partition is already locked"));
                }
            }
            let huddle = ledger.active_mut(huddle_id)?;
            huddle.budget = *budget;
            huddle.barrier = *barrier;
            huddle.participants = participants.clone();
            huddle.partition = partition;
            huddle.phase = HuddlePhase::Admitted;
            Ok(())
        }
        HuddleEvent::Opened { huddle_id } => {
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::Admitted {
                return Err(refusal("only an admitted huddle can open"));
            }
            huddle.phase = HuddlePhase::CollectReports;
            Ok(())
        }
        HuddleEvent::ParticipantInvited {
            huddle_id,
            participant,
        } => {
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::CollectReports {
                return Err(refusal("huddle is not collecting reports"));
            }
            if huddle.participants.len() >= usize::from(huddle.budget.max_participants) {
                return Err(refusal("participant envelope is full"));
            }
            if huddle
                .participants
                .iter()
                .any(|known| known.personnel_id == participant.personnel_id)
            {
                return Err(refusal("duplicate participant"));
            }
            huddle.participants.push(participant.clone());
            huddle
                .partition
                .push(participant.personnel_id.as_str().to_string());
            huddle.partition.sort_unstable();
            huddle.partition.dedup();
            Ok(())
        }
        HuddleEvent::ReportSubmitted { huddle_id, report } => {
            {
                let huddle = ledger.active_mut(huddle_id)?;
                if huddle.phase != HuddlePhase::CollectReports {
                    return Err(refusal("huddle is not collecting reports"));
                }
                if huddle.exhausted {
                    return Err(refusal("budget exhausted; synthesis only"));
                }
                let Some(participant) = huddle
                    .participants
                    .iter()
                    .find(|known| known.personnel_id == report.participant)
                else {
                    return Err(refusal("reporter was not invited"));
                };
                if !participant.role.reports() {
                    return Err(refusal("observer cannot report"));
                }
                if huddle.reported(&report.participant) {
                    return Err(refusal("duplicate report"));
                }
                if report.confidence_milli > 10_000 {
                    return Err(refusal("confidence_milli out of range"));
                }
            }
            ensure_report_evidence(report, view)?;
            let huddle = ledger.active_mut(huddle_id)?;
            for contradiction in &report.contradictions {
                if huddle.contradiction(&contradiction.id) {
                    return Err(refusal("duplicate contradiction id"));
                }
            }
            huddle.reports.push(report.clone());
            huddle
                .contradictions
                .extend(report.contradictions.iter().cloned());
            Ok(())
        }
        HuddleEvent::ContradictionDetected {
            huddle_id,
            contradiction,
        } => {
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::CollectReports {
                return Err(refusal("huddle is not collecting reports"));
            }
            for claim in &contradiction.claims {
                if let Some(evidence) = &claim.evidence_ref {
                    if !view.knows(evidence) {
                        return Err(refusal("unknown evidence reference"));
                    }
                }
            }
            if let Some(existing) = huddle
                .contradictions
                .iter_mut()
                .find(|known| known.id == contradiction.id)
            {
                // The same subject gathering more claims is a severity
                // upgrade, not a duplicate.
                if contradiction.claims.len() > existing.claims.len() {
                    *existing = contradiction.clone();
                    return Ok(());
                }
                return Err(refusal("duplicate contradiction id"));
            }
            huddle.contradictions.push(contradiction.clone());
            Ok(())
        }
        HuddleEvent::BudgetConsumed { huddle_id, cost } => {
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::CollectReports {
                return Err(refusal("huddle is not collecting reports"));
            }
            huddle.spent = budget::consume(huddle.spent, &huddle.budget, cost)?;
            huddle.exhausted = budget::exhausted(&huddle.spent, &huddle.budget);
            Ok(())
        }
        HuddleEvent::OutcomeCommitted { huddle_id, outcome } => {
            {
                let huddle = ledger.active_mut(huddle_id)?;
                if huddle.phase != HuddlePhase::CollectReports {
                    return Err(refusal("huddle is not collecting reports"));
                }
                if &outcome.huddle_id != huddle_id {
                    return Err(refusal("outcome names another huddle"));
                }
                let expected = huddle
                    .participants
                    .iter()
                    .filter(|participant| participant.role.reports())
                    .count();
                let reported = huddle.reports.len();
                if reported < expected && !huddle.exhausted {
                    return Err(refusal("outcome before every report"));
                }
            }
            validate_outcome(ledger, huddle_id, outcome, view, policy)?;
            apply_outcome(ledger, outcome)?;
            let closure = {
                let huddle = ledger.active_mut(huddle_id)?;
                if huddle.exhausted {
                    HuddleClosure::ForcedSynthesis
                } else if outcome.unresolved_contradictions.is_empty() {
                    HuddleClosure::Committed
                } else {
                    HuddleClosure::Partial
                }
            };
            ledger.count_committed(outcome.escalated_to.is_some());
            ledger.close_active(
                huddle_id,
                closure,
                "outcome_committed".into(),
                Some(outcome.clone()),
            )
        }
        HuddleEvent::EscalationRequested {
            huddle_id,
            to_scope,
            reason_code,
        } => {
            let huddle = ledger.active_mut(huddle_id)?;
            if huddle.phase != HuddlePhase::CollectReports {
                return Err(refusal("huddle is not collecting reports"));
            }
            if !is_upward(huddle.scope, *to_scope) {
                return Err(refusal("escalation must move up the ladder"));
            }
            if reason_code.trim().is_empty()
                || (huddle.contradictions.is_empty()
                    && huddle.trigger.kind == crate::domain::HuddleTriggerKind::Success)
            {
                return Err(refusal("escalation lacks scope justification"));
            }
            huddle.escalation = Some((*to_scope, reason_code.clone()));
            Ok(())
        }
        HuddleEvent::Escalated {
            huddle_id,
            new_huddle_id,
            to_scope,
        } => {
            {
                let (trigger, fingerprint, at_ms, debt_before, rework_avoided) = {
                    let huddle = ledger.active_mut(huddle_id)?;
                    if huddle.escalation.as_ref().map(|(scope, _)| scope) != Some(to_scope) {
                        return Err(refusal("no pending escalation to this scope"));
                    }
                    (
                        huddle.trigger.clone(),
                        huddle.field_fingerprint.clone(),
                        huddle.at_ms,
                        huddle.debt_before,
                        huddle.rework_avoided,
                    )
                };
                if ledger.known_id(new_huddle_id) {
                    return Err(refusal("duplicate huddle id"));
                }
                ledger.count_escalated();
                ledger.close_active(
                    huddle_id,
                    HuddleClosure::Escalated,
                    "escalated".into(),
                    None,
                )?;
                ledger.push_active(ActiveHuddle {
                    id: new_huddle_id.clone(),
                    scope: *to_scope,
                    barrier: BarrierMode::Scope,
                    phase: HuddlePhase::Requested,
                    trigger: HuddleTrigger {
                        kind: trigger.kind,
                        reason_code: format!("escalated_from_{}", huddle_id.as_str()),
                        evidence_refs: trigger.evidence_refs,
                        originating_event: trigger.originating_event,
                    },
                    budget: playbook_core::HuddleBudget::zero(),
                    spent: HuddleCost::zero(),
                    participants: Vec::new(),
                    reports: Vec::new(),
                    contradictions: Vec::new(),
                    exhausted: false,
                    escalation: None,
                    partition: Vec::new(),
                    field_fingerprint: fingerprint,
                    at_ms,
                    debt_before,
                    ytg_milli_before: view.ytg_milli,
                    rework_avoided,
                });
            }
            Ok(())
        }
        HuddleEvent::Closed {
            huddle_id,
            reason_code,
        } => {
            ledger.active_mut(huddle_id)?;
            ledger.close_active(huddle_id, HuddleClosure::Aborted, reason_code.clone(), None)
        }
    }
}

fn ensure_report_evidence(report: &HuddleReport, view: &DriveView) -> Result<(), PlaybookError> {
    if !view.knows_all(&report.verified_progress) {
        return Err(refusal("unknown evidence reference"));
    }
    for blocker in &report.blockers {
        if let Some(evidence) = &blocker.evidence_ref {
            if !view.knows(evidence) {
                return Err(refusal("unknown evidence reference"));
            }
        }
    }
    for evidence in report
        .changed_assumptions
        .iter()
        .map(|delta| &delta.evidence_ref)
        .chain(
            report
                .dependency_changes
                .iter()
                .map(|delta| &delta.evidence_ref),
        )
        .flatten()
    {
        if !view.knows(evidence) {
            return Err(refusal("unknown evidence reference"));
        }
    }
    for contradiction in &report.contradictions {
        for claim in &contradiction.claims {
            if let Some(evidence) = &claim.evidence_ref {
                if !view.knows(evidence) {
                    return Err(refusal("unknown evidence reference"));
                }
            }
        }
    }
    Ok(())
}

/// Contract checks for a committed outcome. No evidence resolution without
/// evidence; no historical rewriting; no untracked contradictions.
fn validate_outcome(
    ledger: &HuddleLedger,
    huddle_id: &HuddleId,
    outcome: &crate::domain::HuddleOutcome,
    view: &DriveView,
    policy: &Policy,
) -> Result<(), PlaybookError> {
    let huddle = ledger
        .active_by_id(huddle_id)
        .ok_or_else(|| refusal("unknown huddle"))?;
    let mut resolved_ids: Vec<&crate::domain::ContradictionId> = Vec::new();
    for resolution in &outcome.resolved_contradictions {
        if !huddle.contradiction(&resolution.contradiction_id) {
            return Err(refusal("resolution names an untracked contradiction"));
        }
        if resolved_ids.contains(&&resolution.contradiction_id) {
            return Err(refusal("contradiction resolved twice"));
        }
        resolved_ids.push(&resolution.contradiction_id);
        match resolution.state {
            ResolutionState::ResolvedByEvidence => {
                let Some(evidence) = &resolution.evidence_ref else {
                    return Err(PlaybookError::Evidence(
                        "resolution by evidence requires an evidence reference".into(),
                    ));
                };
                if !view.knows(evidence) {
                    return Err(PlaybookError::Evidence(
                        "resolution cites unknown evidence".into(),
                    ));
                }
            }
            ResolutionState::ResolvedByAuthority => {
                if matches!(resolution.authority.as_deref(), None | Some("")) {
                    return Err(PlaybookError::Evidence(
                        "resolution by authority requires a named authority".into(),
                    ));
                }
            }
            ResolutionState::UnresolvedBlocking => {
                return Err(refusal(
                    "unresolved blocking contradictions belong in unresolved_contradictions",
                ));
            }
            ResolutionState::ResolvedBySupersession | ResolutionState::Deferred => {}
        }
    }
    for contradiction in &outcome.unresolved_contradictions {
        if !huddle.contradiction(&contradiction.id) {
            return Err(refusal("unresolved contradiction was never detected"));
        }
    }
    if let Some(change) = &outcome.strategy_change {
        if change.justification.trim().is_empty() {
            return Err(refusal("strategy change requires justification"));
        }
        if view.down == playbook_core::Down::FourthDown
            && huddle.scope.rank() < HuddleScope::Coordinator.rank()
        {
            return Err(refusal(
                "strategy change on fourth down requires coordinator scope",
            ));
        }
    }
    if let Some(revised) = outcome.revised_ytg_milli {
        if revised != view.ytg_milli {
            return Err(PlaybookError::ReplayDivergence(format!(
                "huddle revised ytg {revised} but the measured field is {}",
                view.ytg_milli
            )));
        }
    }
    if let Some(proposal) = &outcome.next_play {
        if policy.play(&proposal.operation).is_none() {
            return Err(PlaybookError::Inadmissible(
                proposal.operation.clone(),
                "huddle proposed an unknown play".into(),
            ));
        }
    }
    if outcome
        .unresolved_contradictions
        .iter()
        .any(|contradiction| contradiction.severity == ContradictionSeverity::Critical)
        && outcome.escalated_to.is_none()
    {
        return Err(refusal(
            "blocking contradiction prevents unsafe progression",
        ));
    }
    Ok(())
}

/// Tactical effects. A huddle outcome can reassign personnel and queue
/// recommendations; it can never satisfy a gate or alter evidence.
fn apply_outcome(
    ledger: &mut HuddleLedger,
    outcome: &crate::domain::HuddleOutcome,
) -> Result<(), PlaybookError> {
    for change in &outcome.assignment_changes {
        let target = outcome
            .next_play
            .as_ref()
            .and_then(|proposal| proposal.target_gate.clone());
        ledger.apply_assignment(change, target);
    }
    ledger.record_substitutions(&outcome.personnel_changes);
    if let Some(proposal) = &outcome.next_play {
        ledger.record_proposal(proposal);
    }
    ledger.set_conditions(outcome.next_huddle_conditions.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AssumptionDelta, ClaimRef, Contradiction, ContradictionId, ContradictionSeverity,
        DisclosureProfile, HuddleOutcome, HuddleRole, SharedStateDelta,
    };
    use playbook_core::{Down, EventId, EvidenceId, HuddleBudget};

    fn policy() -> Policy {
        Policy::builtin()
    }

    fn view() -> DriveView {
        DriveView {
            active: true,
            down: Down::FirstDown,
            ytg_milli: 10_000,
            field_fingerprint: "field-a".into(),
            evidence_ids: vec![EvidenceId::new("ev-1").unwrap()],
        }
    }

    fn trigger() -> HuddleTrigger {
        HuddleTrigger {
            kind: crate::domain::HuddleTriggerKind::Failure,
            reason_code: "repeated_failed_play".into(),
            evidence_refs: vec![EvidenceId::new("ev-1").unwrap()],
            originating_event: EventId::new("evt-1").unwrap(),
        }
    }

    fn participant(id: &str) -> HuddleParticipant {
        HuddleParticipant {
            personnel_id: playbook_core::PersonnelId::new(id).unwrap(),
            role: HuddleRole::Contributor,
            disclosure_profile: DisclosureProfile {
                include_gates: true,
                include_peer_reports: true,
                include_strategic_state: false,
                max_evidence_refs: 8,
            },
        }
    }

    fn budget() -> HuddleBudget {
        HuddleBudget {
            max_tokens: 2_000,
            max_cost_microunits: 200_000,
            max_duration_ms: 5_000,
            max_rounds: 2,
            max_participants: 3,
        }
    }

    fn report(id: &str, current: &str) -> HuddleReport {
        HuddleReport {
            participant: playbook_core::PersonnelId::new(id).unwrap(),
            verified_progress: vec![EvidenceId::new("ev-1").unwrap()],
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

    fn contradiction(claims: usize) -> crate::domain::Contradiction {
        Contradiction {
            id: ContradictionId::new("con-000").unwrap(),
            subject: "interface".into(),
            claims: (0..claims)
                .map(|index| ClaimRef {
                    personnel_id: playbook_core::PersonnelId::new(format!("agent-{index}"))
                        .unwrap(),
                    statement: format!("v{index}"),
                    evidence_ref: None,
                })
                .collect(),
            severity: if claims >= 3 {
                ContradictionSeverity::High
            } else {
                ContradictionSeverity::Medium
            },
        }
    }

    fn outcome(id: &str) -> HuddleOutcome {
        HuddleOutcome {
            huddle_id: HuddleId::new(id).unwrap(),
            reconciled_state: SharedStateDelta {
                verified_evidence: vec![EvidenceId::new("ev-1").unwrap()],
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
        }
    }

    fn open_huddle(ledger: &mut HuddleLedger, id: &str) {
        let huddle_id = HuddleId::new(id).unwrap();
        fold_huddle(
            ledger,
            &[
                HuddleEvent::Requested {
                    huddle_id: huddle_id.clone(),
                    scope: HuddleScope::Position,
                    trigger: trigger(),
                    at_ms: 1,
                    debt: 0,
                    rework_avoided: 0,
                },
                HuddleEvent::Admitted {
                    huddle_id: huddle_id.clone(),
                    budget: budget(),
                    barrier: BarrierMode::Soft,
                    participants: vec![participant("agent-a"), participant("agent-b")],
                },
                HuddleEvent::Opened {
                    huddle_id: huddle_id.clone(),
                },
            ],
            &view(),
            &policy(),
        )
        .unwrap();
    }

    #[test]
    fn a_huddle_lifecycle_commits_and_closes() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        for author in ["agent-a", "agent-b"] {
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: report(author, "v1"),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        apply_huddle(
            &mut ledger,
            &HuddleEvent::BudgetConsumed {
                huddle_id: id.clone(),
                cost: HuddleCost {
                    tokens: 100,
                    cost_microunits: 10_000,
                    duration_ms: 100,
                    rounds: 1,
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::OutcomeCommitted {
                huddle_id: id.clone(),
                outcome: {
                    let mut committed = outcome("h-1");
                    committed.revised_ytg_milli = Some(10_000);
                    committed
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(ledger.active().is_empty());
        assert_eq!(ledger.committed_total(), 1);
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::Committed);
        assert_eq!(ledger.closed()[0].field_fingerprint, "field-a");
    }

    #[test]
    fn duplicate_reports_and_foreign_evidence_are_refused() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        let first = HuddleEvent::ReportSubmitted {
            huddle_id: id.clone(),
            report: report("agent-a", "v1"),
        };
        apply_huddle(&mut ledger, &first, &view(), &policy()).unwrap();
        assert_eq!(
            apply_huddle(&mut ledger, &first, &view(), &policy()).unwrap_err(),
            refusal("duplicate report")
        );
        let outsider = HuddleEvent::ReportSubmitted {
            huddle_id: id.clone(),
            report: report("agent-z", "v1"),
        };
        assert_eq!(
            apply_huddle(&mut ledger, &outsider, &view(), &policy()).unwrap_err(),
            refusal("reporter was not invited")
        );
        let mut foreign = report("agent-b", "v9");
        foreign.verified_progress = vec![EvidenceId::new("elsewhere").unwrap()];
        assert_eq!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id,
                    report: foreign
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("unknown evidence reference")
        );
    }

    #[test]
    fn budget_exhaustion_forces_synthesis() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::ReportSubmitted {
                huddle_id: id.clone(),
                report: report("agent-a", "v1"),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::BudgetConsumed {
                huddle_id: id.clone(),
                cost: HuddleCost {
                    tokens: 2_000,
                    cost_microunits: 200_000,
                    duration_ms: 5_000,
                    rounds: 2,
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // Past the envelope the fold refuses outright.
        assert_eq!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::BudgetConsumed {
                    huddle_id: id.clone(),
                    cost: HuddleCost {
                        tokens: 1,
                        cost_microunits: 0,
                        duration_ms: 0,
                        rounds: 0
                    }
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            PlaybookError::BudgetExhausted
        );
        // Forced synthesis commits with only one report in.
        let mut synthesized = crate::reconcile::forced_synthesis(ledger.active_by_id(&id).unwrap());
        synthesized.huddle_id = id.clone();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::OutcomeCommitted {
                huddle_id: id.clone(),
                outcome: synthesized,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::ForcedSynthesis);
    }

    #[test]
    fn no_evidence_no_resolution_and_no_history_rewrites() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        for author in ["agent-a", "agent-b"] {
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: report(author, "v1"),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        apply_huddle(
            &mut ledger,
            &HuddleEvent::ContradictionDetected {
                huddle_id: id.clone(),
                contradiction: contradiction(2),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        let mut committed = outcome("h-1");
        committed.resolved_contradictions = vec![crate::domain::ContradictionResolution {
            contradiction_id: ContradictionId::new("con-000").unwrap(),
            state: ResolutionState::ResolvedByEvidence,
            evidence_ref: Some(EvidenceId::new("fabricated").unwrap()),
            authority: None,
        }];
        assert!(matches!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::OutcomeCommitted {
                    huddle_id: id.clone(),
                    outcome: committed.clone()
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            PlaybookError::Evidence(_)
        ));
        committed.resolved_contradictions[0].evidence_ref = Some(EvidenceId::new("ev-1").unwrap());
        let mut rewritten = committed.clone();
        rewritten.resolved_contradictions.clear();
        rewritten.unresolved_contradictions = vec![contradiction(2)];
        rewritten.revised_ytg_milli = Some(9_000);
        assert!(matches!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::OutcomeCommitted {
                    huddle_id: id.clone(),
                    outcome: rewritten
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            PlaybookError::ReplayDivergence(_)
        ));
        // A clean resolution commits and the contradiction is accounted for.
        committed.unresolved_contradictions.clear();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::OutcomeCommitted {
                huddle_id: id,
                outcome: committed,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::Committed);
    }

    #[test]
    fn blocking_contradictions_require_escalation_to_commit() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        for author in ["agent-a", "agent-b"] {
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: report(author, "v1"),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        let mut critical = contradiction(2);
        critical.severity = ContradictionSeverity::Critical;
        apply_huddle(
            &mut ledger,
            &HuddleEvent::ContradictionDetected {
                huddle_id: id.clone(),
                contradiction: critical.clone(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        let mut committed = outcome("h-1");
        committed.unresolved_contradictions = vec![critical.clone()];
        assert_eq!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::OutcomeCommitted {
                    huddle_id: id.clone(),
                    outcome: committed
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("blocking contradiction prevents unsafe progression")
        );
        let mut escalates = outcome("h-1");
        escalates.unresolved_contradictions = vec![critical];
        escalates.escalated_to = Some(HuddleScope::Unit);
        apply_huddle(
            &mut ledger,
            &HuddleEvent::OutcomeCommitted {
                huddle_id: id,
                outcome: escalates,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.closed()[0].closure, HuddleClosure::Partial);
    }

    #[test]
    fn concurrent_huddles_need_disjoint_partitions() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        // The second huddle cites no shared evidence, so its partition is
        // its participants alone.
        let bare_trigger = HuddleTrigger {
            kind: crate::domain::HuddleTriggerKind::CoherenceDebt,
            reason_code: "coherence_debt".into(),
            evidence_refs: Vec::new(),
            originating_event: playbook_core::EventId::new("evt-2").unwrap(),
        };
        let second = HuddleEvent::Requested {
            huddle_id: HuddleId::new("h-2").unwrap(),
            scope: HuddleScope::Unit,
            trigger: bare_trigger,
            at_ms: 2,
            debt: 0,
            rework_avoided: 0,
        };
        apply_huddle(&mut ledger, &second, &view(), &policy()).unwrap();
        let admit = HuddleEvent::Admitted {
            huddle_id: HuddleId::new("h-2").unwrap(),
            budget: budget(),
            barrier: BarrierMode::Scope,
            participants: vec![participant("agent-a")],
        };
        // agent-a is locked by h-1.
        assert_eq!(
            apply_huddle(&mut ledger, &admit, &view(), &policy()).unwrap_err(),
            refusal("state partition is already locked")
        );
        let disjoint = HuddleEvent::Admitted {
            huddle_id: HuddleId::new("h-2").unwrap(),
            budget: budget(),
            barrier: BarrierMode::Scope,
            participants: vec![participant("agent-z")],
        };
        apply_huddle(&mut ledger, &disjoint, &view(), &policy()).unwrap();
        assert_eq!(ledger.active().len(), 2);
    }

    #[test]
    fn escalation_flow_moves_scope_and_requires_justification() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        let unjustified = HuddleEvent::EscalationRequested {
            huddle_id: id.clone(),
            to_scope: HuddleScope::Unit,
            reason_code: "".into(),
        };
        assert_eq!(
            apply_huddle(&mut ledger, &unjustified, &view(), &policy()).unwrap_err(),
            refusal("escalation lacks scope justification")
        );
        apply_huddle(
            &mut ledger,
            &HuddleEvent::ContradictionDetected {
                huddle_id: id.clone(),
                contradiction: contradiction(2),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        apply_huddle(
            &mut ledger,
            &HuddleEvent::EscalationRequested {
                huddle_id: id.clone(),
                to_scope: HuddleScope::Unit,
                reason_code: "unresolved_interface_contradiction".into(),
            },
            &view(),
            &policy(),
        )
        .unwrap();
        // Downward escalation is refused.
        assert!(matches!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::Escalated {
                    huddle_id: id.clone(),
                    new_huddle_id: HuddleId::new("h-2").unwrap(),
                    to_scope: HuddleScope::Position,
                },
                &view(),
                &policy()
            ),
            Err(PlaybookError::Inadmissible(_, _))
        ));
        apply_huddle(
            &mut ledger,
            &HuddleEvent::Escalated {
                huddle_id: id.clone(),
                new_huddle_id: HuddleId::new("h-2").unwrap(),
                to_scope: HuddleScope::Unit,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert!(ledger.active_by_id(&id).is_none());
        let child = ledger.active_by_id(&HuddleId::new("h-2").unwrap()).unwrap();
        assert_eq!(child.scope, HuddleScope::Unit);
        assert_eq!(child.phase, HuddlePhase::Requested);
        assert_eq!(ledger.escalated_total(), 1);
    }

    #[test]
    fn assignments_land_but_gates_and_evidence_do_not_move() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        for author in ["agent-a", "agent-b"] {
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: report(author, "v1"),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        let mut committed = outcome("h-1");
        committed.assignment_changes = vec![crate::domain::AssignmentChange {
            personnel_id: playbook_core::PersonnelId::new("agent-b").unwrap(),
            from: Some("inspect".into()),
            to: "test.unit".into(),
        }];
        committed.personnel_changes = vec![crate::domain::PersonnelChange {
            personnel_id: playbook_core::PersonnelId::new("agent-c").unwrap(),
            action: crate::domain::PersonnelAction::SubstitutionRecommended,
            reason: "weakest reliability on the failing play".into(),
        }];
        committed.next_play = Some(crate::domain::PlayProposal {
            operation: "test.unit".into(),
            rationale: "unit coverage is the open dependency".into(),
            target_gate: Some(playbook_core::GateId::new("unit_tests").unwrap()),
        });
        apply_huddle(
            &mut ledger,
            &HuddleEvent::OutcomeCommitted {
                huddle_id: id,
                outcome: committed,
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(ledger.assignments().len(), 1);
        assert_eq!(ledger.substitutions().len(), 1);
        assert_eq!(ledger.proposals().len(), 1);
    }

    #[test]
    fn an_unknown_play_proposal_is_refused() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        for author in ["agent-a", "agent-b"] {
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: report(author, "v1"),
                },
                &view(),
                &policy(),
            )
            .unwrap();
        }
        let mut committed = outcome("h-1");
        committed.next_play = Some(crate::domain::PlayProposal {
            operation: "deploy.to.production".into(),
            rationale: "unlocked".into(),
            target_gate: None,
        });
        assert!(matches!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::OutcomeCommitted {
                    huddle_id: id,
                    outcome: committed
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            PlaybookError::Inadmissible(_, _)
        ));
    }

    #[test]
    fn a_same_scope_request_while_one_is_active_is_refused_at_fold_time() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let again = HuddleEvent::Requested {
            huddle_id: HuddleId::new("h-2").unwrap(),
            scope: HuddleScope::Position,
            trigger: trigger(),
            at_ms: 3,
            debt: 0,
            rework_avoided: 0,
        };
        assert_eq!(
            apply_huddle(&mut ledger, &again, &view(), &policy()).unwrap_err(),
            refusal("scope_busy")
        );
        // A disjoint scope may ask concurrently; admission owns the
        // unchanged-field storm rule.
        let other_scope = HuddleEvent::Requested {
            huddle_id: HuddleId::new("h-3").unwrap(),
            scope: HuddleScope::Unit,
            trigger: trigger(),
            at_ms: 4,
            debt: 0,
            rework_avoided: 0,
        };
        apply_huddle(&mut ledger, &other_scope, &view(), &policy()).unwrap();
        assert_eq!(ledger.active().len(), 2);
    }

    #[test]
    fn confidence_and_observer_rules_hold() {
        let mut ledger = HuddleLedger::empty();
        open_huddle(&mut ledger, "h-1");
        let id = HuddleId::new("h-1").unwrap();
        let mut overconfident = report("agent-a", "v1");
        overconfident.confidence_milli = u16::MAX;
        assert_eq!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id.clone(),
                    report: overconfident
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("confidence_milli out of range")
        );
        apply_huddle(
            &mut ledger,
            &HuddleEvent::ParticipantInvited {
                huddle_id: id.clone(),
                participant: HuddleParticipant {
                    personnel_id: playbook_core::PersonnelId::new("watcher").unwrap(),
                    role: HuddleRole::Observer,
                    disclosure_profile: DisclosureProfile {
                        include_gates: true,
                        include_peer_reports: true,
                        include_strategic_state: false,
                        max_evidence_refs: 8,
                    },
                },
            },
            &view(),
            &policy(),
        )
        .unwrap();
        assert_eq!(
            apply_huddle(
                &mut ledger,
                &HuddleEvent::ReportSubmitted {
                    huddle_id: id,
                    report: report("watcher", "v1")
                },
                &view(),
                &policy()
            )
            .unwrap_err(),
            refusal("observer cannot report")
        );
    }
}
