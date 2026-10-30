//! Harvesters: deterministic extraction rules from measured failure
//! records into recovery artefacts.
//!
//! Every rule is a doctrine decision written once, here. A harvest never
//! interprets, never summarises, and never invents: diagnostics become
//! facts because they were measured, output references become partial
//! patches because they were produced, and scouting rows become
//! assumptions because the tokens were spent. Sealing inside a harvest
//! cannot fail — harvested content is plain strings and the content
//! address is hex — so a harvest is total and repeatable.

use playbook_blitz::{BlitzFailure, BlitzRecord};
use playbook_core::{FailureClass, PlayResult, ToolFailure};

use crate::artefact::{RecoveryArtefact, RecoverySource};

/// Harvest one materially failed play invocation. Extraction rules,
/// deterministic:
///
/// * `failure.diagnostics` become confirmed facts — they are measured
///   observations, not narrative;
/// * a present result with output references contributes its references as
///   partial patches: work that survived the failure;
/// * a result that never arrived after invocation is recorded as the
///   failed assumption `outcome lost after invocation`;
/// * `failure.kind` maps through
///   [`playbook_core::ToolFailureKind::failure_class`] to the artefact's
///   failure class;
/// * a present idempotency key contributes the reproduction entry
///   `reconcile idempotency key {key} before retry`.
///
/// The artefact is returned sealed; the same failure always harvests to
/// the same bytes and the same id.
pub fn harvest_play(failure: &ToolFailure, result: Option<&PlayResult>) -> RecoveryArtefact {
    let confirmed_facts = failure.diagnostics.clone();
    let mut failed_assumptions = Vec::new();
    let mut partial_patches = Vec::new();
    let mut reproduction = Vec::new();
    match result {
        Some(result) => {
            if !result.output_refs.is_empty() {
                partial_patches = result.output_refs.clone();
            }
        }
        None => failed_assumptions.push("outcome lost after invocation".into()),
    }
    if let Some(key) = &failure.idempotency_key {
        reproduction.push(format!("reconcile idempotency key {key} before retry"));
    }
    let artefact = RecoveryArtefact {
        id: RecoveryArtefact::unsealed_id(),
        source: RecoverySource::FailedPlay {
            play_id: failure.play_id.to_string(),
            operation: failure.operation.clone(),
            personnel_id: failure.personnel_id.to_string(),
        },
        failure_class: Some(failure.kind.failure_class()),
        confirmed_facts,
        failed_assumptions,
        partial_patches,
        reproduction,
        rejected_hypotheses: Vec::new(),
        useful_commands: Vec::new(),
        context_digest: String::new(),
        at_ms: failure.at_ms,
    };
    artefact
        .seal()
        .expect("a harvested play artefact seals cleanly")
}

/// Harvest one closed blitz. Extraction rules, deterministic:
///
/// * scouting rows that verified name their provider and model family as
///   confirmed facts — what worked; rows that produced a candidate without
///   verification name personnel, provider, and family as failed
///   assumptions, because who hunted this ground in vain is exactly the
///   reusable scouting the next formation needs;
/// * correlated failures rule out the shared-fix hypothesis as a rejected
///   hypothesis: `correlated invalid fix produced by N workers`;
/// * a cancellation summary is a confirmed fact that recovery landed;
/// * the recorded blitz failure sets the artefact's failure class through
///   the documented mapping (see `blitz_failure_class`);
/// * a record closed recovered with no failure is **not** a failure: the
///   artefact is still returned, with `failure_class` `None`, because a
///   win is recovery knowledge too — what worked is what the next drive
///   wants to reuse.
///
/// A record without an outcome contributes one confirmed fact that no
/// outcome was recorded and nothing else. The artefact is stamped with the
/// closure time and returned sealed.
pub fn harvest_blitz(record: &BlitzRecord) -> RecoveryArtefact {
    let mut confirmed_facts = Vec::new();
    let mut failed_assumptions = Vec::new();
    let mut rejected_hypotheses = Vec::new();
    let mut failure_class = None;
    let mut source_failure = None;
    if let Some(outcome) = &record.outcome {
        for worker in &outcome.worker_results {
            if worker.candidate_verified {
                confirmed_facts.push(format!(
                    "personnel {} on {}/{} produced a verified fix",
                    worker.personnel_id, worker.provider, worker.model_family
                ));
            } else if worker.produced_candidate {
                failed_assumptions.push(format!(
                    "personnel {} on {}/{} hunted this ground and produced no verified fix",
                    worker.personnel_id, worker.provider, worker.model_family
                ));
            }
        }
        if outcome.correlated_failures > 0 {
            rejected_hypotheses.push(format!(
                "correlated invalid fix produced by {} workers",
                outcome.correlated_failures
            ));
        }
        if outcome.cancellation.is_some() {
            confirmed_facts.push("recovery landed; redundant workers were cancelled".into());
        }
        if let Some(failure) = outcome.failure {
            failure_class = Some(blitz_failure_class(failure));
            source_failure = Some(failure);
        }
    } else {
        confirmed_facts.push("blitz closed without an outcome record".into());
    }
    let artefact = RecoveryArtefact {
        id: RecoveryArtefact::unsealed_id(),
        source: RecoverySource::FailedBlitz {
            blitz_id: record.id.to_string(),
            failure: source_failure,
        },
        failure_class,
        confirmed_facts,
        failed_assumptions,
        partial_patches: Vec::new(),
        reproduction: Vec::new(),
        rejected_hypotheses,
        useful_commands: Vec::new(),
        context_digest: String::new(),
        at_ms: record.closed_at_ms,
    };
    artefact
        .seal()
        .expect("a harvested blitz artefact seals cleanly")
}

/// Harvest one provider outage: the recorded outage is the measurement.
/// The artefact names the provider as a confirmed fact, records the
/// availability assumption the outage disproved, and pins reproduction to
/// an independent health check — never to a blind retry. The failure class
/// is always [`FailureClass::EnvironmentFailure`].
pub fn harvest_outage(provider: &str, at_ms: u64) -> RecoveryArtefact {
    let artefact = RecoveryArtefact {
        id: RecoveryArtefact::unsealed_id(),
        source: RecoverySource::ProviderOutage {
            provider: provider.to_string(),
        },
        failure_class: Some(FailureClass::EnvironmentFailure),
        confirmed_facts: vec![format!("outage recorded for provider {provider}")],
        failed_assumptions: vec![format!("provider {provider} was assumed available")],
        partial_patches: Vec::new(),
        reproduction: vec![format!("health-check provider {provider} before any retry")],
        rejected_hypotheses: Vec::new(),
        useful_commands: Vec::new(),
        context_digest: String::new(),
        at_ms,
    };
    artefact
        .seal()
        .expect("a harvested outage artefact seals cleanly")
}

/// The deterministic map from the blitz failure taxonomy to the play-level
/// failure classes. Defensive failures are never collapsed silently: each
/// mapping below is a doctrine decision, recorded here once.
///
/// * tool and cancellation-machinery defects are tool failures;
/// * no candidate and an ambiguous target mean the capability did not
///   exist on this roster;
/// * invalid or unverified candidates, and policy refusals, are validation
///   failures;
/// * deadline, budget, provider, and correlated failures are environment
///   failures — the field failed, not the play;
/// * a drifting target and a regression are task regressions.
fn blitz_failure_class(failure: BlitzFailure) -> FailureClass {
    match failure {
        BlitzFailure::ToolFailure | BlitzFailure::CancellationFailure => FailureClass::ToolFailure,
        BlitzFailure::NoCandidate | BlitzFailure::TargetAmbiguity => {
            FailureClass::CapabilityAbsence
        }
        BlitzFailure::InvalidCandidates
        | BlitzFailure::VerificationFailure
        | BlitzFailure::PolicyRejection => FailureClass::ValidationFailure,
        BlitzFailure::DeadlineExhausted
        | BlitzFailure::BudgetExhausted
        | BlitzFailure::ProviderFailure
        | BlitzFailure::CorrelatedFailure => FailureClass::EnvironmentFailure,
        BlitzFailure::TargetDrift | BlitzFailure::RecoveryRegression => {
            FailureClass::TaskRegression
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_blitz::{
        BlitzClass, BlitzClosure, BlitzCost, BlitzOutcome, BlitzResourceMode, CancelSummary,
        Severity, WorkerScouting,
    };
    use playbook_core::{
        BlitzId, CapabilityId, DriveBudget, PersonnelId, PlayId, PlayKind, PlayStatus,
        ToolFailureKind,
    };

    fn play_failure(kind: ToolFailureKind, idempotency_key: Option<String>) -> ToolFailure {
        ToolFailure {
            play_id: PlayId::new("play-7").unwrap(),
            play_kind: PlayKind::TargetedInference,
            operation: "inference.complete".into(),
            capability_id: CapabilityId::new("cap-inference").unwrap(),
            personnel_id: PersonnelId::new("p-model-a").unwrap(),
            kind,
            diagnostics: vec![
                "provider returned 503 after 3 attempts".into(),
                "tokens consumed: 412".into(),
            ],
            cost: DriveBudget::zero(),
            at_ms: 1_000,
            idempotency_key,
            side_effects: Vec::new(),
        }
    }

    fn play_result() -> PlayResult {
        PlayResult {
            status: PlayStatus::Failed,
            play_id: PlayId::new("play-7").unwrap(),
            operation: "inference.complete".into(),
            output_refs: vec!["draft/patch-1.diff".into(), "draft/patch-2.diff".into()],
            evidence: Vec::new(),
            artefact_refs: Vec::new(),
            cost: DriveBudget::zero(),
            duration_ms: 50,
            diagnostics: Vec::new(),
            side_effects: Vec::new(),
            at_ms: 1_050,
        }
    }

    fn blitz_record(failure: Option<BlitzFailure>) -> BlitzRecord {
        let worker =
            |personnel: &str, provider: &str, family: &str, produced: bool, verified: bool| {
                WorkerScouting {
                    personnel_id: PersonnelId::new(personnel).unwrap(),
                    provider: provider.into(),
                    model_family: family.into(),
                    wave: 1,
                    produced_candidate: produced,
                    candidate_verified: verified,
                    tokens: 400,
                    cost_microunits: 40,
                    latency_ms: 900,
                }
            };
        BlitzRecord {
            id: BlitzId::new("blitz-3").unwrap(),
            class: BlitzClass::Instinctive,
            closure: if failure.is_none() {
                BlitzClosure::Recovered
            } else {
                BlitzClosure::Exhausted
            },
            reason_code: "scouting".into(),
            target_hash: "target".into(),
            resource_mode: BlitzResourceMode::Normal,
            severity: Severity::High,
            outcome: Some(BlitzOutcome {
                target_hash: "target".into(),
                winner: None,
                first_candidate_ms: Some(400),
                time_to_verified_ms: Some(900),
                time_to_recovery_ms: Some(902),
                raw_parallelism: 3,
                effective_parallelism_milli: 2_400,
                correlated_failures: 2,
                tokens_to_recovery: 1_100,
                cost_to_recovery_microunits: 110,
                cancellation: Some(CancelSummary {
                    cancelled: 1,
                    latency_ms: 2,
                    avoided_tokens: 500,
                    avoided_cost_microunits: 50,
                }),
                worker_results: vec![
                    worker("p-model-a", "provider-a", "family-a", true, true),
                    worker("p-model-b", "provider-b", "family-b", true, false),
                    worker("p-tool-c", "provider-c", "family-c", false, false),
                ],
                failure,
            }),
            spent: BlitzCost::zero(),
            at_ms: 100,
            closed_at_ms: 1_000,
        }
    }

    #[test]
    fn play_harvest_follows_the_extraction_rules() {
        let failure = play_failure(ToolFailureKind::RateLimit, Some("idem-1".into()));
        let result = play_result();
        let artefact = harvest_play(&failure, Some(&result));
        assert_eq!(artefact.confirmed_facts, failure.diagnostics);
        assert_eq!(artefact.partial_patches, result.output_refs);
        assert_eq!(
            artefact.reproduction,
            vec!["reconcile idempotency key idem-1 before retry".to_string()]
        );
        assert_eq!(artefact.failure_class, Some(FailureClass::ToolFailure));
        assert!(artefact.failed_assumptions.is_empty());
        assert_eq!(
            artefact.source,
            RecoverySource::FailedPlay {
                play_id: "play-7".into(),
                operation: "inference.complete".into(),
                personnel_id: "p-model-a".into(),
            }
        );
        assert_eq!(artefact.at_ms, 1_000);
        // The harvested artefact is sealed and intact.
        assert!(artefact.verify().is_ok());
    }

    #[test]
    fn play_harvest_records_a_lost_outcome() {
        let failure = play_failure(ToolFailureKind::Timeout, None);
        let artefact = harvest_play(&failure, None);
        assert_eq!(
            artefact.failed_assumptions,
            vec!["outcome lost after invocation".to_string()]
        );
        assert!(artefact.partial_patches.is_empty());
        assert!(artefact.reproduction.is_empty());
    }

    #[test]
    fn blitz_harvest_extracts_scouting_and_recovery() {
        // A recovered blitz is not a failure; it is still recovery knowledge.
        let harvested = harvest_blitz(&blitz_record(None));
        assert!(harvested.failure_class.is_none());
        assert_eq!(
            harvested.source,
            RecoverySource::FailedBlitz {
                blitz_id: "blitz-3".into(),
                failure: None,
            }
        );
        assert_eq!(
            harvested.confirmed_facts,
            vec![
                "personnel p-model-a on provider-a/family-a produced a verified fix".to_string(),
                "recovery landed; redundant workers were cancelled".to_string(),
            ]
        );
        assert_eq!(
            harvested.failed_assumptions,
            vec!["personnel p-model-b on provider-b/family-b hunted this ground and produced no verified fix".to_string()]
        );
        assert_eq!(
            harvested.rejected_hypotheses,
            vec!["correlated invalid fix produced by 2 workers".to_string()]
        );
        assert_eq!(harvested.at_ms, 1_000);
        assert!(harvested.verify().is_ok());
    }

    #[test]
    fn blitz_failures_map_to_failure_classes() {
        let cases = [
            (
                BlitzFailure::ProviderFailure,
                FailureClass::EnvironmentFailure,
            ),
            (
                BlitzFailure::VerificationFailure,
                FailureClass::ValidationFailure,
            ),
            (BlitzFailure::NoCandidate, FailureClass::CapabilityAbsence),
            (BlitzFailure::TargetDrift, FailureClass::TaskRegression),
            (BlitzFailure::CancellationFailure, FailureClass::ToolFailure),
            (
                BlitzFailure::PolicyRejection,
                FailureClass::ValidationFailure,
            ),
        ];
        for (blitz_failure, class) in cases {
            let record = blitz_record(Some(blitz_failure));
            assert_eq!(harvest_blitz(&record).failure_class, Some(class));
        }
        let mut without_outcome = blitz_record(None);
        without_outcome.outcome = None;
        let harvested = harvest_blitz(&without_outcome);
        assert!(harvested.failure_class.is_none());
        assert_eq!(
            harvested.confirmed_facts,
            vec!["blitz closed without an outcome record".to_string()]
        );
    }

    #[test]
    fn harvest_is_deterministic() {
        let failure = play_failure(ToolFailureKind::Timeout, Some("idem-2".into()));
        let result = play_result();
        assert_eq!(
            harvest_play(&failure, Some(&result)),
            harvest_play(&failure, Some(&result))
        );
        let record = blitz_record(Some(BlitzFailure::CorrelatedFailure));
        assert_eq!(harvest_blitz(&record), harvest_blitz(&record));
        assert_eq!(
            harvest_outage("provider-a", 7),
            harvest_outage("provider-a", 7)
        );
        // Different inputs name different artefacts.
        assert_ne!(
            harvest_outage("provider-a", 7),
            harvest_outage("provider-a", 8)
        );
    }
}
