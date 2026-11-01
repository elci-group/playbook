//! The Live CapabilityPlane (directive §14): the surface through which an
//! execution envelope reaches a real model provider and comes back as a
//! `ReconciledOutcome` carrying a `playbook_resolution::RefusalObservation`.
//!
//! Constitutional position: the plane OBSERVES — it forwards provider
//! metadata verbatim (finish reason, provider status, structural
//! completeness) and never judges. `gate_failures` is always empty here;
//! classification belongs to the RRP via `playbook_resolution::classify`.
//!
//! Cancellation uncertainty (§ cancellation): a dispatch that may already
//! be in flight is never reported as certainly not executed. If
//! cancellation arrives after dispatch began, `outcome_uncertain` is true.

#![forbid(unsafe_code)]

pub mod openrouter;

pub use openrouter::OpenRouterPlane;

/// What the plane can reach (§14): the provider and its model roster.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityManifest {
    pub provider: String,
    pub models: Vec<String>,
}

/// One unit of work handed to the plane.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionEnvelope {
    pub request_id: String,
    pub model: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub timeout_secs: u64,
}

/// A dispatch receipt: proof the request was accepted, nothing more.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DispatchHandle {
    pub request_id: String,
    pub dispatch_started_at_ms: u64,
}

/// The reconciled result of one dispatch. Carries the raw refusal
/// observation so the RRP can classify directly — the plane does not
/// interpret.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReconciledOutcome {
    pub request_id: String,
    pub content: Option<String>,
    pub finish_reason: Option<String>,
    pub total_tokens: u64,
    pub latency_ms: u64,
    pub provider_status: Option<String>,
    pub observation: playbook_resolution::RefusalObservation,
}

/// The report of a cancellation. § cancellation uncertainty: never
/// represent an uncertain cancellation as certain non-execution.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CancellationReport {
    pub request_id: String,
    pub cancelled_before_dispatch: bool,
    /// True when the request may already have been sent: the caller must
    /// treat execution state as unknown, not as "did not run".
    pub outcome_uncertain: bool,
}

/// The live capability surface (§14).
pub trait CapabilityPlane {
    fn capabilities(&self) -> CapabilityManifest;
    fn dispatch(&self, envelope: &ExecutionEnvelope) -> DispatchHandle;
    fn reconcile(&self, handle: DispatchHandle) -> ReconciledOutcome;
    fn cancel(&self, handle: DispatchHandle) -> CancellationReport;
}

/// Replace every occurrence of `key` in `text` with `[REDACTED]`. Applied
/// to everything the plane logs or returns in an error — the credential
/// itself must never survive into any message.
pub fn redact(text: &str, key: &str) -> String {
    if key.is_empty() {
        return text.to_string();
    }
    text.replace(key, "[REDACTED]")
}

/// An empty observation: the plane's neutral starting point. Every field
/// is "unknown / not judged".
fn blank_observation() -> playbook_resolution::RefusalObservation {
    playbook_resolution::RefusalObservation {
        finish_reason: None,
        provider_status: None,
        exit_code: None,
        gate_failures: Vec::new(),
        output_structurally_complete: false,
        artefact_expected_but_missing: false,
        output_path_contended: false,
        version_control_failed: false,
        prior_attempts_with_same_failures: 0,
        semantic_requirements_met_after_normalisation: None,
        first_required_item_missing: None,
    }
}

/// Map one provider response onto a `ReconciledOutcome` and its carried
/// observation. Mapping decisions (§14):
/// - `finish_reason` is forwarded verbatim (e.g. `"length"` lets the RRP
///   derive token starvation without inference).
/// - `provider_status` is `Some` on any non-success (HTTP status text,
///   transport error, unparseable body); `None` only on success. A 429
///   with a captured retry-after becomes `"429 retry-after Ns"`.
/// - `output_structurally_complete` is content non-empty AND finish
///   reason `"stop"` — structural only, no semantics.
/// - `gate_failures` is always empty: the plane does not judge.
pub fn map_response(
    request_id: String,
    response: openrouter::ProviderResponse,
) -> ReconciledOutcome {
    let success = response.provider_status.is_none();
    let content = if success && !response.content.is_empty() {
        Some(response.content.clone())
    } else {
        None
    };
    let finish_reason = response.finish_reason.clone();
    let output_structurally_complete =
        content.is_some() && finish_reason.as_deref() == Some("stop");
    let observation = playbook_resolution::RefusalObservation {
        finish_reason: finish_reason.clone(),
        provider_status: response.provider_status.clone(),
        output_structurally_complete,
        ..blank_observation()
    };
    ReconciledOutcome {
        request_id,
        content,
        finish_reason,
        total_tokens: response.total_tokens,
        latency_ms: response.latency_ms,
        provider_status: response.provider_status,
        observation,
    }
}

/// A simple in-process executor: `dispatch` records the receipt,
/// `reconcile` performs the synchronous OpenRouter call. There is no
/// queue, no concurrency and no retry — parallel dispatch (a dispatcher
/// with a worker pool and per-request state) is future work; this
/// dispatcher is intentionally sequential so that every outcome maps to
/// exactly one visible call.
pub struct SequentialDispatcher<P: CapabilityPlane> {
    plane: P,
}

impl<P: CapabilityPlane> SequentialDispatcher<P> {
    pub fn new(plane: P) -> Self {
        Self { plane }
    }
}

impl<P: CapabilityPlane> CapabilityPlane for SequentialDispatcher<P> {
    fn capabilities(&self) -> CapabilityManifest {
        self.plane.capabilities()
    }

    fn dispatch(&self, envelope: &ExecutionEnvelope) -> DispatchHandle {
        self.plane.dispatch(envelope)
    }

    fn reconcile(&self, handle: DispatchHandle) -> ReconciledOutcome {
        self.plane.reconcile(handle)
    }

    fn cancel(&self, handle: DispatchHandle) -> CancellationReport {
        self.plane.cancel(handle)
    }
}

#[cfg(test)]
mod tests;
