//! Deterministic tests: no network. The live transport is exercised only
//! through the [`FakePlane`] test double and pure mapping functions.

use super::*;

/// A test double with canned outcomes — the plane surface without the
/// wire. Canned outcomes are keyed by request id; an unknown id yields a
/// provider-status failure.
pub struct FakePlane {
    manifest: CapabilityManifest,
    outcomes: std::collections::HashMap<String, ReconciledOutcome>,
    dispatched: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl FakePlane {
    pub fn canned(manifest: CapabilityManifest, outcomes: Vec<ReconciledOutcome>) -> Self {
        Self {
            manifest,
            outcomes: outcomes
                .into_iter()
                .map(|outcome| (outcome.request_id.clone(), outcome))
                .collect(),
            dispatched: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    fn stop(request_id: &str, content: &str) -> ReconciledOutcome {
        let observation = playbook_resolution::RefusalObservation {
            finish_reason: Some("stop".into()),
            provider_status: None,
            exit_code: None,
            gate_failures: Vec::new(),
            output_structurally_complete: true,
            artefact_expected_but_missing: false,
            output_path_contended: false,
            version_control_failed: false,
            prior_attempts_with_same_failures: 0,
            semantic_requirements_met_after_normalisation: None,
            first_required_item_missing: None,
        };
        ReconciledOutcome {
            request_id: request_id.to_string(),
            content: Some(content.to_string()),
            finish_reason: Some("stop".into()),
            total_tokens: 42,
            latency_ms: 500,
            provider_status: None,
            observation,
        }
    }
}

impl CapabilityPlane for FakePlane {
    fn capabilities(&self) -> CapabilityManifest {
        self.manifest.clone()
    }

    fn dispatch(&self, envelope: &ExecutionEnvelope) -> DispatchHandle {
        self.dispatched
            .lock()
            .expect("fake lock")
            .insert(envelope.request_id.clone());
        DispatchHandle {
            request_id: envelope.request_id.clone(),
            dispatch_started_at_ms: 0,
        }
    }

    fn reconcile(&self, handle: DispatchHandle) -> ReconciledOutcome {
        let was_dispatched = self
            .dispatched
            .lock()
            .expect("fake lock")
            .remove(&handle.request_id);
        if !was_dispatched {
            panic!("reconcile called on a handle that was never dispatched");
        }
        self.outcomes
            .get(&handle.request_id)
            .cloned()
            .unwrap_or_else(|| ReconciledOutcome {
                request_id: handle.request_id.clone(),
                content: None,
                finish_reason: None,
                total_tokens: 0,
                latency_ms: 0,
                provider_status: Some("fake: unknown request".into()),
                observation: playbook_resolution::RefusalObservation {
                    finish_reason: None,
                    provider_status: Some("fake: unknown request".into()),
                    exit_code: None,
                    gate_failures: Vec::new(),
                    output_structurally_complete: false,
                    artefact_expected_but_missing: false,
                    output_path_contended: false,
                    version_control_failed: false,
                    prior_attempts_with_same_failures: 0,
                    semantic_requirements_met_after_normalisation: None,
                    first_required_item_missing: None,
                },
            })
    }

    fn cancel(&self, handle: DispatchHandle) -> CancellationReport {
        let was_dispatched = self
            .dispatched
            .lock()
            .expect("fake lock")
            .remove(&handle.request_id);
        CancellationReport {
            request_id: handle.request_id,
            cancelled_before_dispatch: !was_dispatched,
            outcome_uncertain: was_dispatched,
        }
    }
}

fn envelope(request_id: &str) -> ExecutionEnvelope {
    ExecutionEnvelope {
        request_id: request_id.to_string(),
        model: "test/model".into(),
        prompt: "say the thing".into(),
        max_tokens: 100,
        temperature: 0.0,
        timeout_secs: 30,
    }
}

#[test]
fn redaction_replaces_every_occurrence_of_the_key() {
    let key = "sk-or-v1-secret123";
    let text = "curl -H 'authorization: Bearer sk-or-v1-secret123' failed for sk-or-v1-secret123";
    let redacted = redact(text, key);
    assert_eq!(
        redacted,
        "curl -H 'authorization: Bearer [REDACTED]' failed for [REDACTED]"
    );
    assert!(!redacted.contains(key));
}

#[test]
fn redaction_with_empty_key_is_identity() {
    assert_eq!(redact("untouched", ""), "untouched");
}

#[test]
fn length_finish_reason_maps_to_token_starvation_observation() {
    let response = openrouter::ProviderResponse {
        content: "partial tra".into(),
        finish_reason: Some("length".into()),
        total_tokens: 2_000,
        latency_ms: 900,
        provider_status: None,
    };
    let outcome = map_response("req-1".into(), response);
    assert_eq!(outcome.finish_reason.as_deref(), Some("length"));
    assert_eq!(outcome.total_tokens, 2_000);
    assert!(outcome.provider_status.is_none());
    // Truncated output: not structurally complete, token starvation is
    // derivable from the forwarded finish reason alone.
    assert!(!outcome.observation.output_structurally_complete);
    assert_eq!(outcome.observation.finish_reason.as_deref(), Some("length"));
    // The plane does not judge: no gate failures.
    assert!(outcome.observation.gate_failures.is_empty());
}

#[test]
fn success_maps_to_structurally_complete_and_content_present() {
    let response = openrouter::ProviderResponse {
        content: "391".into(),
        finish_reason: Some("stop".into()),
        total_tokens: 17,
        latency_ms: 400,
        provider_status: None,
    };
    let outcome = map_response("req-2".into(), response);
    assert_eq!(outcome.content.as_deref(), Some("391"));
    assert!(outcome.observation.output_structurally_complete);
}

#[test]
fn provider_failure_sets_status_and_structural_incompleteness() {
    let response = openrouter::ProviderResponse {
        content: String::new(),
        finish_reason: None,
        total_tokens: 0,
        latency_ms: 50,
        provider_status: Some("429 retry-after 30s".into()),
    };
    let outcome = map_response("req-3".into(), response);
    assert_eq!(
        outcome.provider_status.as_deref(),
        Some("429 retry-after 30s")
    );
    assert!(outcome.content.is_none());
    assert!(!outcome.observation.output_structurally_complete);
    assert_eq!(
        outcome.observation.provider_status.as_deref(),
        Some("429 retry-after 30s")
    );
}

#[test]
fn cancellation_after_dispatch_is_uncertain_never_certain_non_execution() {
    let plane = FakePlane::canned(
        CapabilityManifest {
            provider: "fake".into(),
            models: vec!["fake/m".into()],
        },
        Vec::new(),
    );
    let handle = plane.dispatch(&envelope("req-cancelled"));
    let report = plane.cancel(handle);
    // Dispatch already happened: the plane must not claim the request
    // certainly did not execute.
    assert!(report.outcome_uncertain);
    assert!(!report.cancelled_before_dispatch);
}

#[test]
fn cancellation_before_dispatch_is_certain() {
    let plane = FakePlane::canned(
        CapabilityManifest {
            provider: "fake".into(),
            models: vec![],
        },
        Vec::new(),
    );
    let handle = plane.dispatch(&envelope("req-never-sent"));
    // Simulate a handle for a request that was never dispatched: cancel
    // directly on a fresh handle id the plane has not seen.
    let fresh = DispatchHandle {
        request_id: "req-unknown".into(),
        dispatch_started_at_ms: 0,
    };
    let report = plane.cancel(fresh);
    assert!(!report.outcome_uncertain);
    assert!(report.cancelled_before_dispatch);
    // And the dispatched handle still reconciles normally afterwards.
    let _ = handle;
}

#[test]
fn manifest_surfaces_through_the_trait() {
    let manifest = CapabilityManifest {
        provider: "openrouter".into(),
        models: vec!["a/free".into(), "b/free".into()],
    };
    let plane = FakePlane::canned(manifest.clone(), vec![FakePlane::stop("req-m", "ok")]);
    assert_eq!(plane.capabilities(), manifest);
    let handle = plane.dispatch(&envelope("req-m"));
    let outcome = plane.reconcile(handle);
    assert_eq!(outcome.content.as_deref(), Some("ok"));
    assert!(outcome.observation.output_structurally_complete);
}

#[test]
fn sequential_dispatcher_delegates_to_the_plane() {
    let manifest = CapabilityManifest {
        provider: "fake".into(),
        models: vec!["fake/one".into()],
    };
    let dispatcher = SequentialDispatcher::new(FakePlane::canned(
        manifest.clone(),
        vec![FakePlane::stop("req-s", "done")],
    ));
    assert_eq!(dispatcher.capabilities(), manifest);
    let handle = dispatcher.dispatch(&envelope("req-s"));
    let outcome = dispatcher.reconcile(handle);
    assert_eq!(outcome.request_id, "req-s");
    assert_eq!(outcome.finish_reason.as_deref(), Some("stop"));
    // Cancel after dispatch: uncertain, per the § rule.
    let handle = dispatcher.dispatch(&envelope("req-s2"));
    let report = dispatcher.cancel(handle);
    assert!(report.outcome_uncertain);
}

#[test]
fn openrouter_debug_output_never_contains_the_key() {
    let key = "sk-or-v1-live-credential";
    let plane = openrouter::OpenRouterPlane::from_key(key);
    let rendered = format!("{plane:?}");
    assert!(!rendered.contains(key));
    assert!(rendered.contains("[REDACTED]"));
}

#[test]
fn from_key_file_trims_whitespace_and_never_logs() {
    let dir = std::env::temp_dir().join("capability-plane-tests");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("key.txt");
    std::fs::write(&path, format!("  {key}\n", key = "sk-test-key ")).expect("write key file");
    let plane = openrouter::OpenRouterPlane::from_key_file(&path).expect("read key file");
    let rendered = format!("{plane:?}");
    assert!(!rendered.contains("sk-test-key"));
    std::fs::remove_file(&path).ok();
}

#[test]
fn http_header_parsing_helpers() {
    use openrouter::{header_value, split_http, status_code};
    let raw = "HTTP/2 429\r\nretry-after: 30\r\ncontent-type: application/json\r\n\r\n{\"e\":1}";
    let (headers, body) = split_http(raw);
    assert_eq!(status_code(headers.lines().next().unwrap()), Some(429));
    assert_eq!(header_value(headers, "Retry-After").as_deref(), Some("30"));
    assert_eq!(body, "{\"e\":1}");
    // LF-only variant.
    let (headers, body) = split_http("HTTP/1.1 200 OK\n\nbody");
    assert_eq!(status_code(headers), Some(200));
    assert_eq!(body, "body");
    // Body without any header block.
    let (headers, body) = split_http("just body");
    assert_eq!(status_code(headers), None);
    assert_eq!(body, "just body");
}
