use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use playbook_observability::SpanNode;
use playbook_otlp::{export, wire_span_id, wire_trace_id, OtlpConfig, TimeWindow};

#[tracing::instrument]
fn trace() -> Vec<SpanNode> {
    vec![
        SpanNode {
            trace_id: "drive-otlp-it".into(),
            span_id: "drive-otlp-it:drive".into(),
            parent_span_id: None,
            name: "drive".into(),
            attributes: BTreeMap::from([("drive_id".into(), "drive-otlp-it".into())]),
        },
        SpanNode {
            trace_id: "drive-otlp-it".into(),
            span_id: "drive-otlp-it:play:1".into(),
            parent_span_id: Some("drive-otlp-it:drive".into()),
            name: "pass".into(),
            attributes: BTreeMap::from([("down".into(), "second".into())]),
        },
        SpanNode {
            trace_id: "drive-otlp-it".into(),
            span_id: "drive-otlp-it:tool:2".into(),
            parent_span_id: Some("drive-otlp-it:play:1".into()),
            name: "throw".into(),
            attributes: BTreeMap::from([("error".into(), "intercepted".into())]),
        },
    ]
}

#[tracing::instrument]
fn serve_once(listener: TcpListener, body: &str) -> Vec<u8> {
    let (mut stream, _) = listener.accept().expect("accept");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("read timeout");
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => request.extend_from_slice(&buffer[..n]),
            Err(_) => break,
        }
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).expect("respond");
    stream.flush().expect("flush");
    request
}

#[test]
#[tracing::instrument]
fn the_collector_receives_a_valid_otlp_trace() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = thread::spawn(move || serve_once(listener, "{}"));

    let spans = trace();
    let summary = export(
        &OtlpConfig {
            endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
            service_name: "playbook-it".into(),
        },
        &spans,
        TimeWindow {
            start_unix_nano: 1_000,
            end_unix_nano: 9_000,
        },
    )
    .expect("export");

    let raw = server.join().expect("server thread");
    let request = String::from_utf8(raw).expect("utf8 request");
    assert!(request.starts_with("POST /v1/traces HTTP/1.1"));
    assert!(request.contains("Content-Type: application/json"));
    assert!(request.contains(&format!("Host: 127.0.0.1:{port}")));

    let (_, body) = request.split_once("\r\n\r\n").expect("header/body split");
    let document: serde_json::Value = serde_json::from_str(body).expect("json body");
    let wire_spans = &document["resourceSpans"][0]["scopeSpans"][0]["spans"];
    assert_eq!(wire_spans.as_array().expect("span array").len(), 3);

    let trace_id = wire_spans[0]["traceId"].as_str().expect("trace id");
    assert_eq!(trace_id, wire_trace_id("drive-otlp-it"));
    assert_eq!(trace_id.len(), 32);
    assert!(trace_id.chars().all(|c| c.is_ascii_hexdigit()));

    let drive_span_id = wire_spans[0]["spanId"].as_str().expect("drive span id");
    assert_eq!(drive_span_id, wire_span_id("drive-otlp-it:drive"));
    assert_eq!(drive_span_id.len(), 16);
    assert!(drive_span_id.chars().all(|c| c.is_ascii_hexdigit()));

    let play_span_id = wire_spans[1]["spanId"].as_str().expect("play span id");
    let tool_span_id = wire_spans[2]["spanId"].as_str().expect("tool span id");
    assert_eq!(drive_span_id.len(), 16);
    assert_eq!(
        wire_spans[1]["parentSpanId"].as_str().expect("play parent"),
        drive_span_id
    );
    assert_eq!(
        wire_spans[2]["parentSpanId"].as_str().expect("tool parent"),
        play_span_id
    );
    assert_eq!(tool_span_id.len(), 16);

    let drive_start: u64 = wire_spans[0]["startTimeUnixNano"]
        .as_str()
        .expect("drive start")
        .parse()
        .expect("u64 start");
    assert_eq!(drive_start, 1_000);
    let tool_end: u64 = wire_spans[2]["endTimeUnixNano"]
        .as_str()
        .expect("tool end")
        .parse()
        .expect("u64 end");
    assert_eq!(tool_end, 9_000);

    assert_eq!(summary.status, 200);
    assert_eq!(summary.spans, 3);
    assert_eq!(summary.partial_error, None);
    assert_eq!(summary.retries, 0);
}

#[test]
#[tracing::instrument]
fn a_failing_collector_reports_its_partial_success() {
    let body = r#"{"partialSuccess":{"errorMessage":"one span dropped"}}"#;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let server = thread::spawn(move || serve_once(listener, body));

    let summary = export(
        &OtlpConfig {
            endpoint: format!("http://127.0.0.1:{port}/v1/traces"),
            service_name: "playbook-it".into(),
        },
        &trace(),
        TimeWindow {
            start_unix_nano: 1_000,
            end_unix_nano: 9_000,
        },
    )
    .expect("export");

    server.join().expect("server thread");
    assert_eq!(summary.status, 200);
    assert_eq!(summary.partial_error, Some("one span dropped".into()));
}
