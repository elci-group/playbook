//! DIY OTLP/HTTP+JSON exporter.
//!
//! The trace model comes from `playbook_observability`; this crate encodes it
//! as a spec-compliant `ExportTraceServiceRequest` JSON document and POSTs it
//! to an OTLP/HTTP collector with a hand-rolled HTTP/1.1 client. Wire span ids
//! are derived deterministically from the logical ids with SHA-256, so replays
//! produce identical telemetry. No opentelemetry dependency; plain http only.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use playbook_core::{sha256_hex, PlaybookError};
use playbook_observability::SpanNode;
use serde_json::{json, Value};

pub const SCOPE_NAME: &str = "playbook-observability";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtlpConfig {
    pub endpoint: String,
    pub service_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeWindow {
    pub start_unix_nano: u64,
    pub end_unix_nano: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportSummary {
    pub spans: usize,
    pub bytes_sent: usize,
    pub status: u16,
    pub partial_error: Option<String>,
    pub retries: u32,
}

/// The 16-byte OTLP trace id, derived from the logical drive id.
pub fn wire_trace_id(drive_id: &str) -> String {
    sha256_hex(format!("playbook-trace:{drive_id}").as_bytes())[..32].to_string()
}

/// The 8-byte OTLP span id, derived from the logical span id.
pub fn wire_span_id(logical_span_id: &str) -> String {
    sha256_hex(format!("playbook-span:{logical_span_id}").as_bytes())[..16].to_string()
}

fn span_value(node: &SpanNode, trace_id: &str, start: u64, end: u64) -> Value {
    let mut span = json!({
        "traceId": trace_id,
        "spanId": wire_span_id(&node.span_id),
        "name": node.name,
        "kind": "SPAN_KIND_INTERNAL",
        "startTimeUnixNano": start.to_string(),
        "endTimeUnixNano": end.to_string(),
    });
    if let Some(parent) = &node.parent_span_id {
        span["parentSpanId"] = json!(wire_span_id(parent));
    }
    let attributes: Vec<Value> = node
        .attributes
        .iter()
        .map(|(key, value)| json!({"key": key, "value": {"stringValue": value}}))
        .collect();
    if !attributes.is_empty() {
        span["attributes"] = json!(attributes);
    }
    match node.attributes.get("error") {
        Some(error) => span["status"] = json!({"code": "STATUS_CODE_ERROR", "message": error}),
        None => span["status"] = json!({"code": "STATUS_CODE_OK"}),
    }
    span
}

fn cut_points(window: TimeWindow, count: usize) -> Vec<u64> {
    let span_total = window.end_unix_nano.saturating_sub(window.start_unix_nano);
    let denominator = count.max(1) as u128;
    (0..=count)
        .map(|index| {
            window
                .start_unix_nano
                .saturating_add((span_total as u128 * index as u128 / denominator) as u64)
        })
        .collect()
}

/// Encode the spans as an OTLP/HTTP JSON `ExportTraceServiceRequest`.
pub fn encode_request(
    service_name: &str,
    spans: &[SpanNode],
    window: TimeWindow,
) -> Result<String, PlaybookError> {
    if spans.is_empty() {
        return Err(PlaybookError::Incomplete);
    }
    let trace_id = wire_trace_id(&spans[0].trace_id);
    let cuts = cut_points(window, spans.len());
    let wire: Vec<Value> = spans
        .iter()
        .enumerate()
        .map(|(index, node)| span_value(node, &trace_id, cuts[index], cuts[index + 1]))
        .collect();
    let document = json!({
        "resourceSpans": [{
            "resource": {
                "attributes": [
                    {"key": "service.name", "value": {"stringValue": service_name}},
                ],
            },
            "scopeSpans": [{
                "scope": {"name": SCOPE_NAME},
                "spans": wire,
            }],
        }],
    });
    serde_json::to_string(&document).map_err(PlaybookError::json)
}

struct Endpoint {
    host: String,
    port: u16,
    path: String,
}

fn parse_endpoint(endpoint: &str) -> Result<Endpoint, PlaybookError> {
    let Some(rest) = endpoint.strip_prefix("http://") else {
        return Err(PlaybookError::Io(format!(
            "the DIY exporter speaks plain http only: {endpoint}"
        )));
    };
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/v1/traces".to_string()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host.to_string(),
            port.parse()
                .map_err(|_| PlaybookError::Io(format!("invalid endpoint port: {endpoint}")))?,
        ),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return Err(PlaybookError::Io(format!(
            "endpoint has no host: {endpoint}"
        )));
    }
    Ok(Endpoint { host, port, path })
}

struct Response {
    status: u16,
    retry_after: Option<u64>,
    body: String,
}

fn parse_response(bytes: &[u8]) -> Result<Response, PlaybookError> {
    let text = String::from_utf8_lossy(bytes);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| PlaybookError::Io("collector response has no header block".into()))?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| PlaybookError::Io("collector response has no status line".into()))?;
    let retry_after = head.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("retry-after")
            .then(|| value.trim().parse::<u64>().ok())?
    });
    Ok(Response {
        status,
        retry_after,
        body: body.to_string(),
    })
}

fn post_once(endpoint: &Endpoint, body: &str) -> Result<Response, PlaybookError> {
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        endpoint.path,
        endpoint.host,
        endpoint.port,
        body.len(),
    );
    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| PlaybookError::Io(error.to_string()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| PlaybookError::Io(error.to_string()))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| PlaybookError::Io(error.to_string()))?;
    stream
        .shutdown(std::net::Shutdown::Write)
        .map_err(|error| PlaybookError::Io(error.to_string()))?;
    let mut bytes = Vec::new();
    stream
        .read_to_end(&mut bytes)
        .map_err(|error| PlaybookError::Io(error.to_string()))?;
    parse_response(&bytes)
}

fn partial_error(body: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    parsed["partialSuccess"]["errorMessage"]
        .as_str()
        .map(str::to_string)
}

/// Export the spans to the collector. Only transport failures are errors; the
/// summary carries the final HTTP status and any partial-success report.
pub fn export(
    config: &OtlpConfig,
    spans: &[SpanNode],
    window: TimeWindow,
) -> Result<ExportSummary, PlaybookError> {
    let body = encode_request(&config.service_name, spans, window)?;
    let endpoint = parse_endpoint(&config.endpoint)?;
    let mut response = post_once(&endpoint, &body)?;
    let mut retries = 0;
    if matches!(response.status, 429 | 503 | 504) {
        let wait = response.retry_after.unwrap_or(1).min(5);
        std::thread::sleep(Duration::from_secs(wait));
        response = post_once(&endpoint, &body)?;
        retries = 1;
    }
    Ok(ExportSummary {
        spans: spans.len(),
        bytes_sent: body.len(),
        status: response.status,
        partial_error: partial_error(&response.body),
        retries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::net::TcpListener;
    use std::thread;

    fn trace() -> Vec<SpanNode> {
        vec![
            SpanNode {
                trace_id: "drive-otlp".into(),
                span_id: "drive-otlp:drive".into(),
                parent_span_id: None,
                name: "drive".into(),
                attributes: BTreeMap::from([("drive_id".into(), "drive-otlp".into())]),
            },
            SpanNode {
                trace_id: "drive-otlp".into(),
                span_id: "drive-otlp:play:1".into(),
                parent_span_id: Some("drive-otlp:drive".into()),
                name: "compile".into(),
                attributes: BTreeMap::from([("down".into(), "SECOND_DOWN_GAIN".into())]),
            },
            SpanNode {
                trace_id: "drive-otlp".into(),
                span_id: "drive-otlp:tool:2".into(),
                parent_span_id: Some("drive-otlp:play:1".into()),
                name: "test.integration".into(),
                attributes: BTreeMap::from([
                    ("error".into(), "induced timeout".into()),
                    ("outcome".into(), "rejected".into()),
                ]),
            },
        ]
    }

    fn serve(listener: TcpListener, responses: &[&str]) -> Vec<Vec<u8>> {
        let mut requests = Vec::new();
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&buffer[..read]),
                }
            }
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            requests.push(request);
        }
        requests
    }

    fn ok_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len(),
        )
    }

    const WINDOW: TimeWindow = TimeWindow {
        start_unix_nano: 1_000,
        end_unix_nano: 9_000,
    };

    #[test]
    fn ids_are_deterministic_wire_shaped() {
        let trace_id = wire_trace_id("drive-otlp");
        let span_id = wire_span_id("drive-otlp:drive");
        assert_eq!(trace_id.len(), 32);
        assert_eq!(span_id.len(), 16);
        assert!(trace_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(span_id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(trace_id, wire_trace_id("drive-otlp"));
        assert_eq!(span_id, wire_span_id("drive-otlp:drive"));
        assert_ne!(trace_id, wire_trace_id("drive-other"));
        assert_ne!(span_id, wire_span_id("drive-otlp:play:1"));
    }

    #[test]
    fn encode_request_matches_the_otlp_json_contract() {
        let document: Value =
            serde_json::from_str(&encode_request("playbook", &trace(), WINDOW).unwrap()).unwrap();
        let resource = &document["resourceSpans"][0];
        assert_eq!(
            resource["resource"]["attributes"][0]["key"],
            json!("service.name")
        );
        assert_eq!(
            resource["resource"]["attributes"][0]["value"]["stringValue"],
            json!("playbook")
        );
        assert_eq!(
            resource["scopeSpans"][0]["scope"]["name"],
            json!(SCOPE_NAME)
        );
        let spans = resource["scopeSpans"][0]["spans"].as_array().unwrap();
        assert_eq!(spans.len(), 3);
        let trace_id = wire_trace_id("drive-otlp");
        let drive_span = spans[0]["spanId"].as_str().unwrap();
        let play_span = spans[1]["spanId"].as_str().unwrap();
        assert_eq!(spans[0]["traceId"].as_str().unwrap(), trace_id);
        assert_eq!(drive_span, wire_span_id("drive-otlp:drive"));
        assert_eq!(spans[1]["parentSpanId"].as_str().unwrap(), drive_span);
        assert_eq!(spans[2]["parentSpanId"].as_str().unwrap(), play_span);
        assert_eq!(spans[0]["kind"], json!("SPAN_KIND_INTERNAL"));
        let drive_start: u64 = spans[0]["startTimeUnixNano"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let drive_end: u64 = spans[0]["endTimeUnixNano"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let tool_end: u64 = spans[2]["endTimeUnixNano"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(drive_start, 1_000);
        assert_eq!(drive_end, 3_666);
        assert_eq!(tool_end, 9_000);
        assert_eq!(spans[2]["status"]["code"], json!("STATUS_CODE_ERROR"));
        assert_eq!(spans[2]["status"]["message"], json!("induced timeout"));
        assert_eq!(spans[0]["status"]["code"], json!("STATUS_CODE_OK"));
        let attributes = spans[2]["attributes"].as_array().unwrap();
        assert_eq!(attributes[0]["key"], json!("error"));
        assert_eq!(attributes[1]["key"], json!("outcome"));
        assert_eq!(attributes[1]["value"]["stringValue"], json!("rejected"));
    }

    #[test]
    fn empty_span_batch_is_rejected() {
        assert_eq!(
            encode_request("playbook", &[], WINDOW).unwrap_err(),
            PlaybookError::Incomplete
        );
    }

    #[test]
    fn export_posts_and_reads_a_collector_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || serve(listener, &[&ok_response("{}")]));
        let summary = export(
            &OtlpConfig {
                endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
                service_name: "playbook".into(),
            },
            &trace(),
            WINDOW,
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        let request = String::from_utf8(requests[0].clone()).unwrap();
        assert!(request.starts_with("POST /v1/traces HTTP/1.1"));
        assert!(request.contains("Content-Type: application/json"));
        assert!(request.contains(&format!("Host: 127.0.0.1:{port}")));
        assert_eq!(summary.status, 200);
        assert_eq!(summary.spans, 3);
        assert_eq!(summary.retries, 0);
        assert_eq!(summary.partial_error, None);
        assert!(summary.bytes_sent > 0);
    }

    #[test]
    fn partial_success_is_surfaced() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = r#"{"partialSuccess":{"errorMessage":"dropped 1 span"}}"#;
        let server = thread::spawn(move || serve(listener, &[&ok_response(body)]));
        let summary = export(
            &OtlpConfig {
                endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
                service_name: "playbook".into(),
            },
            &trace(),
            WINDOW,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(summary.status, 200);
        assert_eq!(summary.partial_error, Some("dropped 1 span".into()));
    }

    #[test]
    fn server_error_is_retried_once() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let responses = [
            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\n\r\n"
                .to_string(),
            ok_response("{}"),
        ];
        let server = thread::spawn(move || {
            let response_refs: Vec<&str> = responses.iter().map(String::as_str).collect();
            serve(listener, &response_refs)
        });
        let summary = export(
            &OtlpConfig {
                endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
                service_name: "playbook".into(),
            },
            &trace(),
            WINDOW,
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(summary.status, 200);
        assert_eq!(summary.retries, 1);
    }

    #[test]
    fn refused_connection_is_an_io_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let result = export(
            &OtlpConfig {
                endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
                service_name: "playbook".into(),
            },
            &trace(),
            WINDOW,
        )
        .unwrap_err();
        assert!(matches!(result, PlaybookError::Io(_)));
    }

    #[test]
    fn https_endpoint_is_refused() {
        let result = export(
            &OtlpConfig {
                endpoint: "https://collector:4318/v1/traces".into(),
                service_name: "playbook".into(),
            },
            &trace(),
            WINDOW,
        )
        .unwrap_err();
        match result {
            PlaybookError::Io(message) => assert!(message.contains("plain http")),
            other => panic!("expected Io error, got {other:?}"),
        }
    }
}
