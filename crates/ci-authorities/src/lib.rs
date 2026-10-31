//! `ci-authorities` — adapter crates mapping portfolio specialist tools
//! (traci, amber, scrawny, verity) into CI gate evidence for Playbook's
//! evidence-gated CI campaign (`CI_EVIDENCE/1`).
//!
//! Constitutional posture (ci-evidence §4/§30): authorities observe,
//! policy decides. Every adapter here fails closed — a specialist that
//! cannot run yields `Error` or `SkippedNotApplicable`, never `Pass`.
//! When a specialist binary is absent the gate is *not applicable* in
//! this environment and is skipped with an explicit finding, so the
//! omission is itself recorded evidence.

#![forbid(unsafe_code)]

pub mod amber;
pub mod runner;
pub mod scrawny;
pub mod traci;
pub mod verity;

use ci_evidence::GateClassification;

/// The declaration of one specialist gate: which gate it is, which
/// authority owns it, and what class of evidence it produces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateSpec {
    pub gate_id: String,
    pub authority: String,
    pub classification: GateClassification,
}

impl GateSpec {
    pub fn new(
        gate_id: impl Into<String>,
        authority: impl Into<String>,
        classification: GateClassification,
    ) -> Self {
        Self {
            gate_id: gate_id.into(),
            authority: authority.into(),
            classification,
        }
    }
}

#[cfg(test)]
mod tests;
