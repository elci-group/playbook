//! Escalation seams.
//!
//! Independence has failed; coherence must now be purchased. Wave 1 never
//! huddles — only an exhausted blitz may hand its failure evidence to the
//! Huddle subsystem. Lastdown may escalate a sequential substitution drive
//! into a blitz; Manmark may request concentrated reinforcement for a
//! persistent threat. Neither seam can authorise expenditure: the trigger is
//! a proposal, and deterministic policy still decides.

use crate::domain::{
    refusal, ActiveBlitz, BlitzClass, BlitzFailure, BlitzTrigger, HuddleRecommendation, Severity,
};
use playbook_core::{EventId, EvidenceId, PlaybookError};
use playbook_policy::BlitzPolicy;

/// A persistent-domain defender requesting concentrated reinforcement.
/// Manmark remains responsible for the threat; the blitz is temporary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManmarkRequest {
    pub threat: String,
    pub severity: Severity,
    pub evidence_refs: Vec<EvidenceId>,
    pub originating_event: EventId,
}

impl ManmarkRequest {
    /// The blitz trigger this request proposes. It is a strategic class:
    /// Manmark proposes urgency, policy authorises the expenditure.
    pub fn into_trigger(self) -> Result<BlitzTrigger, PlaybookError> {
        if self.threat.trim().is_empty() {
            return Err(refusal("manmark threat requires a subject"));
        }
        Ok(BlitzTrigger {
            class: BlitzClass::Strategic,
            reason_code: format!("manmark_escalation_{}", self.threat),
            evidence_refs: self.evidence_refs,
            originating_event: self.originating_event,
        })
    }

    /// The defensive pressure a tracked threat implies. Deterioration and
    /// blast radius come from the manmark tracking history, not from the
    /// requesting agent's confidence.
    pub fn pressure(&self, severity: Severity) -> crate::domain::DefensivePressure {
        crate::domain::DefensivePressure {
            severity,
            deadline_ms: None,
            deterioration_rate_ppm: 0,
            blast_radius_ppm: severity.milli() * 1_000,
            solver_uncertainty_ppm: 500,
            containment_confidence_ppm: 200,
            recovery_confidence_ppm: 500,
            resource_decay_ppm: 0,
            expected_delay_loss: u64::from(severity.milli()),
        }
    }
}

/// Whether Lastdown should escalate from sequential substitution into a
/// blitz: remaining time inside the policy horizon while both solver
/// uncertainty and failure cost sit above the urgency floor. Ordinary
/// continuation asks which substitute preserves the drive; this asks whether
/// remaining time is now worth more than inference efficiency.
pub fn should_escalate_from_lastdown(
    remaining_time_ms: u64,
    solver_uncertainty_ppm: u32,
    failure_cost_ppm: u32,
    policy: &BlitzPolicy,
) -> bool {
    let inside_horizon = remaining_time_ms < policy.deadline_horizon_ms;
    let uncertainty_milli = solver_uncertainty_ppm.min(1_000_000) / 1_000;
    let failure_cost_milli = failure_cost_ppm.min(1_000_000) / 1_000;
    inside_horizon
        && uncertainty_milli >= policy.min_urgency_score
        && failure_cost_milli >= policy.min_urgency_score
}

/// The wave-3 handoff: a failure-aggregated recommendation for the Huddle
/// subsystem. Scope follows severity — critical failures are already
/// team-level emergencies; everything else starts where the divergence is.
pub fn huddle_target(blitz: &ActiveBlitz) -> Result<HuddleRecommendation, PlaybookError> {
    if blitz.phase != crate::domain::BlitzPhase::Exhausted {
        return Err(refusal("only an exhausted blitz escalates"));
    }
    if blitz.winner.is_some() {
        return Err(refusal("a recovered blitz does not escalate"));
    }
    let scope_key = match blitz.pressure.severity {
        Severity::Critical => "team",
        Severity::High => "coordinator",
        _ => "unit",
    };
    Ok(HuddleRecommendation {
        scope_key: scope_key.to_string(),
        reason_code: "post_blitz_independence_failed".into(),
        evidence_refs: blitz.trigger.evidence_refs.clone(),
        failure: blitz.failure,
        correlated_failures: blitz.correlated_failures,
    })
}

/// The failure class an exhausted race implies, for the wave-closure
/// diagnostics the engine records. No candidates at all, every candidate
/// invalid, and a verifier that could not run are different problems
/// requiring different remediation.
pub fn wave_failure(blitz: &ActiveBlitz) -> Option<BlitzFailure> {
    if blitz.workers.is_empty() {
        return Some(BlitzFailure::NoCandidate);
    }
    let emitted = blitz.candidates.len();
    if emitted == 0 {
        return Some(BlitzFailure::NoCandidate);
    }
    let mut rejected = 0;
    let mut unavailable = 0;
    for candidate in &blitz.candidates {
        let record = blitz.verification(&candidate.candidate_id)?;
        if record.verifier_unavailable {
            unavailable += 1;
            rejected += 1;
        } else if record.failed.is_some() {
            rejected += 1;
        }
    }
    if rejected == emitted {
        if unavailable == emitted {
            return Some(BlitzFailure::VerificationFailure);
        }
        return Some(BlitzFailure::InvalidCandidates);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ActiveBlitz, BlitzClass, BlitzCost, BlitzPhase, BlitzResourceMode, BlitzTrigger,
        DefensivePressure, TargetPackage, VerificationKind,
    };
    use playbook_core::{BlitzBudget, BlitzId, EventId, PPM_SCALE};

    fn exhausted(severity: Severity, correlated: u32) -> ActiveBlitz {
        let target = TargetPackage {
            problem_statement: "restore the failing component".into(),
            evidence_refs: Vec::new(),
            artefact_refs: Vec::new(),
            constraints: Vec::new(),
            forbidden_actions: Vec::new(),
            expected_output: "a patch".into(),
            win_condition: "gates pass".into(),
            verification_gates: vec![VerificationKind::Compilation],
            deadline_ms: 10_000,
            target_hash: String::new(),
        }
        .seal()
        .unwrap();
        ActiveBlitz {
            id: BlitzId::new("b-1").unwrap(),
            class: BlitzClass::Instinctive,
            phase: BlitzPhase::Exhausted,
            trigger: BlitzTrigger {
                class: BlitzClass::Instinctive,
                reason_code: "power_critical".into(),
                evidence_refs: vec![EvidenceId::new("ev-1").unwrap()],
                originating_event: EventId::new("evt-1").unwrap(),
            },
            target,
            pressure: DefensivePressure {
                severity,
                deadline_ms: Some(10_000),
                deterioration_rate_ppm: 0,
                blast_radius_ppm: 0,
                solver_uncertainty_ppm: 0,
                containment_confidence_ppm: 0,
                recovery_confidence_ppm: 0,
                resource_decay_ppm: 0,
                expected_delay_loss: 0,
            },
            budget: BlitzBudget::zero(),
            spent: BlitzCost::zero(),
            corroboration: playbook_policy::BlitzCorroboration::FirstPass,
            resource_mode: BlitzResourceMode::Normal,
            workers: Vec::new(),
            closed_waves: Vec::new(),
            exited: Vec::new(),
            candidates: Vec::new(),
            verifications: Vec::new(),
            human_confirmations: Vec::new(),
            winner: None,
            worker_spend: Vec::new(),
            cancellations: Vec::new(),
            cancel_started_at: None,
            cancel_complete: None,
            failure: Some(BlitzFailure::InvalidCandidates),
            correlated_failures: correlated,
            effective_parallelism_milli: 0,
            at_ms: 0,
            deadline_at_ms: 10_000,
        }
    }

    #[test]
    fn manmark_requests_are_strategic_proposals_with_pressure() {
        let request = ManmarkRequest {
            threat: "supply_chain_drift".into(),
            severity: Severity::High,
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-2").unwrap(),
        };
        let trigger = request.clone().into_trigger().unwrap();
        assert_eq!(trigger.class, BlitzClass::Strategic);
        assert!(trigger.reason_code.contains("manmark"));
        let pressure = request.pressure(Severity::High);
        assert_eq!(pressure.severity, Severity::High);
        assert!(pressure.blast_radius_ppm > 0);
        assert!(ManmarkRequest {
            threat: "  ".into(),
            ..request
        }
        .into_trigger()
        .is_err());
    }

    #[test]
    fn lastdown_escalates_only_under_joint_pressure() {
        let policy = playbook_policy::Policy::builtin().blitz;
        // Plenty of time: sequential substitution remains rational.
        assert!(!should_escalate_from_lastdown(
            500_000, 1_000_000, 1_000_000, &policy
        ));
        // Inside the horizon but only one dimension is elevated: not yet.
        assert!(!should_escalate_from_lastdown(
            60_000, 900_000, 100_000, &policy
        ));
        // Time short, uncertainty high, failure cost high: escalate.
        assert!(should_escalate_from_lastdown(
            60_000, 900_000, 900_000, &policy
        ));
    }

    #[test]
    fn huddle_scope_follows_severity_and_requires_exhaustion() {
        let policy = playbook_policy::Policy::builtin().blitz;
        let critical = huddle_target(&exhausted(Severity::Critical, 0)).unwrap();
        assert_eq!(critical.scope_key, "team");
        let high = huddle_target(&exhausted(Severity::High, 2)).unwrap();
        assert_eq!(high.scope_key, "coordinator");
        assert_eq!(high.correlated_failures, 2);
        assert_eq!(high.failure, Some(BlitzFailure::InvalidCandidates));
        assert_eq!(critical.reason_code, "post_blitz_independence_failed");
        let _ = policy;

        let mut racing = exhausted(Severity::High, 0);
        racing.phase = BlitzPhase::Verifying;
        assert_eq!(
            huddle_target(&racing).unwrap_err(),
            refusal("only an exhausted blitz escalates")
        );
    }

    #[test]
    fn wave_failures_are_classified() {
        use crate::domain::{Candidate, VerificationRecord};
        use playbook_core::{CandidateId, WorkerId};
        let mut blitz = exhausted(Severity::High, 0);
        blitz.phase = BlitzPhase::Racing;
        assert_eq!(wave_failure(&blitz), Some(BlitzFailure::NoCandidate));
        blitz.workers.push(crate::domain::WorkerSlot {
            worker_id: WorkerId::new("w-1").unwrap(),
            personnel_id: playbook_core::PersonnelId::new("w-1").unwrap(),
            provider: "alpha".into(),
            model_family: "family-1".into(),
            failure_domains: Vec::new(),
            wave: 1,
            dispatched_at_ms: 0,
            deadline_ms: 1_000,
        });
        blitz.candidates.push(Candidate {
            candidate_id: CandidateId::new("c-1").unwrap(),
            blitz_id: blitz.id.clone(),
            worker_id: WorkerId::new("w-1").unwrap(),
            target_hash: blitz.target.target_hash.clone(),
            artefact_digest: "d".into(),
            claim: "fix".into(),
            confidence_ppm: PPM_SCALE,
            evidence_refs: Vec::new(),
            verification_hint: String::new(),
            emitted_at_ms: 1,
            wave: 1,
        });
        // Pending verification: no failure class yet.
        assert_eq!(wave_failure(&blitz), None);
        blitz.verifications.push(VerificationRecord {
            candidate_id: CandidateId::new("c-1").unwrap(),
            worker_id: WorkerId::new("w-1").unwrap(),
            passed: Vec::new(),
            failed: Some((VerificationKind::Compilation, "compile_failure".into())),
            verifier_unavailable: false,
            attempts: 1,
        });
        assert_eq!(wave_failure(&blitz), Some(BlitzFailure::InvalidCandidates));
    }
}
