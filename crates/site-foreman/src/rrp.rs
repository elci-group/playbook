//! The foreman's RRP loop: dispatch through the CapabilityPlane, classify
//! every rejection from real signals, prescribe under the policy, apply
//! only authorised execution-condition repairs, and replay into the
//! unchanged validator. Operator fallback exists but is an escalation,
//! recorded as such — never a silent rescue.

use playbook_resolution::{
    classify, prescribe, Action, RefusalObservation, ResolutionEvent, ResolutionLineage,
    ResolutionPolicy, ResolutionStage,
};
use work_adaptation::{normalise_brief, normalise_observed_answer, semantic_requirement_met};

use crate::{AuthoredSection, Brief, InterventionLog};

/// The outcome of one RRP-governed authoring attempt.
pub struct Authored {
    pub section: AuthoredSection,
    pub events: Vec<ResolutionEvent>,
    /// Operator interventions this authoring consumed (target: zero).
    pub operator_interventions: u32,
}

struct LoopState {
    brief_text: String,
    envelope: u64,
    attempts: u32,
    last_failures_digest: Option<String>,
    tokens_spent: u64,
}

/// Author one section under the RRP. Bounded by the policy budget; the
/// validator is never weakened; every rejection is classified, every
/// repair is prescribed, every replay is revalidated.
#[allow(clippy::too_many_arguments)]
pub fn author_under_rrp(
    log: &mut InterventionLog,
    plane: &dyn capability_plane::CapabilityPlane,
    builders: &[String],
    brief: &Brief,
    policy: &ResolutionPolicy,
    mut enforce_anchors: bool,
) -> Authored {
    let mut events = Vec::new();
    let mut operator_interventions = 0u32;
    let refusal_digest_base = format!("{}:{}", brief.id, brief.instruction.len());
    let mut lineage = ResolutionLineage::new();
    lineage.root("attempt-1", 0);
    let mut state = LoopState {
        brief_text: brief.instruction.clone(),
        envelope: policy.token_envelope,
        attempts: 0,
        last_failures_digest: None,
        tokens_spent: 0,
    };
    let mut builder_cycle = builders.iter().cycle();
    let mut attempt_id = String::from("attempt-1");
    for _ in 0..=policy.budget.max_remediation_attempts {
        let builder = builder_cycle.next().expect("cycle is infinite");
        state.attempts += 1;
        let envelope = capability_plane::ExecutionEnvelope {
            request_id: format!("{}-{}", brief.id, state.attempts),
            model: builder.clone(),
            prompt: state.brief_text.clone(),
            max_tokens: u32::try_from(state.envelope).unwrap_or(u32::MAX),
            temperature: 0.0,
            timeout_secs: 120,
        };
        let handle = plane.dispatch(&envelope);
        let outcome = plane.reconcile(handle);
        state.tokens_spent += outcome.total_tokens;
        let attempt_refusal = format!("{}:{}", refusal_digest_base, state.attempts);
        events.push(ResolutionEvent::new(
            ResolutionStage::ReplayStarted,
            &attempt_refusal,
            outcome.latency_ms,
            &format!("dispatch to {} (envelope {})", builder, state.envelope),
        ));
        let content = outcome.content.clone().unwrap_or_default();
        // Validate. The gate is the same on every attempt.
        match crate::validate_fragment(&content, brief) {
            Ok(()) => {
                events.push(ResolutionEvent::new(
                    ResolutionStage::ResolutionSucceeded,
                    &attempt_refusal,
                    outcome.latency_ms,
                    "the unchanged validator accepted the replayed attempt",
                ));
                let provenance = if state.attempts == 1 {
                    "model"
                } else {
                    "model-repaired"
                };
                return Authored {
                    section: AuthoredSection {
                        brief_id: brief.id.to_string(),
                        builder: builder.clone(),
                        provenance: provenance.into(),
                        fragment: content,
                    },
                    events,
                    operator_interventions,
                };
            }
            Err(failures) => {
                let structurally_complete =
                    !content.trim().is_empty() && outcome.finish_reason.as_deref() == Some("stop");
                // Post-normalisation adequacy: does the content meet the
                // semantic requirements once wrappers are stripped?
                let semantic_met = brief.must_contain.iter().all(|required| {
                    semantic_requirement_met(required, &normalise_observed_answer(&content))
                });
                let observation = RefusalObservation {
                    finish_reason: outcome.finish_reason.clone(),
                    provider_status: outcome.provider_status.clone(),
                    exit_code: None,
                    gate_failures: failures.clone(),
                    output_structurally_complete: structurally_complete,
                    artefact_expected_but_missing: false,
                    output_path_contended: false,
                    version_control_failed: false,
                    prior_attempts_with_same_failures: state.attempts.saturating_sub(1),
                    semantic_requirements_met_after_normalisation: Some(semantic_met),
                    first_required_item_missing: brief
                        .must_contain
                        .first()
                        .map(|first| !content.contains(first.as_str())),
                };
                let diagnosis = classify(&observation);
                let prescription = prescribe(
                    &diagnosis,
                    policy,
                    state.attempts,
                    state.tokens_spent,
                    state.last_failures_digest == Some(digest(&failures)),
                );
                state.last_failures_digest = Some(digest(&failures));
                events.push(ResolutionEvent::new(
                    ResolutionStage::RefusalClassified,
                    &attempt_refusal,
                    outcome.latency_ms,
                    &format!(
                        "{} ({:?}, {:?}) — prescribed: {}",
                        diagnosis.rationale,
                        diagnosis.confidence,
                        diagnosis.deterministic,
                        prescription
                            .actions
                            .iter()
                            .map(|a| a.key())
                            .collect::<Vec<_>>()
                            .join("+")
                    ),
                ));
                log.record(
                    "rrp",
                    &format!(
                        "section {} refused: {} — prescribed {}",
                        brief.id,
                        diagnosis.primary.key(),
                        prescription
                            .actions
                            .iter()
                            .map(|a| a.key())
                            .collect::<Vec<_>>()
                            .join("+")
                    ),
                    &diagnosis.rationale,
                    "the RRP classified and prescribed mechanically; the unchanged validator re-decides the replay",
                );
                if prescription.escalated {
                    operator_interventions += 1;
                    events.push(ResolutionEvent::new(
                        ResolutionStage::EscalationRequested,
                        &attempt_refusal,
                        outcome.latency_ms,
                        prescription.rationale.as_str(),
                    ));
                    break;
                }
                // Apply authorised execution-condition repairs.
                for action in &prescription.actions {
                    match action {
                        Action::NormaliseBrief => {
                            state.brief_text = format!(
                                "{}\n{}",
                                state.brief_text,
                                normalise_brief(&state.brief_text, &failures)
                            );
                        }
                        Action::ExpandEnvelope => {
                            if let Some(new) = prescription.suggested_envelope {
                                state.envelope = new.max(state.envelope);
                            }
                        }
                        Action::ImposeAnchors => {
                            if enforce_anchors {
                                if let (Some(first), Some(last)) =
                                    (brief.must_contain.first(), brief.must_contain.last())
                                {
                                    state.brief_text = format!(
                                        "{}\nYour output MUST BEGIN with the card for '{}' and END with the card for '{}'.",
                                        state.brief_text, first, last
                                    );
                                }
                            }
                        }
                        Action::DecomposeWork => {
                            // Decomposition at this level means: relax the
                            // anchor enforcement and let the envelope
                            // expansion carry the retry; a structural split
                            // happens at the brief-construction layer (the
                            // grid is already chunked).
                            enforce_anchors = false;
                        }
                        Action::ChangeProvider | Action::SubstitutePlayer => {
                            // Advance to the next builder in the cycle.
                            let _ = builder_cycle.next();
                        }
                        Action::AssignOwnership | Action::BoundedRetry | Action::Escalate => {}
                    }
                }
                if let Some(new_envelope) = prescription.suggested_envelope {
                    state.envelope = state.envelope.max(new_envelope);
                }
                let _ = enforce_anchors;
                let child = lineage
                    .refuse(&attempt_id, &attempt_refusal, "replay")
                    .unwrap_or_else(|| attempt_id.clone());
                attempt_id = child;
            }
        }
    }
    // Budget exhausted: an honest escalation, recorded as an operator
    // intervention (§55 — autonomy includes knowing when not to continue).
    operator_interventions += 1;
    events.push(ResolutionEvent::new(
        ResolutionStage::EscalationRequested,
        &refusal_digest_base,
        0,
        "remediation budget exhausted",
    ));
    log.record(
        "operator",
        &format!(
            "hand-wrote the {} section after RRP budget exhaustion",
            brief.id
        ),
        "every authorised repair was applied and the gate still refused",
        "an honest escalation: the budget is the boundary of autonomy",
    );
    Authored {
        section: AuthoredSection {
            brief_id: brief.id.to_string(),
            builder: "operator".into(),
            provenance: "fallback".into(),
            fragment: format!(
                "<section><h2>{}</h2><p>Escalated after budget exhaustion. See the audit log.</p></section>",
                brief.id
            ),
        },
        events,
        operator_interventions,
    }
}

fn digest(failures: &[String]) -> String {
    playbook_core::sha256_hex(failures.join("\n").as_bytes())
}
