//! The deterministic 3form semantic projection layer.
//!
//! Authoritative Playbook state is the only input. Every presentation
//! object in this crate is a pure function of a `DriveState` (or of one
//! `DriveEvent`), so the same journal always projects the same frames —
//! the TUI, the web dashboards, the play cards and a replay recording
//! cannot drift from the doctrine they depict (the `playbook-cards`
//! invariant, extended).
//!
//! Layout-independent. No colours, no cursor control, no terminal
//! assumptions: `playbook-tui` is the first renderer, not the owner.

#![forbid(unsafe_code)]

mod replay;
mod resolution;
mod snapshot;
mod threeform;

pub use replay::{frames, Frame};
pub use resolution::{ProjectionParts, ResolutionSnapshot};
pub use snapshot::{
    BlitzOverlay, FieldProjection, GateRow, PlayerRow, PresentationSnapshot, ScoreBug, SegmentMark,
    ShadowCompare, SubstitutionRow,
};
pub use threeform::{kind, present_event, ThreeForm};
