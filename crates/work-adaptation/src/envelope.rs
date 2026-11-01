//! The EnvelopeController (§10): expands the token envelope when a
//! refusal was caused by truncation, but never blindly.
//!
//! Policy:
//! 1. Non-truncation is never an envelope problem — Keep.
//! 2. Truncation with the output structurally *incomplete* (less than
//!    roughly half of the expected structure was produced) means the unit
//!    cannot fit at any reasonable envelope: Split is preferred, provided
//!    the prescription authorises `DecomposeWork`.
//! 3. Truncation with a structurally complete-but-truncated output is an
//!    envelope-size problem: Expand — multiply the current envelope by 4
//!    (8 for reasoning models, whose hidden reasoning tokens also consume
//!    the envelope), or adopt the prescription's suggested envelope when
//!    it is at least that large, capped at the ceiling.
//! 4. Beyond the ceiling there is nothing left to expand: Split when
//!    decomposition is authorised, otherwise Keep with an escalation
//!    rationale (the escalation itself is the prescription's to carry).

use playbook_resolution::{Action, Prescription};

/// The decision the controller reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeDecision {
    pub action: EnvelopeAction,
    pub new_envelope: Option<u64>,
    pub rationale: String,
}

/// Expand, Split, or Keep the current envelope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeAction {
    Expand,
    Split,
    Keep,
}

/// Multiplier applied on expansion (§10: not a blind doubling).
const EXPAND_FACTOR: u64 = 4;
/// Reasoning models spend envelope on hidden reasoning tokens too.
const EXPAND_FACTOR_REASONING: u64 = 8;

/// What counts as a truncation finish reason.
fn is_truncation(finish_reason: Option<&str>) -> bool {
    matches!(
        finish_reason.map(str::to_ascii_lowercase).as_deref(),
        Some("length") | Some("max_tokens") | Some("truncated") | Some("max_output_tokens")
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeController {
    pub current_envelope: u64,
    pub ceiling: u64,
}

impl EnvelopeController {
    pub fn new(current_envelope: u64, ceiling: u64) -> Self {
        Self {
            current_envelope,
            ceiling,
        }
    }

    /// Decide how to adapt the envelope for the next attempt.
    pub fn adapt(
        &self,
        finish_reason: Option<&str>,
        output_structurally_complete: bool,
        prescription: &Prescription,
        reasoning_model: bool,
    ) -> EnvelopeDecision {
        if !is_truncation(finish_reason) {
            return EnvelopeDecision {
                action: EnvelopeAction::Keep,
                new_envelope: Some(self.current_envelope),
                rationale: "refusal was not caused by truncation; the envelope is not the fault"
                    .into(),
            };
        }

        let decompose_authorised = prescription.actions.contains(&Action::DecomposeWork);
        let factor = if reasoning_model {
            EXPAND_FACTOR_REASONING
        } else {
            EXPAND_FACTOR
        };

        // The unit cannot fit at any reasonable envelope: split it.
        if !output_structurally_complete && decompose_authorised {
            let chunks = prescription
                .suggested_chunks
                .map(|c| c.to_string())
                .unwrap_or_else(|| "an authorised count".into());
            return EnvelopeDecision {
                action: EnvelopeAction::Split,
                new_envelope: Some(self.current_envelope),
                rationale: format!(
                    "truncated with the output structurally incomplete: the delivered portion \
                     was far under half the expected structure, so no reasonable envelope fits \
                     the unit; split into {chunks} independently verifiable children (§11)"
                ),
            };
        }

        // Try to expand, up to the ceiling.
        let expanded = self
            .current_envelope
            .saturating_mul(factor)
            .min(self.ceiling);
        let candidate = match prescription.suggested_envelope {
            Some(suggested) if suggested >= expanded => suggested.min(self.ceiling),
            _ => expanded,
        };

        if candidate > self.current_envelope {
            return EnvelopeDecision {
                action: EnvelopeAction::Expand,
                new_envelope: Some(candidate),
                rationale: format!(
                    "truncated but structurally (near-)complete: the unit fits, the envelope \
                     did not; expand {old} -> {candidate} (ceiling {ceiling}, factor {factor})",
                    old = self.current_envelope,
                    ceiling = self.ceiling,
                ),
            };
        }

        // Beyond the ceiling: nothing left to expand.
        if decompose_authorised {
            EnvelopeDecision {
                action: EnvelopeAction::Split,
                new_envelope: Some(self.current_envelope),
                rationale: format!(
                    "truncation persists at the ceiling ({ceiling}); expansion is exhausted, \
                     so split the unit into authorised children (§11)",
                    ceiling = self.ceiling
                ),
            }
        } else {
            EnvelopeDecision {
                action: EnvelopeAction::Keep,
                new_envelope: Some(self.current_envelope),
                rationale: format!(
                    "truncation persists at the ceiling ({ceiling}) and decomposition is not \
                     authorised; keep the envelope and escalate per the prescription (§55)",
                    ceiling = self.ceiling
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_resolution::Prescription;

    fn rx(actions: &[Action], envelope: Option<u64>, chunks: Option<u32>) -> Prescription {
        Prescription {
            actions: actions.to_vec(),
            suggested_envelope: envelope,
            suggested_chunks: chunks,
            rationale: "test".into(),
            escalated: false,
        }
    }

    const CEILING: u64 = 64_000;

    fn controller(current: u64) -> EnvelopeController {
        EnvelopeController::new(current, CEILING)
    }

    #[test]
    fn non_truncation_keeps() {
        for reason in [None, Some("stop"), Some("tool_calls")] {
            let d = controller(4_000).adapt(
                reason,
                true,
                &rx(&[Action::ExpandEnvelope], None, None),
                false,
            );
            assert_eq!(d.action, EnvelopeAction::Keep);
            assert_eq!(d.new_envelope, Some(4_000));
        }
    }

    #[test]
    fn truncation_structurally_complete_expands_by_factor() {
        let d = controller(4_000).adapt(Some("length"), true, &rx(&[], None, None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(16_000));
    }

    #[test]
    fn reasoning_model_expands_by_eight() {
        let d = controller(4_000).adapt(Some("length"), true, &rx(&[], None, None), true);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(32_000));
    }

    #[test]
    fn expansion_capped_at_ceiling() {
        let d = controller(50_000).adapt(Some("max_tokens"), true, &rx(&[], None, None), true);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(CEILING));
    }

    #[test]
    fn suggested_envelope_adopted_when_larger() {
        let d = controller(4_000).adapt(Some("length"), true, &rx(&[], Some(20_000), None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(20_000));
    }

    #[test]
    fn suggested_envelope_capped_at_ceiling() {
        let d =
            controller(4_000).adapt(Some("length"), true, &rx(&[], Some(1_000_000), None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(CEILING));
    }

    #[test]
    fn suggested_envelope_ignored_when_smaller_than_factor() {
        let d = controller(4_000).adapt(Some("length"), true, &rx(&[], Some(5_000), None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(16_000));
    }

    #[test]
    fn incomplete_output_with_decompose_authorised_splits() {
        let d = controller(4_000).adapt(
            Some("length"),
            false,
            &rx(
                &[Action::DecomposeWork, Action::ImposeAnchors],
                None,
                Some(5),
            ),
            false,
        );
        assert_eq!(d.action, EnvelopeAction::Split);
        assert_eq!(d.new_envelope, Some(4_000));
        assert!(d.rationale.contains('5'));
    }

    #[test]
    fn incomplete_output_without_decompose_authorised_expands() {
        let d = controller(4_000).adapt(Some("length"), false, &rx(&[], None, None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(16_000));
    }

    #[test]
    fn at_ceiling_with_decompose_splits() {
        let d = controller(CEILING).adapt(
            Some("length"),
            true,
            &rx(&[Action::DecomposeWork], None, None),
            false,
        );
        assert_eq!(d.action, EnvelopeAction::Split);
        assert_eq!(d.new_envelope, Some(CEILING));
    }

    #[test]
    fn at_ceiling_without_decompose_keeps_with_escalation_rationale() {
        let d = controller(CEILING).adapt(Some("length"), true, &rx(&[], None, None), false);
        assert_eq!(d.action, EnvelopeAction::Keep);
        assert_eq!(d.new_envelope, Some(CEILING));
        assert!(d.rationale.contains("escalate"));
    }

    #[test]
    fn truncation_reason_is_case_insensitive() {
        let d = controller(4_000).adapt(Some("LENGTH"), true, &rx(&[], None, None), false);
        assert_eq!(d.action, EnvelopeAction::Expand);
        assert_eq!(d.new_envelope, Some(16_000));
    }
}
