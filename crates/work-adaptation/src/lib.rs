//! The work-adaptation subsystem (§9–§13) of Playbook's Refusal
//! Resolution Plane.
//!
//! Every function here is a pure transformation over data: no I/O, no
//! state mutation, no network. Repair touches execution conditions only
//! (§3.3) — briefs, envelopes, decomposition, anchors — never the truth
//! conditions of completion.
//!
//! - [`envelope`]: the EnvelopeController (§10) — never a blind doubling.
//! - [`brief`]: brief normalisation (§9.1) — adds clarity, never lowers
//!   standards.
//! - [`decompose`]: deterministic decomposition contracts (§11).
//! - [`assembly`]: structural verification of reassembled work (§12).

#![forbid(unsafe_code)]

pub mod assembly;
pub mod brief;
pub mod decompose;
pub mod envelope;

pub use assembly::{verify_assembly, AssemblyError};
pub use brief::{normalise_brief, normalise_observed_answer, semantic_requirement_met};
pub use decompose::{decompose, DecompositionError};
pub use envelope::{EnvelopeAction, EnvelopeController, EnvelopeDecision};
