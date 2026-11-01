//! The OpenRouter transport (§15): chat completions and the model roster
//! over the system `curl`, following the same external-command pattern as
//! `value-lab::live` (PATH prefix `$HOME/.cargo/bin` for children, the key
//! read from a file and never logged).

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::{
    map_response, CancellationReport, CapabilityManifest, CapabilityPlane, DispatchHandle,
    ExecutionEnvelope, ReconciledOutcome,
};

const CHAT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_ENDPOINT: &str = "https://openrouter.ai/api/v1/models";

/// One raw provider exchange, before observation mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderResponse {
    pub content: String,
    pub finish_reason: Option<String>,
    pub total_tokens: u64,
    pub latency_ms: u64,
    /// `None` on success; otherwise the HTTP status / transport / parse
    /// failure text. A 429 with a captured retry-after reads
    /// `"429 retry-after Ns"`.
    pub provider_status: Option<String>,
}

/// State bound at dispatch time and consumed at reconcile time.
struct InFlight {
    started: Instant,
    envelope: ExecutionEnvelope,
}

/// The production plane. Construct with [`OpenRouterPlane::from_key_file`]
/// or [`OpenRouterPlane::from_key`]; the key is held in memory and is
/// never logged, echoed, or included in any returned error (all error text
/// passes through [`crate::redact`]).
pub struct OpenRouterPlane {
    key: String,
    in_flight: Mutex<HashMap<String, InFlight>>,
}

impl std::fmt::Debug for OpenRouterPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key never appears in debug output either.
        f.debug_struct("OpenRouterPlane")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl OpenRouterPlane {
    /// The caller owns the secret from here on; it is never logged.
    pub fn from_key(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// Read and trim the key from `path`. The file contents are never
    /// logged.
    pub fn from_key_file(path: &std::path::Path) -> std::io::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Ok(Self::from_key(raw.trim()))
    }

    /// The curl invocation with the project's child-process PATH prefix.
    fn curl(args: &[&str]) -> std::io::Result<std::process::Output> {
        let path_prefix = format!(
            "$HOME/.cargo/bin:{path}",
            path = std::env::var("PATH").unwrap_or_default()
        );
        std::process::Command::new("curl")
            .args(args)
            .env("PATH", path_prefix)
            .output()
    }

    /// One chat completion over the wire. Response headers are captured
    /// (`-i`) so a 429's retry-after can be surfaced into
    /// `provider_status`. The key is only ever placed in a request header,
    /// never in an argument that could be logged, and every failure text
    /// is redacted against it before leaving this function.
    pub fn chat_raw(&self, envelope: &ExecutionEnvelope) -> ProviderResponse {
        let started = Instant::now();
        let body = serde_json::json!({
            "model": envelope.model,
            "messages": [{"role": "user", "content": envelope.prompt}],
            "max_tokens": envelope.max_tokens,
            "temperature": envelope.temperature,
        });
        let output = Self::curl(&[
            "-s",
            "-i",
            "--max-time",
            &envelope.timeout_secs.to_string(),
            "-X",
            "POST",
            CHAT_ENDPOINT,
            "-H",
            "content-type: application/json",
            "-H",
            &format!("authorization: Bearer {}", self.key),
            "-d",
            &body.to_string(),
        ]);
        let latency_ms = started.elapsed().as_millis() as u64;
        match output {
            Ok(output) if output.status.success() => {
                let raw = String::from_utf8_lossy(&output.stdout);
                let (headers, body) = split_http(&raw);
                let status_line = headers.lines().next().unwrap_or_default().to_string();
                if let Some(code) = status_code(&status_line) {
                    if code != 200 {
                        let mut text = format!("{code}");
                        if code == 429 {
                            if let Some(retry) = header_value(headers, "retry-after") {
                                text.push_str(&format!(" retry-after {retry}s"));
                            }
                        }
                        return ProviderResponse {
                            content: String::new(),
                            finish_reason: None,
                            total_tokens: 0,
                            latency_ms,
                            provider_status: Some(crate::redact(&text, &self.key)),
                        };
                    }
                }
                match serde_json::from_str::<serde_json::Value>(body) {
                    Ok(parsed) => {
                        if let Some(error) = parsed.get("error").filter(|e| e.is_object()) {
                            let message = error["message"].as_str().unwrap_or("unknown");
                            return ProviderResponse {
                                content: String::new(),
                                finish_reason: None,
                                total_tokens: 0,
                                latency_ms,
                                provider_status: Some(crate::redact(
                                    &format!("provider error: {message}"),
                                    &self.key,
                                )),
                            };
                        }
                        ProviderResponse {
                            content: parsed["choices"][0]["message"]["content"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            finish_reason: parsed["choices"][0]["finish_reason"]
                                .as_str()
                                .map(str::to_string),
                            total_tokens: parsed["usage"]["total_tokens"].as_u64().unwrap_or(0),
                            latency_ms,
                            provider_status: None,
                        }
                    }
                    Err(error) => ProviderResponse {
                        content: String::new(),
                        finish_reason: None,
                        total_tokens: 0,
                        latency_ms,
                        provider_status: Some(crate::redact(
                            &format!("unparseable response: {error}"),
                            &self.key,
                        )),
                    },
                }
            }
            Ok(output) => ProviderResponse {
                content: String::new(),
                finish_reason: None,
                total_tokens: 0,
                latency_ms,
                provider_status: Some(crate::redact(
                    &format!("curl exit {}", output.status.code().unwrap_or(-1)),
                    &self.key,
                )),
            },
            Err(error) => ProviderResponse {
                content: String::new(),
                finish_reason: None,
                total_tokens: 0,
                latency_ms,
                provider_status: Some(crate::redact(&format!("spawn failed: {error}"), &self.key)),
            },
        }
    }

    /// The provider's model roster (`:free` ids, sorted, truncated to
    /// `limit`), via the public models endpoint.
    pub fn free_models(&self, limit: usize) -> Vec<String> {
        let output = Self::curl(&[
            "-s",
            "--max-time",
            "20",
            MODELS_ENDPOINT,
            "-H",
            &format!("authorization: Bearer {}", self.key),
        ]);
        let Ok(output) = output else {
            return Vec::new();
        };
        let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
        let mut models: Vec<String> = parsed["data"]
            .as_array()
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry["id"].as_str())
                    .filter(|id| id.ends_with(":free"))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        models.sort();
        models.truncate(limit);
        models
    }
}

/// Split a raw `-i` response into headers and body at the first blank
/// line (tolerating both CRLF and LF).
pub(crate) fn split_http(raw: &str) -> (&str, &str) {
    for (sep, len) in [("\r\n\r\n", 4), ("\n\n", 2)] {
        if let Some(index) = raw.find(sep) {
            return (&raw[..index], &raw[index + len..]);
        }
    }
    // No header block at all: treat everything as body.
    ("", raw)
}

/// Extract the numeric status from an HTTP status line.
pub(crate) fn status_code(status_line: &str) -> Option<u16> {
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
}

/// Case-insensitive header lookup.
pub(crate) fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim().eq_ignore_ascii_case(name) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

impl CapabilityPlane for OpenRouterPlane {
    fn capabilities(&self) -> CapabilityManifest {
        CapabilityManifest {
            provider: "openrouter".to_string(),
            models: self.free_models(50),
        }
    }

    fn dispatch(&self, envelope: &ExecutionEnvelope) -> DispatchHandle {
        self.in_flight
            .lock()
            .expect("in-flight lock poisoned")
            .insert(
                envelope.request_id.clone(),
                InFlight {
                    started: Instant::now(),
                    envelope: envelope.clone(),
                },
            );
        DispatchHandle {
            request_id: envelope.request_id.clone(),
            dispatch_started_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }

    /// Perform the synchronous call bound at dispatch time and map the
    /// provider exchange onto a `ReconciledOutcome` carrying its refusal
    /// observation.
    fn reconcile(&self, handle: DispatchHandle) -> ReconciledOutcome {
        let in_flight = self
            .in_flight
            .lock()
            .expect("in-flight lock poisoned")
            .remove(&handle.request_id);
        let (bound, envelope) = match &in_flight {
            Some(state) => (true, state.envelope.clone()),
            None => {
                // Reconciling an unknown handle: nothing was bound, so there is
                // nothing to send. Report an unknown-state failure observation.
                (
                    false,
                    ExecutionEnvelope {
                        request_id: handle.request_id.clone(),
                        model: String::new(),
                        prompt: String::new(),
                        max_tokens: 0,
                        temperature: 0.0,
                        timeout_secs: 1,
                    },
                )
            }
        };
        let mut response = self.chat_raw(&envelope);
        if bound {
            // End-to-end latency from the moment the caller dispatched.
            response.latency_ms = in_flight
                .as_ref()
                .map(|state| state.started.elapsed().as_millis() as u64)
                .unwrap_or(response.latency_ms);
        } else {
            response.provider_status = Some("unknown handle: request was never dispatched".into());
        }
        map_response(handle.request_id, response)
    }

    /// Mark the request cancelled. Once dispatched, the request may
    /// already be in flight at the provider: the report never claims
    /// certain non-execution (§ cancellation uncertainty).
    fn cancel(&self, handle: DispatchHandle) -> CancellationReport {
        let was_dispatched = self
            .in_flight
            .lock()
            .expect("in-flight lock poisoned")
            .remove(&handle.request_id)
            .is_some();
        if was_dispatched {
            CancellationReport {
                request_id: handle.request_id,
                cancelled_before_dispatch: false,
                outcome_uncertain: true,
            }
        } else {
            CancellationReport {
                request_id: handle.request_id,
                cancelled_before_dispatch: true,
                outcome_uncertain: false,
            }
        }
    }
}
