//! Socket-level tests for the continuous drive service.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use playbook_core::{DriveId, EvidenceClass, Gate, GateId, TaskContract, TaskIntegrity};
use playbook_elci::FixturePlane;
use playbook_events::DriveEvent;
use playbook_journal::Journal;
use playbook_policy::Policy;
use playbook_server::{bind, ServerConfig};
use playbook_state::{started_budget, Authority, EventSink};

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("playbook-server-it-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn spawn_server(journal_dir: PathBuf) -> SocketAddr {
    let config = ServerConfig {
        port: 0,
        journal_dir,
        ..ServerConfig::default()
    };
    let bound = bind(config).expect("bind");
    let address = bound.local_addr();
    thread::spawn(move || bound.run().expect("serve"));
    thread::sleep(Duration::from_millis(100));
    address
}

fn request(address: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(address).expect("connect");
    stream.write_all(raw.as_bytes()).expect("write");
    let mut response = String::new();
    stream.read_to_string(&mut response).expect("read");
    response
}

fn call(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let raw = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let response = request(address, &raw);
    let status: u16 = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

fn json_call(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    call(address, method, path, body)
}

fn post(address: SocketAddr, path: &str, body: &str) -> (u16, String) {
    json_call(address, "POST", path, body)
}

fn seed_crashed_journal(path: &Path) -> String {
    let policy = Policy::builtin();
    let mut authority = Authority::start(
        DriveId::new("drive-crash").unwrap(),
        TaskContract {
            statement: "crash over the api".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        },
        started_budget(1_000, 1_000),
        policy.clone(),
        playbook_sim::roster(),
        1_000,
    )
    .unwrap();
    authority
        .declare_gate(
            Gate::declare(
                GateId::new("implementation").unwrap(),
                "compiler accepts the implementation",
                1_000_000,
                Vec::new(),
                vec![EvidenceClass::CompilerResult],
                true,
            )
            .unwrap(),
        )
        .unwrap();
    let mut plane = FixturePlane::mvp();
    authority.preflight(&mut plane).unwrap();
    authority.step(&mut plane, 1_100).unwrap();
    let log = authority.log().to_vec();
    let invoke_end = log
        .iter()
        .position(|event| matches!(event, DriveEvent::ToolInvoked { .. }))
        .unwrap()
        + 1;
    let key = match &log[invoke_end - 1] {
        DriveEvent::ToolInvoked { execution } => execution.idempotency_key.clone().unwrap(),
        _ => unreachable!(),
    };
    let mut journal = Journal::create(path).unwrap();
    for event in &log[..invoke_end] {
        EventSink::record(&mut journal, event).unwrap();
    }
    drop(journal);
    key
}

#[test]
fn restart_replays_every_drive_into_the_same_snapshot() {
    let dir = temp_dir("boot");
    let address = spawn_server(dir.clone());
    let (status, _) = post(
        address,
        "/v1/drives",
        r#"{"drive_id":"drive-boot","statement":"boot check"}"#,
    );
    assert_eq!(status, 201);
    assert_eq!(post(address, "/v1/drives/drive-boot/preflight", "").0, 200);
    assert_eq!(post(address, "/v1/drives/drive-boot/steps", "").0, 200);
    assert_eq!(post(address, "/v1/drives/drive-boot/steps", "").0, 200);
    let (status, before) = call(address, "GET", "/v1/drives/drive-boot", "");
    assert_eq!(status, 200);

    let address = spawn_server(dir);
    let (status, after) = call(address, "GET", "/v1/drives/drive-boot", "");
    assert_eq!(status, 200);
    assert_eq!(before, after);
    std::fs::remove_dir_all(temp_dir("boot")).ok();
}

#[test]
fn lifecycle_advances_over_a_real_socket() {
    let dir = temp_dir("life");
    let address = spawn_server(dir.clone());
    let (status, _) = post(
        address,
        "/v1/drives",
        r#"{"drive_id":"drive-life","statement":"life check"}"#,
    );
    assert_eq!(status, 201);
    assert_eq!(post(address, "/v1/drives/drive-life/preflight", "").0, 200);
    for _ in 0..3 {
        let (status, body) = post(address, "/v1/drives/drive-life/steps", "");
        assert_eq!(status, 200, "{body}");
        assert!(body.contains("\"snapshot\""));
        assert!(body.contains("\"decision\""));
    }
    let (status, events) = call(address, "GET", "/v1/drives/drive-life/events", "");
    assert_eq!(status, 200);
    let log: Vec<serde_json::Value> = serde_json::from_str(&events).unwrap();
    assert!(log
        .iter()
        .any(|event| event["type"] == serde_json::json!("tool_invoked")));
    let recovery = Journal::open(&dir.join("drive-life.jsonl")).unwrap();
    assert_eq!(recovery.events.len(), log.len());
    std::fs::remove_dir_all(temp_dir("life")).ok();
}

#[test]
fn a_crashed_invocation_reconciles_over_the_api() {
    let dir = temp_dir("crash");
    std::fs::create_dir_all(&dir).unwrap();
    let key = seed_crashed_journal(&dir.join("drive-crash.jsonl"));
    let address = spawn_server(dir);
    let (status, snapshot) = call(address, "GET", "/v1/drives/drive-crash", "");
    assert_eq!(status, 200);
    assert!(snapshot.contains("active_capability"));
    assert!(!snapshot.contains("\"active_capability\":null"));
    let (status, refused) = post(address, "/v1/drives/drive-crash/steps", "");
    assert_eq!(status, 409, "{refused}");
    assert!(refused.contains("reconcile_required"));
    let (status, _) = post(
        address,
        "/v1/drives/drive-crash/reconcile",
        &serde_json::json!({"key": key, "occurred": false}).to_string(),
    );
    assert_eq!(status, 200);
    let (status, _) = post(address, "/v1/drives/drive-crash/steps", "");
    assert_ne!(status, 409);
    std::fs::remove_dir_all(temp_dir("crash")).ok();
}

#[test]
fn a_drive_reaches_fourth_down_pressure_punt_and_refusal() {
    let dir = temp_dir("fourth");
    let address = spawn_server(dir.clone());
    let (status, _) = post(
        address,
        "/v1/drives",
        r#"{"drive_id":"drive-fourth","statement":"fourth down"}"#,
    );
    assert_eq!(status, 201);
    assert_eq!(
        post(address, "/v1/drives/drive-fourth/preflight", "").0,
        200
    );
    for _ in 0..4 {
        let (status, body) = post(address, "/v1/drives/drive-fourth/steps", "");
        assert_eq!(status, 200, "{body}");
    }
    let (status, body) = post(address, "/v1/drives/drive-fourth/steps", "");
    assert_eq!(status, 200, "{body}");
    let (status, body) = post(
        address,
        "/v1/drives/drive-fourth/pressure",
        r#"{"provider_quota":900}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"fourth_down\""), "{body}");
    let (status, body) = post(address, "/v1/drives/drive-fourth/punt", "");
    assert_eq!(status, 200, "{body}");
    let (status, refused) = post(address, "/v1/drives/drive-fourth/complete", "");
    assert_eq!(status, 200, "{refused}");
    assert!(refused.contains("\"status\":\"punted\""), "{refused}");

    let (status, events) = call(address, "GET", "/v1/drives/drive-fourth/events", "");
    assert_eq!(status, 200);
    let log: Vec<serde_json::Value> = serde_json::from_str(&events).unwrap();
    assert!(log
        .iter()
        .any(|event| event["type"] == serde_json::json!("punt_issued")));
    let state = playbook_state::fold(
        &serde_json::from_value::<Vec<playbook_sim::DriveEvent>>(
            serde_json::from_str(&events).unwrap(),
        )
        .unwrap(),
        &Policy::builtin(),
        playbook_state::ApplyMode::Audit,
    )
    .unwrap();
    assert!(state.package().is_some());
    assert_eq!(state.down(), playbook_sim::Down::FourthDown);
    std::fs::remove_dir_all(temp_dir("fourth")).ok();
}

#[test]
fn a_pass_heavy_drive_recovers_with_its_own_policy() {
    let dir = temp_dir("policy");
    let address = spawn_server(dir.clone());
    let (status, _) = post(
        address,
        "/v1/drives",
        r#"{"drive_id":"drive-ph","statement":"contrast","policy":"pass-heavy"}"#,
    );
    assert_eq!(status, 201);
    assert_eq!(post(address, "/v1/drives/drive-ph/preflight", "").0, 200);
    assert_eq!(post(address, "/v1/drives/drive-ph/steps", "").0, 200);
    let (_, before) = call(address, "GET", "/v1/drives/drive-ph", "");
    assert!(dir.join("drive-ph.jsonl.policy").exists());

    let address = spawn_server(dir);
    let (status, after) = call(address, "GET", "/v1/drives/drive-ph", "");
    assert_eq!(status, 200);
    assert_eq!(before, after);
    let (_, ready) = call(address, "GET", "/readyz", "");
    assert!(ready.contains("\"rejected_journals\":0"), "{ready}");
    std::fs::remove_dir_all(temp_dir("policy")).ok();
}

#[test]
fn the_error_contract_and_limits_hold() {
    let dir = temp_dir("errors");
    let address = spawn_server(dir);
    let (status, body) = call(address, "GET", "/v1/drives/nope", "");
    assert_eq!(status, 404);
    assert!(body.contains("\"error\""));
    let (status, body) = post(address, "/v1/drives", "{bad");
    assert_eq!(status, 400);
    assert!(body.contains("bad_json"));
    let (status, body) = call(address, "GET", "/healthz", "");
    assert_eq!(status, 200);
    assert!(body.contains("\"ok\":true"));
    let (status, body) = call(address, "GET", "/readyz", "");
    assert_eq!(status, 200);
    assert!(body.contains("\"drives\""));

    let oversized = "a".repeat(1_048_577);
    let raw = format!(
        "POST /v1/drives HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{oversized}",
        oversized.len()
    );
    let response = request(address, &raw);
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
    std::fs::remove_dir_all(temp_dir("errors")).ok();
}
