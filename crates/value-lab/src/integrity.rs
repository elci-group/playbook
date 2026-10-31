//! Directive §15 — the VALUE integrity gate.
//!
//! Playbook production code must never acquire authority over its own
//! evaluation. Any change that touches the research apparatus (thresholds,
//! utility weights, fault schedules, regime definitions, strata) triggers a
//! dedicated integrity gate with before/after protocol hashes as evidence.
//!
//! Verdict rules (mechanical, fail closed):
//! - Protocol hash unchanged and no apparatus flags: [`IntegrityVerdict::Clean`].
//! - Protocol hash changed with NO declared apparatus change: undeclared
//!   mutation — strictly worse than a declared one — [`IntegrityVerdict::Blocked`].
//! - Any declared apparatus change: [`IntegrityVerdict::RequiresReview`] at
//!   minimum; [`IntegrityVerdict::Blocked`] if the changeset also discards
//!   runs, or if the subject authored the change without a valid waiver
//!   (author plus a non-empty reason).
//! - A valid waiver downgrades exactly one level (Blocked → RequiresReview).
//!   It never yields Clean: a RequiresReview verdict stays RequiresReview
//!   with or without a waiver.

#![forbid(unsafe_code)]

use playbook_core::sha256_hex;

use crate::emit_protocol;

/// SHA-256 of `emit_protocol()` output. Computed exactly as the bin does:
/// `sha256_hex(emit_protocol().as_bytes())`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolHash(pub String);

impl ProtocolHash {
    pub fn compute() -> ProtocolHash {
        ProtocolHash(sha256_hex(emit_protocol().as_bytes()))
    }
}

/// Inputs to the integrity gate, gathered from the changeset under review.
#[derive(Debug, Clone)]
pub struct IntegrityInputs<'a> {
    pub before_protocol_hash: &'a str,
    pub after_protocol_hash: &'a str,
    pub thresholds_changed_in_changeset: bool,
    pub utility_weights_changed_in_changeset: bool,
    pub fault_schedule_changed_in_changeset: bool,
    pub regime_definitions_changed_in_changeset: bool,
    pub strata_changed_in_changeset: bool,
    pub runs_discarded_in_changeset: bool,
    pub subject_authored_the_change: bool,
    pub waiver_author: Option<&'a str>,
    pub waiver_reason: Option<&'a str>,
}

impl<'a> IntegrityInputs<'a> {
    /// True if any research-apparatus element is declared changed.
    pub fn apparatus_changed(&self) -> bool {
        self.thresholds_changed_in_changeset
            || self.utility_weights_changed_in_changeset
            || self.fault_schedule_changed_in_changeset
            || self.regime_definitions_changed_in_changeset
            || self.strata_changed_in_changeset
    }

    /// A waiver is valid only with a named author and a non-empty reason.
    pub fn waiver_valid(&self) -> bool {
        match (self.waiver_author, self.waiver_reason) {
            (Some(author), Some(reason)) => !author.trim().is_empty() && !reason.trim().is_empty(),
            _ => false,
        }
    }
}

/// Mechanical verdict of the integrity gate. Fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityVerdict {
    /// Protocol unchanged, no apparatus touched.
    Clean,
    /// Declared apparatus change; a human reviewer must sign off.
    RequiresReview,
    /// Undeclared mutation, run discarding, or unwaived subject authorship.
    Blocked,
}

impl IntegrityVerdict {
    pub fn key(&self) -> &'static str {
        match self {
            IntegrityVerdict::Clean => "clean",
            IntegrityVerdict::RequiresReview => "requires_review",
            IntegrityVerdict::Blocked => "blocked",
        }
    }
}

/// The §15 evidence bundle: before/after protocol hashes, the verdict, and
/// the list of triggered flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchIntegrityEvidence {
    pub before: String,
    pub after: String,
    pub verdict: IntegrityVerdict,
    pub flags: Vec<&'static str>,
}

/// Compute the integrity verdict mechanically. Fail closed: any ambiguity
/// resolves toward the harsher verdict.
pub fn integrity_gate(inputs: &IntegrityInputs) -> IntegrityVerdict {
    let hash_changed = inputs.before_protocol_hash != inputs.after_protocol_hash;
    let apparatus_changed = inputs.apparatus_changed();
    let waiver_valid = inputs.waiver_valid();

    let mut verdict = if hash_changed {
        if apparatus_changed {
            // Declared apparatus change: reviewable, unless escalated below.
            IntegrityVerdict::RequiresReview
        } else {
            // Hash changed with nothing declared: an undeclared mutation of
            // the research apparatus — worse than a declared one.
            IntegrityVerdict::Blocked
        }
    } else if apparatus_changed {
        IntegrityVerdict::RequiresReview
    } else {
        IntegrityVerdict::Clean
    };

    // Escalations.
    if inputs.runs_discarded_in_changeset {
        verdict = IntegrityVerdict::Blocked;
    }
    if inputs.subject_authored_the_change && !waiver_valid {
        verdict = IntegrityVerdict::Blocked;
    }

    // A valid waiver downgrades exactly one level (Blocked → RequiresReview).
    // It never yields Clean, and RequiresReview stays RequiresReview.
    if verdict == IntegrityVerdict::Blocked && waiver_valid {
        verdict = IntegrityVerdict::RequiresReview;
    }

    verdict
}

/// Bundle the before/after protocol hashes with the verdict and every
/// triggered flag. The hashes are the required evidence under §15.
pub fn gather(before: &str, after: &str, inputs: &IntegrityInputs) -> ResearchIntegrityEvidence {
    let hash_changed = before != after;
    let mut flags: Vec<&'static str> = Vec::new();

    if hash_changed {
        flags.push("protocol_hash_changed");
        if !inputs.apparatus_changed() {
            flags.push("undeclared_protocol_mutation");
        }
    }
    if inputs.thresholds_changed_in_changeset {
        flags.push("thresholds_changed");
    }
    if inputs.utility_weights_changed_in_changeset {
        flags.push("utility_weights_changed");
    }
    if inputs.fault_schedule_changed_in_changeset {
        flags.push("fault_schedule_changed");
    }
    if inputs.regime_definitions_changed_in_changeset {
        flags.push("regime_definitions_changed");
    }
    if inputs.strata_changed_in_changeset {
        flags.push("strata_changed");
    }
    if inputs.runs_discarded_in_changeset {
        flags.push("runs_discarded");
    }
    if inputs.subject_authored_the_change {
        if inputs.waiver_valid() {
            flags.push("subject_authored_with_waiver");
        } else {
            flags.push("subject_authored_without_waiver");
        }
    }

    ResearchIntegrityEvidence {
        before: before.to_string(),
        after: after.to_string(),
        verdict: integrity_gate(inputs),
        flags,
    }
}

/// True iff the before and after protocol hashes match.
pub fn verify_protocol_unchanged(before: &str, after: &str) -> bool {
    before == after
}

/// §15 forbidden-authority self-check: returns every prohibition violated.
pub fn forbidden_authority_detected(
    production_code_touches_value_lab: bool,
    subject_declares_own_verdict: bool,
    frozen_snapshot_rewritten: bool,
) -> Vec<&'static str> {
    let mut violations = Vec::new();
    if production_code_touches_value_lab {
        violations.push("production_code_must_not_touch_the_lab");
    }
    if subject_declares_own_verdict {
        violations.push("subject_must_not_declare_its_own_verdict");
    }
    if frozen_snapshot_rewritten {
        violations.push("frozen_snapshots_must_not_be_rewritten");
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_inputs<'a>(before: &'a str, after: &'a str) -> IntegrityInputs<'a> {
        IntegrityInputs {
            before_protocol_hash: before,
            after_protocol_hash: after,
            thresholds_changed_in_changeset: false,
            utility_weights_changed_in_changeset: false,
            fault_schedule_changed_in_changeset: false,
            regime_definitions_changed_in_changeset: false,
            strata_changed_in_changeset: false,
            runs_discarded_in_changeset: false,
            subject_authored_the_change: false,
            waiver_author: None,
            waiver_reason: None,
        }
    }

    #[test]
    fn unchanged_hashes_no_flags_is_clean() {
        let inputs = base_inputs("abc", "abc");
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::Clean);
    }

    #[test]
    fn threshold_change_requires_review_and_names_the_flag() {
        let inputs = base_inputs("abc", "abc");
        let inputs = IntegrityInputs {
            thresholds_changed_in_changeset: true,
            ..inputs
        };
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::RequiresReview);
        let evidence = gather("abc", "abc", &inputs);
        assert!(evidence.flags.contains(&"thresholds_changed"));
        assert_eq!(evidence.verdict, IntegrityVerdict::RequiresReview);
    }

    #[test]
    fn undeclared_hash_change_is_blocked() {
        let inputs = base_inputs("abc", "def");
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::Blocked);
        let evidence = gather("abc", "def", &inputs);
        assert!(evidence.flags.contains(&"undeclared_protocol_mutation"));
    }

    #[test]
    fn discarding_runs_is_blocked() {
        let inputs = IntegrityInputs {
            utility_weights_changed_in_changeset: true,
            runs_discarded_in_changeset: true,
            ..base_inputs("abc", "abc")
        };
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::Blocked);
    }

    #[test]
    fn subject_authored_blocked_without_waiver_requires_review_with_waiver() {
        let without = IntegrityInputs {
            thresholds_changed_in_changeset: true,
            subject_authored_the_change: true,
            ..base_inputs("abc", "abc")
        };
        assert_eq!(integrity_gate(&without), IntegrityVerdict::Blocked);

        let with = IntegrityInputs {
            waiver_author: Some("reviewer"),
            waiver_reason: Some("approved apparatus tweak"),
            ..without
        };
        assert_eq!(integrity_gate(&with), IntegrityVerdict::RequiresReview);
    }

    #[test]
    fn waiver_never_yields_clean() {
        let cases = [
            // Blocked by undeclared mutation, waiver present.
            IntegrityInputs {
                waiver_author: Some("reviewer"),
                waiver_reason: Some("known doc-only drift"),
                ..base_inputs("abc", "def")
            },
            // Blocked by run discarding, waiver present.
            IntegrityInputs {
                runs_discarded_in_changeset: true,
                waiver_author: Some("reviewer"),
                waiver_reason: Some("approved"),
                ..base_inputs("abc", "abc")
            },
            // RequiresReview, waiver present.
            IntegrityInputs {
                strata_changed_in_changeset: true,
                waiver_author: Some("reviewer"),
                waiver_reason: Some("approved"),
                ..base_inputs("abc", "abc")
            },
        ];
        for inputs in cases {
            assert_ne!(integrity_gate(&inputs), IntegrityVerdict::Clean);
        }
    }

    #[test]
    fn waiver_requires_author_and_nonempty_reason() {
        let inputs = IntegrityInputs {
            subject_authored_the_change: true,
            waiver_author: Some("reviewer"),
            waiver_reason: Some("   "),
            ..base_inputs("abc", "abc")
        };
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::Blocked);
    }

    #[test]
    fn forbidden_authority_names_each_violation() {
        assert!(forbidden_authority_detected(false, false, false).is_empty());
        let v = forbidden_authority_detected(true, true, true);
        assert_eq!(v.len(), 3);
        assert!(v.contains(&"production_code_must_not_touch_the_lab"));
        assert!(v.contains(&"subject_must_not_declare_its_own_verdict"));
        assert!(v.contains(&"frozen_snapshots_must_not_be_rewritten"));
    }

    #[test]
    fn verify_protocol_unchanged_round_trips_real_protocol_hash() {
        // emit_protocol is deterministic: hashing it twice must agree, and
        // verify_protocol_unchanged must accept the match and reject drift.
        let before = ProtocolHash::compute();
        let after = ProtocolHash::compute();
        assert_eq!(before, after);
        assert!(verify_protocol_unchanged(&before.0, &after.0));
        assert!(!verify_protocol_unchanged(&before.0, "deadbeef"));
        // And the gate itself is Clean on the real hashes.
        let inputs = base_inputs(&before.0, &after.0);
        assert_eq!(integrity_gate(&inputs), IntegrityVerdict::Clean);
    }
}
