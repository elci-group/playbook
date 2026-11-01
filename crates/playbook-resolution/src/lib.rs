//! The Refusal Resolution Plane (RRP).
//!
//! Constitutional rule: the system that repairs a failure is never the
//! system that decides whether the repair succeeded. The classifier
//! answers *what happened*; the policy engine answers *what actions are
//! authorised*; the repair executor acts; the unchanged evidence authority
//! re-decides. No remediation may lower, bypass, reinterpret or satisfy a
//! gate — a repaired attempt earns admission through the same sovereign
//! authority as the original (§1, §3.1).
//!
//! Refusal is data (§3.2): every refusal is a structured, typed, hashed
//! record — never a log line. Repair touches execution conditions only
//! (§3.3): briefs, envelopes, decomposition, players, paths — never the
//! truth conditions of completion.

#![forbid(unsafe_code)]

pub mod anchors;
pub mod classify;
pub mod events;
pub mod lineage;
pub mod ownership;
pub mod policy;

pub use anchors::{check_anchors, AnchorVerdict, WorkUnitContract};
pub use classify::{
    classify, Confidence, Diagnosis, RefusalCause, RefusalFamily, RefusalObservation,
};
pub use events::{ResolutionEvent, ResolutionStage};
pub use lineage::ResolutionLineage;
pub use ownership::{ArtefactOwnership, OverwritePolicy, OwnershipVerdict};
pub use policy::{prescribe, Action, Prescription, ResolutionBudget, ResolutionPolicy};
