//! `ci-governance` — the continuous governance layer for Playbook's
//! evidence-gated CI campaign.
//!
//! Two concerns, one rule: never re-run the scanners.
//!
//! # Isopod (§19)
//!
//! CI asks "may this change enter?". Isopod asks a different question:
//! "is the organisation still operating the system that makes those
//! decisions trustworthy?". It consumes already-produced evidence and a
//! snapshot of governance state; it never executes scanners, never
//! derives a verdict from gate outputs, and never replaces
//! [`ci_evidence::verify_admission`]. It fails closed.
//!
//! # Padagonia (§20)
//!
//! The epistemic history graph: assertions of fact about the system,
//! related to evidence by reference. The graph stores only REFERENCES
//! (strings naming evidence envelopes); it never stores an evidence
//! payload. It is a map of what we believed and why — never a
//! replacement for the immutable evidence itself.

#![forbid(unsafe_code)]

pub mod isopod;
pub mod padagonia;

#[cfg(test)]
mod tests;

pub use isopod::{
    assess, governance_drift_detected, verdict, ControlReport, GovernanceState, GovernanceVerdict,
};
pub use padagonia::{Assertion, AssertionId, AssertionStatus, EdgeKind, EpistemicGraph, Relation};
