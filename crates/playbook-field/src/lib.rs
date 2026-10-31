//! The YTG field geometry: the completion-economics protocol.
//!
//! The revised field model derives deterministic completion geometry from
//! the explicit gate graph — validated frontier, unvalidated progress,
//! tenth-yard validation boundaries, causal field adjustments, semantic Red
//! Zone, and terminal-action economics — while the gate graph remains the
//! authoritative record and evidence remains the only thing that moves the
//! validated state.
//!
//! The doctrine in one line per module:
//!
//! * `geometry` — banked versus unvalidated progress and the 9+1 segment
//!   rhythm: every tenth yard is validation, not production;
//! * `kicker` — the Opposition Kicker constructs the initial field from a
//!   bounded (≤99-gate) decomposition and is measured on its own accuracy;
//! * `penalties` — topology changes carry causal provenance: exogenous,
//!   discovery, kicker estimation error, or execution-created scope never
//!   masquerade as each other;
//! * `redzone` — the Red Zone is session-closure feasibility, not yardage;
//! * `fieldgoal` — field-goal range is terminal-action economics: one
//!   bounded kick versus continued progression, never a fixed threshold;
//! * `routing` — Playbook chooses continue / validate / kick / Lastdown
//!   from the field state itself.

#![forbid(unsafe_code)]

pub mod domain;
pub mod fieldgoal;
pub mod geometry;
pub mod kicker;
pub mod penalties;
pub mod redzone;
pub mod routing;
pub mod shadow;
pub mod state;

pub use domain::{
    field_length_yards, field_refusal, validate_gate_count, AdjustmentDirection,
    BoundaryRequirement, BoundaryState, ConsolidationCheckpoint, FieldAdjustment, FieldPosition,
    Foreseeability, KickOutcome, KickUtility, MovementAttribution, ProgressRefusal,
    RedZoneAssessment, ValidationBoundary, ValidationEvidence, MAX_PRODUCTIVE_GATES,
    MAX_UNVALIDATED_YARDS, SEGMENT_YARDS,
};
pub use geometry::{
    fail_validation, pass_validation, record_progress as record_field_progress, SegmentLedger,
    SegmentRecord,
};

#[cfg(test)]
mod tests {
    use super::SEGMENT_YARDS;

    #[test]
    fn the_field_crate_stands_up() {
        // The crate compiles; module tests carry the depth.
        assert_eq!(SEGMENT_YARDS, 10);
    }
}
