//! Live execution for the Playbook franchise.
//!
//! `v0.1.x` proved Playbook knows how an organisation should behave. This
//! crate is where it starts operating one: real tool execution through a
//! non-fixture [`CapabilityPlane`], live personnel identity, durable
//! at-least-once dispatch semantics, and explicit cancellation accounting —
//! with the constitutional rules intact.
//!
//! The rules this crate owns:
//!
//! * no inferred authority — every live invocation receives a complete
//!   [`ExecutionEnvelope`]; the executor never supplies a missing field
//!   itself;
//! * unknown cost is never zero — an invocation whose spend cannot be
//!   measured reports [`CostReport::unknown`] and the authority reserves
//!   capacity for it, never books it as free;
//! * stable empirical identity — a change of provider, model, profile, or
//!   tool envelope yields a distinct [`LiveIdentity`] and therefore a
//!   distinct personnel record, because comparability would otherwise be a
//!   lie;
//! * at-least-once dispatch with explicit reconciliation — invocation
//!   persisted, dispatch attempted, effect occurrence unknown, effect
//!   reconciled, outcome accepted or rejected. No retry happens merely
//!   because the caller did not receive a response;
//! * conservative cancellation — an unknown cancellation state is an
//!   unknown effect: it is reconciled, never optimistically booked;
//! * claims stay claims — a model response is packaged as an artefact for
//!   downstream verification and can never satisfy a gate by itself.

#![forbid(unsafe_code)]

pub mod cancel;
pub mod dispatch;
pub mod envelope;
pub mod faults;
pub mod identity;
pub mod provider;
pub mod toolplane;

pub use cancel::{cancellation_counts_as_effect_unknown, CancellationReport, CancellationState};
pub use dispatch::{DispatchLedger, DispatchRecord, DispatchStage};
pub use envelope::{CostReport, ExecutionEnvelope};
pub use faults::{Fault, FaultPlane};
pub use identity::LiveIdentity;
pub use provider::{ModelRequest, ModelResponse, ModelTransport, ProviderPlane, UnimplementedTransport};
pub use toolplane::{ToolCommand, ToolPlane, ToolOutcome};

#[cfg(test)]
mod tests {
    use super::*;

    /// The identity seam: a live plane built around a real working directory
    /// is not a fixture, and every module here must agree on that.
    #[test]
    fn the_live_vocabulary_is_coherent() {
        let identity = LiveIdentity::new("local", "cargo", "v1", "readonly-tools").unwrap();
        let id = identity.personnel_id().unwrap();
        assert!(id.as_str().starts_with("live-"));
        // A change of provider is a different empirical player, even with
        // the same model name.
        let other = LiveIdentity::new("remote", "cargo", "v1", "readonly-tools").unwrap();
        assert_ne!(id, other.personnel_id().unwrap());
        let _: Option<CancellationState> = None;
        let _: Option<DispatchStage> = None;
    }
}
