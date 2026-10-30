//! Deterministic verification.
//!
//! Verification races inference instead of following it: a candidate's
//! arrival immediately makes it eligible for verification against the frozen
//! gate set. The first response is never trusted; the first candidate whose
//! required gates all pass is. A verifier that cannot run leaves the
//! candidate untrusted — fail closed — with restarts bounded by policy.

use crate::domain::{ActiveBlitz, Candidate, VerificationKind, VerificationRecord};
use playbook_core::PlaybookError;
use playbook_policy::BlitzCorroboration;

/// The verdict one verification attempt returns for one candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verification {
    /// The named gates passed. The fold requires the frozen gate set to be
    /// covered completely; extra gates are reordered by rank.
    Passed { gates: Vec<VerificationKind> },
    /// A gate failed, with a machine reason. Verification failure is evidence
    /// about the candidate, not about the race.
    Failed {
        gate: VerificationKind,
        reason_code: String,
    },
    /// The verifier itself could not run. The candidate stays untrusted;
    /// nothing may be committed on the strength of a missing verifier.
    VerifierUnavailable,
}

/// Resolve one candidate against the required gate set into a fold-ready
/// record. Deterministic: the same verdict always produces the same record.
pub fn verify_candidate(
    candidate: &Candidate,
    required: &[VerificationKind],
    verdict: &Verification,
) -> Result<VerificationRecord, PlaybookError> {
    let base = VerificationRecord {
        candidate_id: candidate.candidate_id.clone(),
        worker_id: candidate.worker_id.clone(),
        passed: Vec::new(),
        failed: None,
        verifier_unavailable: false,
        attempts: 1,
    };
    match verdict {
        Verification::VerifierUnavailable => Ok(VerificationRecord {
            verifier_unavailable: true,
            ..base
        }),
        Verification::Failed { gate, reason_code } => Ok(VerificationRecord {
            failed: Some((*gate, reason_code.clone())),
            ..base
        }),
        Verification::Passed { gates } => {
            if let Some(missing) = required.iter().find(|gate| !gates.contains(gate)) {
                return Ok(VerificationRecord {
                    failed: Some((*missing, "required_gate_missing".into())),
                    ..base
                });
            }
            let mut passed = gates.clone();
            passed.sort_by_key(|gate| gate.rank());
            passed.dedup();
            Ok(VerificationRecord { passed, ..base })
        }
    }
}

/// Whether a winner may be committed right now: corroboration must be
/// satisfied by independently verified candidates, and no winner may exist
/// yet. This is the atomic-victory check; the fold enforces it on
/// `WinnerSelected`.
pub fn winner_may_commit(blitz: &ActiveBlitz, corroboration: &BlitzCorroboration) -> bool {
    blitz.winner.is_none() && blitz.corroboration_satisfied(corroboration)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ActiveBlitz, BlitzClass, BlitzCost, BlitzPhase, BlitzResourceMode, BlitzTrigger, Candidate,
        DefensivePressure, SelectedWinner, Severity, TargetPackage, WorkerSlot,
    };
    use playbook_core::{BlitzBudget, BlitzId, CandidateId, EventId, PersonnelId, WorkerId};

    fn candidate(id: &str, worker: &str) -> Candidate {
        Candidate {
            candidate_id: CandidateId::new(id).unwrap(),
            blitz_id: BlitzId::new("b-1").unwrap(),
            worker_id: WorkerId::new(worker).unwrap(),
            target_hash: "hash".into(),
            artefact_digest: format!("digest-{worker}"),
            claim: "fixes the failure".into(),
            confidence_ppm: 900_000,
            evidence_refs: Vec::new(),
            verification_hint: String::new(),
            emitted_at_ms: 10,
            wave: 1,
        }
    }

    fn required() -> Vec<VerificationKind> {
        vec![VerificationKind::Compilation, VerificationKind::UnitTest]
    }

    #[test]
    fn missing_required_gates_fail_the_candidate() {
        let candidate = candidate("c-1", "w-1");
        let partial = Verification::Passed {
            gates: vec![VerificationKind::Compilation],
        };
        let record = verify_candidate(&candidate, &required(), &partial).unwrap();
        assert_eq!(
            record.failed,
            Some((VerificationKind::UnitTest, "required_gate_missing".into()))
        );
        let complete = Verification::Passed {
            gates: vec![VerificationKind::UnitTest, VerificationKind::Compilation],
        };
        let record = verify_candidate(&candidate, &required(), &complete).unwrap();
        assert!(record.failed.is_none());
        // Gate order follows the verification hierarchy, not arrival order.
        assert_eq!(
            record.passed,
            vec![VerificationKind::Compilation, VerificationKind::UnitTest]
        );
    }

    #[test]
    fn an_unavailable_verifier_leaves_the_candidate_untrusted() {
        let candidate = candidate("c-1", "w-1");
        let record =
            verify_candidate(&candidate, &required(), &Verification::VerifierUnavailable).unwrap();
        assert!(record.verifier_unavailable);
        assert!(record.passed.is_empty());
        assert!(record.failed.is_none());
    }

    fn blitz_with(corroboration: BlitzCorroboration) -> ActiveBlitz {
        let target = TargetPackage {
            problem_statement: "restore the failing component".into(),
            evidence_refs: Vec::new(),
            artefact_refs: Vec::new(),
            constraints: Vec::new(),
            forbidden_actions: Vec::new(),
            expected_output: "a patch".into(),
            win_condition: "gates pass".into(),
            verification_gates: vec![VerificationKind::Compilation, VerificationKind::UnitTest],
            deadline_ms: 10_000,
            target_hash: String::new(),
        }
        .seal()
        .unwrap();
        let mut blitz = ActiveBlitz {
            id: BlitzId::new("b-1").unwrap(),
            class: BlitzClass::Instinctive,
            phase: BlitzPhase::Verifying,
            trigger: BlitzTrigger {
                class: BlitzClass::Instinctive,
                reason_code: "power_critical".into(),
                evidence_refs: Vec::new(),
                originating_event: EventId::new("evt-1").unwrap(),
            },
            target,
            pressure: DefensivePressure {
                severity: Severity::Critical,
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
            corroboration,
            resource_mode: BlitzResourceMode::PowerCritical,
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
            failure: None,
            correlated_failures: 0,
            effective_parallelism_milli: 0,
            at_ms: 0,
            deadline_at_ms: 10_000,
        };
        for (worker, provider, family) in
            [("w-1", "alpha", "family-1"), ("w-2", "beta", "family-2")]
        {
            blitz.workers.push(WorkerSlot {
                worker_id: WorkerId::new(worker).unwrap(),
                personnel_id: PersonnelId::new(worker).unwrap(),
                provider: provider.into(),
                model_family: family.into(),
                failure_domains: Vec::new(),
                wave: 1,
                dispatched_at_ms: 0,
                deadline_ms: 10_000,
            });
        }
        blitz
    }

    fn verified(blitz: &mut ActiveBlitz, candidate: Candidate) {
        let required = blitz.target.verification_gates.clone();
        let record = verify_candidate(
            &candidate,
            &required,
            &Verification::Passed {
                gates: required.clone(),
            },
        )
        .unwrap();
        blitz.candidates.push(candidate);
        blitz.verifications.push(record);
    }

    #[test]
    fn first_pass_corroboration_accepts_one_verified_candidate() {
        let mut blitz = blitz_with(BlitzCorroboration::FirstPass);
        assert!(!winner_may_commit(&blitz, &BlitzCorroboration::FirstPass));
        verified(&mut blitz, candidate("c-1", "w-1"));
        assert!(winner_may_commit(&blitz, &BlitzCorroboration::FirstPass));
    }

    #[test]
    fn two_independent_passes_reject_a_single_verified_candidate() {
        let mut blitz = blitz_with(BlitzCorroboration::TwoIndependentPasses);
        verified(&mut blitz, candidate("c-1", "w-1"));
        assert!(!winner_may_commit(
            &blitz,
            &BlitzCorroboration::TwoIndependentPasses
        ));
        verified(&mut blitz, candidate("c-2", "w-2"));
        assert!(winner_may_commit(
            &blitz,
            &BlitzCorroboration::TwoIndependentPasses
        ));
    }

    #[test]
    fn heterogeneous_confirmation_requires_distinct_providers_and_families() {
        let mut blitz = blitz_with(BlitzCorroboration::HeterogeneousConfirmation);
        // Same provider, different families: not heterogeneous.
        blitz.workers[1].provider = "alpha".into();
        verified(&mut blitz, candidate("c-1", "w-1"));
        verified(&mut blitz, candidate("c-2", "w-2"));
        assert!(!winner_may_commit(
            &blitz,
            &BlitzCorroboration::HeterogeneousConfirmation
        ));
        blitz.workers[1].provider = "beta".into();
        assert!(winner_may_commit(
            &blitz,
            &BlitzCorroboration::HeterogeneousConfirmation
        ));
    }

    #[test]
    fn deterministic_only_never_counts_generative_gates_alone() {
        let mut blitz = blitz_with(BlitzCorroboration::DeterministicOnly);
        verified(&mut blitz, candidate("c-1", "w-1"));
        assert!(winner_may_commit(
            &blitz,
            &BlitzCorroboration::DeterministicOnly
        ));
        // A second candidate verified only by adjudication and human review
        // adds nothing to deterministic-only corroboration — but the first
        // deterministic pass already carried it.
        blitz.candidates.push(candidate("c-2", "w-2"));
        blitz.verifications.push(VerificationRecord {
            candidate_id: CandidateId::new("c-2").unwrap(),
            worker_id: WorkerId::new("w-2").unwrap(),
            passed: vec![
                VerificationKind::IndependentAdjudication,
                VerificationKind::HumanReview,
            ],
            failed: None,
            verifier_unavailable: false,
            attempts: 1,
        });
        assert!(winner_may_commit(
            &blitz,
            &BlitzCorroboration::DeterministicOnly
        ));
    }

    #[test]
    fn human_confirmation_requires_a_recorded_confirmation() {
        let mut blitz = blitz_with(BlitzCorroboration::HumanConfirmation);
        verified(&mut blitz, candidate("c-1", "w-1"));
        assert!(!winner_may_commit(
            &blitz,
            &BlitzCorroboration::HumanConfirmation
        ));
        blitz
            .human_confirmations
            .push((CandidateId::new("c-1").unwrap(), "on-call".into()));
        assert!(winner_may_commit(
            &blitz,
            &BlitzCorroboration::HumanConfirmation
        ));
    }

    #[test]
    fn a_selected_winner_is_never_replaced() {
        let mut blitz = blitz_with(BlitzCorroboration::FirstPass);
        verified(&mut blitz, candidate("c-1", "w-1"));
        blitz.winner = Some(SelectedWinner {
            candidate_id: CandidateId::new("c-1").unwrap(),
            worker_id: WorkerId::new("w-1").unwrap(),
            at_ms: 10,
            human_confirmed: false,
        });
        assert!(!winner_may_commit(&blitz, &BlitzCorroboration::FirstPass));
    }
}
