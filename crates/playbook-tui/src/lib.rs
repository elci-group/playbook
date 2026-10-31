//! The Matchday renderer: pure functions from a `PresentationSnapshot`
//! (or a `ThreeForm`) to a fixed-width text frame. No colours, no cursor
//! control, no I/O — the same frames the CLI paints are the frames a web
//! dashboard or a replay recording would render. The terminal is a
//! projection surface, never an authority.

#![forbid(unsafe_code)]

mod heat;
mod render;

pub use heat::render_heatmap;
pub use render::{
    render_blitz_overlay, render_broadcast, render_kick_decision, render_lastdown, render_manager,
    render_player_depth, render_scorebug, render_three, FormKey, BROADCAST_WIDTH, MANAGER_WIDTH,
};
