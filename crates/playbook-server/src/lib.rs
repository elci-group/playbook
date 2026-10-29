//! Continuous HTTP drive service.
//!
//! The server owns drives, dispatches plays against a fixture capability
//! plane, and journals every event before the response is sent. Boot is
//! recovery: journals on disk are folded back into live drives, and a drive
//! killed mid-invocation reappears with its open execution until it is
//! reconciled. The contract of record is `docs/server-directive.md`.

#![forbid(unsafe_code)]

mod franchise;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use playbook_core::{
    DriveBudget, DriveId, DriveStatus, PlaybookError, TaskContract, TaskIntegrity,
};
use playbook_elci::FixturePlane;
use playbook_events::DriveEvent;
use playbook_journal::Journal;
use playbook_observability::SpanNode;
use playbook_policy::Policy;
use playbook_state::{started_budget, ApplyMode, Authority};
use serde_json::{json, Value};

const MAX_DRIVES: usize = 256;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const CLOCK_STEP_MS: u64 = 100;

pub struct ServerConfig {
    pub bind: String,
    pub port: u16,
    pub journal_dir: PathBuf,
    pub otlp: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".into(),
            port: 8088,
            journal_dir: PathBuf::from("./playbook-journals"),
            otlp: None,
        }
    }
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

fn reply(status: u16, body: &Value) -> Response {
    Response {
        status,
        body: body.to_string(),
    }
}

fn error_reply(status: u16, code: &str, detail: &str) -> Response {
    reply(status, &json!({"error": code, "detail": detail}))
}

fn error_of(err: PlaybookError) -> Response {
    let code = match &err {
        PlaybookError::Reconcile(_) => "reconcile_required",
        PlaybookError::BlindRetry(_) => "blind_retry",
        PlaybookError::PuntNotReady => "punt_not_ready",
        PlaybookError::Incomplete => "incomplete",
        PlaybookError::Closed => "closed",
        PlaybookError::NoPlay => "no_play",
        PlaybookError::Inadmissible(_, _) => "inadmissible",
        PlaybookError::StaleCapability => "stale_capability",
        PlaybookError::BudgetExhausted
        | PlaybookError::BudgetCeiling
        | PlaybookError::BudgetIncrease => "budget",
        PlaybookError::DuplicateEffect(_) => "duplicate_effect",
        PlaybookError::UnknownGate(_) => "unknown_gate",
        PlaybookError::Json(_) => "bad_json",
        PlaybookError::Evidence(_) => "bad_request",
        PlaybookError::InvalidId
        | PlaybookError::Probability(_)
        | PlaybookError::Schema(_)
        | PlaybookError::WaiverAuthority => "bad_request",
        PlaybookError::Io(_) => "io",
        PlaybookError::DigestMismatch
        | PlaybookError::AuthorizationStop(_)
        | PlaybookError::SideEffect(_)
        | PlaybookError::ReplayDivergence(_)
        | PlaybookError::DispositionMismatch
        | PlaybookError::SwarmUnjustified
        | PlaybookError::PreflightRequired
        | PlaybookError::AmnesiaLeak => "refused",
    };
    error_reply(error_status(code), code, &err.to_string())
}

fn cleanup_journal(path: &Path, sidecar: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(sidecar);
}

fn error_status(code: &str) -> u16 {
    match code {
        "io" => 500,
        "internal" => 500,
        "bad_request" | "bad_json" | "unknown_gate" => 400,
        _ => 409,
    }
}

struct DriveHandle {
    authority: Authority,
    plane: FixturePlane,
    otlp_exported: bool,
}

pub struct Service {
    franchises: franchise::FranchiseApi,
    journal_dir: PathBuf,
    otlp: Option<String>,
    drives: Mutex<BTreeMap<String, DriveHandle>>,
    rejected_journals: Mutex<usize>,
}

/// The newest logical clock reading visible in the log. `ToolInvoked` carries
/// its dispatch time in `execution.timeout_ms`; outcome events inherit the
/// same reading through their inner play records.
fn last_at_ms(events: &[DriveEvent]) -> u64 {
    events
        .iter()
        .rev()
        .map(|event| match event {
            DriveEvent::DriveStarted { at_ms, .. }
            | DriveEvent::BudgetConsumed { at_ms, .. }
            | DriveEvent::PossessionStarted { at_ms, .. }
            | DriveEvent::CompletionRejected { at_ms, .. }
            | DriveEvent::DriveCompleted { at_ms } => *at_ms,
            DriveEvent::ToolInvoked { execution } => execution.timeout_ms,
            _ => 0,
        })
        .find(|at_ms| *at_ms > 0)
        .unwrap_or(1_000)
}

fn spans_value(spans: &[SpanNode]) -> Value {
    Value::Array(
        spans
            .iter()
            .map(|span| {
                json!({
                    "trace_id": span.trace_id,
                    "span_id": span.span_id,
                    "parent_span_id": span.parent_span_id,
                    "name": span.name,
                    "attributes": span.attributes,
                })
            })
            .collect(),
    )
}

impl Service {
    pub fn open(config: &ServerConfig) -> Result<Service, PlaybookError> {
        std::fs::create_dir_all(&config.journal_dir)
            .map_err(|error| PlaybookError::Io(error.to_string()))?;
        let mut drives = BTreeMap::new();
        let mut rejected = 0usize;
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&config.journal_dir)
            .map_err(|error| PlaybookError::Io(error.to_string()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "jsonl")
            })
            .collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            let policy = Service::policy_for(&path);
            match Service::recover_drive(&path, &policy) {
                Ok(handle) => {
                    drives.insert(name, handle);
                }
                Err(_) => {
                    let quarantined = path.with_extension("jsonl.rejected");
                    match std::fs::rename(&path, quarantined) {
                        Ok(()) => rejected += 1,
                        Err(error) => eprintln!("quarantine of {} failed: {error}", path.display()),
                    }
                }
            }
        }
        Ok(Service {
            franchises: franchise::FranchiseApi::open(&config.journal_dir.join("franchises"))?,
            journal_dir: config.journal_dir.clone(),
            otlp: config.otlp.clone(),
            drives: Mutex::new(drives),
            rejected_journals: Mutex::new(rejected),
        })
    }
    /// The policy a journal folds under: the sidecar records the choice at
    /// creation; the fold test breaks ties for journals without one.
    fn policy_for(path: &Path) -> Policy {
        let sidecar = path.with_extension("jsonl.policy");
        if let Ok(text) = std::fs::read_to_string(&sidecar) {
            match text.trim() {
                "pass-heavy" => return Policy::pass_heavy_contrast(),
                "builtin" => return Policy::builtin(),
                _ => {}
            }
        }
        let events = match Journal::open(path) {
            Ok(recovery) => recovery.events,
            Err(_) => return Policy::builtin(),
        };
        if playbook_state::fold(&events, &Policy::builtin(), ApplyMode::Audit).is_ok() {
            Policy::builtin()
        } else {
            Policy::pass_heavy_contrast()
        }
    }

    fn recover_drive(path: &Path, policy: &Policy) -> Result<DriveHandle, PlaybookError> {
        let recovery = Journal::open(path)?;
        playbook_state::fold(&recovery.events, policy, ApplyMode::Audit)?;
        let mut authority =
            Authority::resume(recovery.events, policy.clone(), playbook_sim::roster())?;
        authority.attach_journal(Box::new(recovery.journal));
        let mut plane = FixturePlane::mvp();
        plane.resume_generation(authority.state().capability_generation());
        Ok(DriveHandle {
            authority,
            plane,
            otlp_exported: false,
        })
    }

    pub fn rejected_journals(&self) -> usize {
        *self.rejected_journals.lock().expect("rejected lock")
    }

    fn create_drive(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(error) => return error_of(PlaybookError::json(error)),
        };
        let Some(raw_id) = parsed["drive_id"].as_str() else {
            return error_reply(400, "bad_request", "drive_id is required");
        };
        let Ok(drive_id) = DriveId::new(raw_id) else {
            return error_reply(400, "bad_request", "drive_id is not a valid identifier");
        };
        let statement = parsed["statement"]
            .as_str()
            .unwrap_or("unspecified task")
            .to_string();
        let workload_class = parsed["workload_class"]
            .as_str()
            .unwrap_or("engineering")
            .to_string();
        let tokens = parsed["tokens"].as_u64().unwrap_or(1_000);
        let quota = parsed["quota"].as_u64().unwrap_or(1_000);
        let policy_name = parsed["policy"].as_str().unwrap_or("builtin");
        let policy = match policy_name {
            "builtin" => Policy::builtin(),
            "pass-heavy" => Policy::pass_heavy_contrast(),
            other => return error_reply(400, "bad_request", &format!("unknown policy {other}")),
        };
        let path = self.journal_dir.join(format!("{raw_id}.jsonl"));
        let sidecar = self.journal_dir.join(format!("{raw_id}.jsonl.policy"));
        let mut drives = self.drives.lock().expect("drive lock");
        if drives.contains_key(raw_id) || path.exists() {
            return error_reply(409, "duplicate_drive", "drive id already exists");
        }
        if drives.len() >= MAX_DRIVES {
            return error_reply(503, "drive_cap", "the drive cap is reached");
        }
        let journal = match Journal::create_new(&path) {
            Ok(journal) => journal,
            Err(err) => {
                if path.exists() {
                    return error_reply(409, "duplicate_drive", "drive id already exists");
                }
                return error_of(err);
            }
        };
        let task = TaskContract {
            statement,
            workload_class,
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        };
        let gates = match playbook_sim::standard_gates() {
            Ok(gates) => gates,
            Err(err) => {
                cleanup_journal(&path, &sidecar);
                return error_of(err);
            }
        };
        let authority = Authority::start_journaled(
            drive_id,
            task,
            started_budget(tokens, quota),
            policy,
            playbook_sim::roster(),
            1_000,
            Box::new(journal),
        );
        let mut authority = match authority {
            Ok(authority) => authority,
            Err(err) => {
                cleanup_journal(&path, &sidecar);
                return error_of(err);
            }
        };
        for gate in gates {
            if let Err(err) = authority.declare_gate(gate) {
                cleanup_journal(&path, &sidecar);
                return error_of(err);
            }
        }
        if let Err(error) = std::fs::write(&sidecar, format!("{policy_name}\n")) {
            eprintln!("policy sidecar for {raw_id} failed: {error}");
        }
        let snapshot = authority.state().snapshot(authority.policy());
        drives.insert(
            raw_id.to_string(),
            DriveHandle {
                authority,
                plane: FixturePlane::mvp(),
                otlp_exported: false,
            },
        );
        reply(201, &json!({"snapshot": snapshot}))
    }

    fn snapshot_of(handle: &DriveHandle) -> Value {
        json!(handle.authority.state().snapshot(handle.authority.policy()))
    }

    fn mutate<F>(&self, id: &str, body: &str, apply: F) -> Response
    where
        F: FnOnce(&mut DriveHandle, u64) -> Result<Option<Value>, PlaybookError>,
    {
        let parsed: Value = if body.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str(body) {
                Ok(value) => value,
                Err(error) => return error_of(PlaybookError::json(error)),
            }
        };
        let export_job = {
            let mut drives = self.drives.lock().expect("drive lock");
            let Some(handle) = drives.get_mut(id) else {
                return error_reply(404, "unknown_drive", "no such drive");
            };
            let at_ms = parsed["at_ms"].as_u64().unwrap_or_else(|| {
                last_at_ms(handle.authority.log()).saturating_add(CLOCK_STEP_MS)
            });
            let decision = match apply(handle, at_ms) {
                Ok(decision) => decision,
                Err(err) => return error_of(err),
            };
            let mut payload = json!({"snapshot": Service::snapshot_of(handle)});
            if let Some(decision) = decision {
                payload["decision"] = decision;
            }
            let export = Service::export_job(handle, id, &self.otlp);
            (payload, export)
        };
        let (payload, export) = export_job;
        if let Some((endpoint, id, spans, window)) = export {
            let summary = playbook_otlp::export(
                &playbook_otlp::OtlpConfig {
                    endpoint,
                    service_name: "playbook".into(),
                },
                &spans,
                window,
            );
            match summary {
                Ok(summary) => println!("otlp export {id} status {}", summary.status),
                Err(err) => println!("otlp export {id} failed {err}"),
            }
        }
        reply(200, &payload)
    }

    /// Mark a terminal drive as exported and return the network job, so the
    /// blocking OTLP POST runs without the registry lock held.
    fn export_job(
        handle: &mut DriveHandle,
        id: &str,
        otlp: &Option<String>,
    ) -> Option<(String, String, Vec<SpanNode>, playbook_otlp::TimeWindow)> {
        if handle.otlp_exported {
            return None;
        }
        let endpoint = otlp.clone()?;
        if !matches!(
            handle.authority.state().status(),
            DriveStatus::Completed | DriveStatus::Abandoned
        ) {
            return None;
        }
        handle.otlp_exported = true;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0);
        let window = playbook_otlp::TimeWindow {
            start_unix_nano: now,
            end_unix_nano: now.saturating_add(handle.authority.log().len() as u64 * 1_000_000),
        };
        Some((
            endpoint,
            id.to_string(),
            playbook_observability::spans(id, handle.authority.log()),
            window,
        ))
    }

    pub fn handle(&self, method: &str, path: &str, body: &str) -> Response {
        let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
        if method == "POST" && segments == ["v1", "front-office", "rosters"] {
            return match serde_json::from_str::<playbook_front_office::PortfolioInput>(body)
                .map_err(PlaybookError::json)
                .and_then(|input| playbook_front_office::construct(&input))
            {
                Ok(contract) => reply(200, &json!(contract)),
                Err(error) => error_of(error),
            };
        }
        if segments.get(1) == Some(&"franchises") && segments.first() == Some(&"v1") {
            return self.franchises.handle(method, &segments[2..], body);
        }
        match (method, segments.as_slice()) {
            ("GET", ["healthz"]) => reply(200, &json!({"ok": true})),
            ("GET", ["readyz"]) => reply(
                200,
                &json!({
                    "ok": true,
                    "drives": self.drives.lock().expect("drive lock").len(),
                    "rejected_journals": self.rejected_journals(),
                }),
            ),
            ("GET", ["v1", "drives"]) => {
                let drives = self.drives.lock().expect("drive lock");
                let ids: Vec<&String> = drives.keys().collect();
                reply(200, &json!({ "drives": ids }))
            }
            ("POST", ["v1", "drives"]) => self.create_drive(body),
            ("GET", ["v1", "drives", id]) => match self.snapshot_json(id) {
                Some(snapshot) => reply(200, &snapshot),
                None => error_reply(404, "unknown_drive", "no such drive"),
            },
            ("GET", ["v1", "drives", id, "events"]) => match self.events_json(id) {
                Some(events) => reply(200, &json!(events)),
                None => error_reply(404, "unknown_drive", "no such drive"),
            },
            ("GET", ["v1", "drives", id, "trace"]) => match self.trace_json(id) {
                Some(spans) => reply(200, &json!({"spans": spans})),
                None => error_reply(404, "unknown_drive", "no such drive"),
            },
            ("GET", ["v1", "drives", id, "huddles"]) => match self.huddles_json(id) {
                Some(huddles) => reply(200, &huddles),
                None => error_reply(404, "unknown_drive", "no such drive"),
            },
            ("GET", ["v1", "drives", id, "huddles", huddle_id]) => {
                match self.huddle_json(id, huddle_id) {
                    Some(huddle) => reply(200, &huddle),
                    None => error_reply(404, "unknown_huddle", "no such huddle"),
                }
            }
            ("POST", ["v1", "drives", id, "huddles", "request"]) => {
                self.mutate(id, body, |handle, at_ms| {
                    let request: playbook_huddle::HuddleRequestBody =
                        match serde_json::from_str(body) {
                            Ok(request) => request,
                            Err(error) => return Err(PlaybookError::json(error)),
                        };
                    match handle.authority.open_huddle(
                        playbook_state::HuddleRequest::from_body(
                            request,
                            handle.authority.roster(),
                        ),
                        at_ms,
                    )? {
                        playbook_state::HuddleAdmission::Open(huddle_id) => Ok(Some(json!({
                            "admission": "open",
                            "huddle_id": huddle_id.as_str(),
                        }))),
                        playbook_state::HuddleAdmission::Refused {
                            huddle_id,
                            reason_code,
                        } => Ok(Some(json!({
                            "admission": "refused",
                            "huddle_id": huddle_id.as_str(),
                            "reason_code": reason_code,
                        }))),
                    }
                })
            }
            ("POST", ["v1", "drives", id, "huddles", huddle_id, action]) => match *action {
                "reports" => self.mutate(id, body, |handle, _| {
                    let Ok(huddle_id) = playbook_core::HuddleId::new(*huddle_id) else {
                        return Err(PlaybookError::InvalidId);
                    };
                    let report: playbook_huddle::HuddleReport = match serde_json::from_str(body) {
                        Ok(report) => report,
                        Err(error) => return Err(PlaybookError::json(error)),
                    };
                    let detected = handle.authority.submit_huddle_report(&huddle_id, report)?;
                    Ok(Some(json!({
                        "huddle_id": huddle_id.as_str(),
                        "contradictions_detected": detected,
                    })))
                }),
                "commit" => self.mutate(id, body, |handle, _| {
                    let Ok(huddle_id) = playbook_core::HuddleId::new(*huddle_id) else {
                        return Err(PlaybookError::InvalidId);
                    };
                    let outcome: playbook_huddle::HuddleOutcome = match serde_json::from_str(body) {
                        Ok(outcome) => outcome,
                        Err(error) => return Err(PlaybookError::json(error)),
                    };
                    handle
                        .authority
                        .commit_huddle_outcome(&huddle_id, outcome)?;
                    Ok(Some(
                        json!({"huddle_id": huddle_id.as_str(), "committed": true}),
                    ))
                }),
                "escalate" => self.mutate(id, body, |handle, _| {
                    let Ok(huddle_id) = playbook_core::HuddleId::new(*huddle_id) else {
                        return Err(PlaybookError::InvalidId);
                    };
                    let parsed: Value = match serde_json::from_str(body) {
                        Ok(value) => value,
                        Err(error) => return Err(PlaybookError::json(error)),
                    };
                    let child = handle.authority.escalate_huddle(
                        &huddle_id,
                        parsed["manual"].as_bool().unwrap_or(false),
                        parsed["reason_code"].as_str().unwrap_or("escalated"),
                    )?;
                    Ok(Some(json!({
                        "huddle_id": huddle_id.as_str(),
                        "child_huddle_id": child.as_str(),
                    })))
                }),
                _ => error_reply(404, "unknown_action", "no such action"),
            },
            ("POST", ["v1", "drives", _id, "huddles", ..]) => {
                error_reply(404, "unknown_action", "no such action")
            }
            ("POST", ["v1", "drives", id, action]) => match *action {
                "preflight" => self.mutate(id, body, |handle, _| {
                    handle.authority.preflight(&mut handle.plane)?;
                    Ok(None)
                }),
                "steps" => self.mutate(id, body, |handle, at_ms| {
                    let decision = handle.authority.step(&mut handle.plane, at_ms)?;
                    Ok(Some(json!(decision)))
                }),
                "pressure" => self.mutate(id, body, |handle, at_ms| {
                    let parsed: Value = serde_json::from_str(body).unwrap_or(json!({}));
                    let cost = DriveBudget {
                        provider_quota: parsed["provider_quota"].as_u64().unwrap_or(0),
                        ..DriveBudget::zero()
                    };
                    handle.authority.pressure(cost, at_ms)?;
                    Ok(None)
                }),
                "punt" => self.mutate(id, body, |handle, _| {
                    handle.authority.punt()?;
                    Ok(None)
                }),
                "reconcile" => self.mutate(id, body, |handle, _| {
                    let parsed: Value = match serde_json::from_str(body) {
                        Ok(value) => value,
                        Err(error) => return Err(PlaybookError::json(error)),
                    };
                    let Some(key) = parsed["key"].as_str() else {
                        return Err(PlaybookError::Evidence("key is required".into()));
                    };
                    handle
                        .authority
                        .reconcile(key, parsed["occurred"].as_bool().unwrap_or(false))?;
                    Ok(None)
                }),
                "complete" => self.mutate(id, body, |handle, at_ms| {
                    handle.authority.claim_complete(at_ms)?;
                    Ok(None)
                }),
                _ => error_reply(404, "unknown_action", "no such action"),
            },
            (_, ["v1", "drives", ..]) => {
                error_reply(405, "method_not_allowed", "method is not allowed")
            }
            _ => error_reply(404, "not_found", "no such route"),
        }
    }

    fn snapshot_json(&self, id: &str) -> Option<Value> {
        let drives = self.drives.lock().expect("drive lock");
        drives.get(id).map(Service::snapshot_of)
    }

    fn events_json(&self, id: &str) -> Option<Vec<DriveEvent>> {
        let drives = self.drives.lock().expect("drive lock");
        drives.get(id).map(|handle| handle.authority.log().to_vec())
    }

    fn trace_json(&self, id: &str) -> Option<Vec<Value>> {
        let drives = self.drives.lock().expect("drive lock");
        drives.get(id).map(|handle| {
            match spans_value(&playbook_observability::spans(id, handle.authority.log())) {
                Value::Array(spans) => spans,
                _ => Vec::new(),
            }
        })
    }

    fn huddles_json(&self, id: &str) -> Option<Value> {
        let drives = self.drives.lock().expect("drive lock");
        drives.get(id).map(|handle| {
            serde_json::to_value(handle.authority.state().huddles()).unwrap_or(Value::Null)
        })
    }

    fn huddle_json(&self, id: &str, huddle_id: &str) -> Option<Value> {
        let drives = self.drives.lock().expect("drive lock");
        let handle = drives.get(id)?;
        let ledger = handle.authority.state().huddles();
        let needle = playbook_core::HuddleId::new(huddle_id).ok()?;
        if let Some(active) = ledger.active_by_id(&needle) {
            return serde_json::to_value(active).ok();
        }
        ledger
            .closed()
            .iter()
            .find(|record| record.id == needle)
            .and_then(|record| serde_json::to_value(record).ok())
    }
}

pub struct Bound {
    listener: TcpListener,
    service: Arc<Service>,
    stop: Arc<AtomicBool>,
    live: Arc<AtomicUsize>,
}

pub fn bind(config: ServerConfig) -> std::io::Result<Bound> {
    let listener = TcpListener::bind((config.bind.as_str(), config.port))?;
    let address = listener.local_addr()?;
    let service = Arc::new(
        Service::open(&config)
            .map_err(|error| std::io::Error::other(format!("service open failed: {error}")))?,
    );
    println!("listening http://{address}");
    Ok(Bound {
        listener,
        service,
        stop: Arc::new(AtomicBool::new(false)),
        live: Arc::new(AtomicUsize::new(0)),
    })
}

impl Bound {
    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().expect("bound listener")
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    pub fn run(self) -> std::io::Result<()> {
        let Bound {
            listener,
            service,
            stop,
            live,
        } = self;
        listener.set_nonblocking(true)?;
        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let service = Arc::clone(&service);
                    let live = Arc::clone(&live);
                    thread::spawn(move || handle_connection(stream, &service, live));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => return Err(error),
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while live.load(Ordering::Acquire) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        Ok(())
    }
}

struct Parsed {
    method: String,
    path: String,
    body: String,
}

fn parse_request(raw: &[u8]) -> Result<Parsed, Response> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| error_reply(400, "bad_request", "no header terminator"))?;
    let head = String::from_utf8_lossy(&raw[..split]).to_string();
    let body_bytes = &raw[split + 4..];
    let mut lines = head.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| error_reply(400, "bad_request", "empty request"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| error_reply(400, "bad_request", "missing method"))?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| error_reply(400, "bad_request", "missing path"))?
        .to_string();
    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value
                    .trim()
                    .parse()
                    .map_err(|_| error_reply(400, "bad_request", "bad content-length"))?;
            }
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err(error_reply(413, "body_too_large", "body exceeds 1 MiB"));
    }
    if body_bytes.len() < content_length {
        return Err(error_reply(400, "bad_request", "truncated body"));
    }
    Ok(Parsed {
        method,
        path,
        body: String::from_utf8_lossy(&body_bytes[..content_length]).into_owned(),
    })
}

fn header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn content_length(head: &[u8]) -> Result<usize, Response> {
    let text = String::from_utf8_lossy(head);
    let mut declared: Option<usize> = None;
    for line in text.lines().skip(1) {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                let parsed = value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| error_reply(400, "bad_request", "bad content-length"))?;
                if let Some(previous) = declared {
                    if previous != parsed {
                        return Err(error_reply(
                            400,
                            "bad_request",
                            "conflicting content-length headers",
                        ));
                    }
                }
                declared = Some(parsed);
            }
        }
    }
    Ok(declared.unwrap_or(0))
}

fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, Response> {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let deadline = Instant::now() + READ_TIMEOUT;
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        if Instant::now() > deadline {
            return Err(error_reply(408, "timeout", "request timed out"));
        }
        if let Some(end) = header_end(&raw) {
            if end > MAX_HEADER_BYTES {
                return Err(error_reply(
                    431,
                    "headers_too_large",
                    "headers exceed 8 KiB",
                ));
            }
            let length = content_length(&raw[..end])?;
            if length > MAX_BODY_BYTES {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
                drain(stream, length);
                return Err(error_reply(413, "body_too_large", "body exceeds 1 MiB"));
            }
            if raw.len() >= end + length {
                return Ok(raw);
            }
        } else if raw.len() > MAX_HEADER_BYTES {
            return Err(error_reply(
                431,
                "headers_too_large",
                "headers exceed 8 KiB",
            ));
        }
        match stream.read(&mut buffer) {
            Ok(0) => {
                if header_end(&raw).is_some() {
                    return Ok(raw);
                }
                return Err(error_reply(400, "bad_request", "incomplete request"));
            }
            Ok(read) => raw.extend_from_slice(&buffer[..read]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err(error_reply(408, "timeout", "request timed out"));
            }
            Err(error) => {
                return Err(error_reply(
                    400,
                    "bad_request",
                    &format!("read failed: {error}"),
                ))
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, service: &Service, live: Arc<AtomicUsize>) {
    live.fetch_add(1, Ordering::AcqRel);
    let response = match read_request(&mut stream) {
        Ok(raw) => match parse_request(&raw) {
            Ok(parsed) => {
                let began = Instant::now();
                let response = service.handle(&parsed.method, &parsed.path, &parsed.body);
                println!(
                    "{} {} {} {}",
                    parsed.method,
                    parsed.path,
                    response.status,
                    began.elapsed().as_micros()
                );
                response
            }
            Err(response) => response,
        },
        Err(response) => response,
    };
    write_response(&mut stream, &response);
    let _ = stream.shutdown(std::net::Shutdown::Both);
    live.fetch_sub(1, Ordering::AcqRel);
}

/// Consume a rejected body so the response is not lost to a TCP reset. The
/// budget bounds the drain; a client still sending past it gets a reset.
fn drain(stream: &mut TcpStream, remaining: usize) {
    let mut discard = [0u8; 8192];
    let mut budget = remaining.min(2 * MAX_BODY_BYTES);
    while budget > 0 {
        match stream.read(&mut discard) {
            Ok(0) | Err(_) => break,
            Ok(read) => budget = budget.saturating_sub(read),
        }
    }
}

fn write_response(stream: &mut TcpStream, response: &Response) {
    let reason = match response.status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Response",
    };
    let wire = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        reason,
        response.body.len(),
        response.body
    );
    let _ = stream.write_all(wire.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::EvidenceClass;
    use playbook_state::EventSink;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("playbook-server-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn config(dir: &Path) -> ServerConfig {
        ServerConfig {
            journal_dir: dir.to_path_buf(),
            ..ServerConfig::default()
        }
    }

    fn create(service: &Service, id: &str) -> Response {
        service.handle(
            "POST",
            "/v1/drives",
            &json!({"drive_id": id, "statement": "serve a drive"}).to_string(),
        )
    }

    #[test]
    fn health_and_ready_report_the_registry() {
        let dir = temp_dir("health");
        let service = Service::open(&config(&dir)).unwrap();
        assert_eq!(service.handle("GET", "/healthz", "").status, 200);
        let ready = service.handle("GET", "/readyz", "");
        assert!(ready.body.contains("\"drives\":0"));
        assert!(ready.body.contains("\"rejected_journals\":0"));
        assert_eq!(service.handle("GET", "/nope", "").status, 404);
        assert_eq!(service.handle("DELETE", "/v1/drives", "").status, 405);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn creation_validates_its_input() {
        let dir = temp_dir("create");
        let service = Service::open(&config(&dir)).unwrap();
        assert_eq!(create(&service, "drive-a").status, 201);
        assert_eq!(create(&service, "drive-a").status, 409);
        assert_eq!(
            service
                .handle(
                    "POST",
                    "/v1/drives",
                    &json!({"drive_id": "drive-b", "policy": "mystery"}).to_string()
                )
                .status,
            400
        );
        assert_eq!(
            service
                .handle(
                    "POST",
                    "/v1/drives",
                    &json!({"drive_id": "bad id!"}).to_string()
                )
                .status,
            400
        );
        assert_eq!(service.handle("POST", "/v1/drives", "{bad").status, 400);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_drive_lifecycle_advances_and_journals() {
        let dir = temp_dir("lifecycle");
        let service = Service::open(&config(&dir)).unwrap();
        assert_eq!(create(&service, "drive-life").status, 201);
        assert_eq!(
            service
                .handle("POST", "/v1/drives/drive-life/preflight", "")
                .status,
            200
        );
        for _ in 0..3 {
            let response = service.handle("POST", "/v1/drives/drive-life/steps", "");
            assert_eq!(response.status, 200, "{}", response.body);
            assert!(response.body.contains("\"snapshot\""));
            assert!(response.body.contains("\"decision\""));
        }
        let events = service.handle("GET", "/v1/drives/drive-life/events", "");
        let log: Vec<DriveEvent> = serde_json::from_str(&events.body).unwrap();
        assert!(log
            .iter()
            .any(|event| matches!(event, DriveEvent::ToolInvoked { .. })));
        let policy = Policy::builtin();
        assert!(playbook_state::fold(&log, &policy, ApplyMode::Audit).is_ok());
        let recovered = Journal::open(&dir.join("drive-life.jsonl")).unwrap();
        assert_eq!(recovered.events.len(), log.len());
        let trace = service.handle("GET", "/v1/drives/drive-life/trace", "");
        assert!(trace.body.contains("span_id"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_crashed_invocation_reconciles_at_handler_level() {
        let dir = temp_dir("crash");
        std::fs::create_dir_all(&dir).unwrap();
        let policy = Policy::builtin();
        let personnel = playbook_sim::roster();
        let mut authority = Authority::start(
            DriveId::new("drive-crash").unwrap(),
            TaskContract {
                statement: "crash recovery".into(),
                workload_class: "engineering".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            started_budget(1_000, 1_000),
            policy.clone(),
            personnel.clone(),
            1_000,
        )
        .unwrap();
        authority
            .declare_gate(
                playbook_core::Gate::declare(
                    playbook_core::GateId::new("implementation").unwrap(),
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
        let mut journal = Journal::create(&dir.join("drive-crash.jsonl")).unwrap();
        for event in &log[..invoke_end] {
            EventSink::record(&mut journal, event).unwrap();
        }
        drop(journal);

        let service = Service::open(&config(&dir)).unwrap();
        let snapshot = service.handle("GET", "/v1/drives/drive-crash", "");
        assert_eq!(snapshot.status, 200);
        assert!(snapshot.body.contains("active_capability"));
        assert!(!snapshot.body.contains("\"active_capability\":null"));
        let refused = service.handle("POST", "/v1/drives/drive-crash/steps", "");
        assert_eq!(refused.status, 409);
        assert!(refused.body.contains("reconcile_required"));
        let reconciled = service.handle(
            "POST",
            "/v1/drives/drive-crash/reconcile",
            &json!({"key": key, "occurred": false}).to_string(),
        );
        assert_eq!(reconciled.status, 200);
        let after = service.handle("POST", "/v1/drives/drive-crash/steps", "");
        assert_ne!(after.status, 409, "{}", after.body);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_journal_is_the_authority_for_state() {
        let dir = temp_dir("authority");
        let service = Service::open(&config(&dir)).unwrap();
        create(&service, "drive-auth");
        assert_eq!(
            service
                .handle("POST", "/v1/drives/drive-auth/preflight", "")
                .status,
            200
        );
        service.handle("POST", "/v1/drives/drive-auth/steps", "");
        let events = service.handle("GET", "/v1/drives/drive-auth/events", "");
        let log: Vec<DriveEvent> = serde_json::from_str(&events.body).unwrap();
        let policy = Policy::builtin();
        let live = playbook_state::fold(&log, &policy, ApplyMode::Audit).unwrap();
        let recovered = Journal::open(&dir.join("drive-auth.jsonl")).unwrap();
        let disk = playbook_state::fold(&recovered.events, &policy, ApplyMode::Audit).unwrap();
        assert_eq!(live, disk);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_rejected_journal_is_quarantined_and_counted() {
        let dir = temp_dir("quarantine");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("broken.jsonl"), b"{\"seq\":1,\"digest\":\"00\"}\n").unwrap();
        let service = Service::open(&config(&dir)).unwrap();
        assert_eq!(service.rejected_journals(), 1);
        assert!(dir.join("broken.jsonl.rejected").exists());
        assert!(!dir.join("broken.jsonl").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn huddle_value() -> Value {
        json!({
            "predicted_rework_avoided": 600,
            "expected_failure_reduction": 300,
            "expected_coordination_gain": 200,
            "expected_budget_preservation": 100,
            "inference_cost": 100,
            "latency_cost": 50,
            "context_cost": 50,
            "opportunity_cost": 0
        })
    }

    fn huddle_request_body() -> Value {
        json!({
            "scope": "position",
            "trigger_kind": "failure",
            "reason_code": "repeated_failed_play",
            "evidence_refs": [],
            "manual": false,
            "value": huddle_value()
        })
    }

    fn huddle_report_body(participant: &str) -> Value {
        json!({
            "participant": participant,
            "verified_progress": [],
            "blockers": [],
            "changed_assumptions": [],
            "dependency_changes": [],
            "contradictions": [],
            "proposed_action": null,
            "confidence_milli": 9_000
        })
    }

    #[test]
    fn huddles_open_collect_commit_and_refuse_storms_over_http() {
        let dir = temp_dir("huddles");
        let service = Service::open(&config(&dir)).unwrap();
        let created = service.handle(
            "POST",
            "/v1/drives",
            &json!({"drive_id": "drive-huddle", "tokens": 100_000}).to_string(),
        );
        assert_eq!(created.status, 201, "{}", created.body);

        let opened = service.handle(
            "POST",
            "/v1/drives/drive-huddle/huddles/request",
            &huddle_request_body().to_string(),
        );
        assert_eq!(opened.status, 200, "{}", opened.body);
        assert!(opened.body.contains("\"admission\":\"open\""));
        assert!(opened.body.contains("\"huddle_id\":\"huddle-1\""));

        let list = service.handle("GET", "/v1/drives/drive-huddle/huddles", "");
        assert_eq!(list.status, 200);
        assert!(list.body.contains("\"scope\":\"position\""));
        assert!(list.body.contains("\"phase\":\"collect_reports\""));

        // The roster is the candidate pool; every member clears the policy
        // floor, so all three are invited and must report before a commit.
        for participant in ["cargo-toolchain", "coding-specialist", "rust-analyser"] {
            let response = service.handle(
                "POST",
                "/v1/drives/drive-huddle/huddles/huddle-1/reports",
                &huddle_report_body(participant).to_string(),
            );
            assert_eq!(response.status, 200, "{}: {}", participant, response.body);
        }
        let duplicate = service.handle(
            "POST",
            "/v1/drives/drive-huddle/huddles/huddle-1/reports",
            &huddle_report_body("cargo-toolchain").to_string(),
        );
        assert_eq!(duplicate.status, 409);
        assert!(duplicate.body.contains("inadmissible"));

        let outcome = json!({
            "huddle_id": "huddle-1",
            "reconciled_state": {"verified_evidence": [], "gate_snapshots": [], "assumptions": []},
            "resolved_contradictions": [],
            "unresolved_contradictions": [],
            "assignment_changes": [
                {"personnel_id": "rust-analyser", "from": null, "to": "test.unit"}
            ],
            "strategy_change": null,
            "personnel_changes": [],
            "revised_ytg_milli": null,
            "next_play": null,
            "next_huddle_conditions": [],
            "escalated_to": null
        });
        let committed = service.handle(
            "POST",
            "/v1/drives/drive-huddle/huddles/huddle-1/commit",
            &outcome.to_string(),
        );
        assert_eq!(committed.status, 200, "{}", committed.body);

        let record = service.handle("GET", "/v1/drives/drive-huddle/huddles/huddle-1", "");
        assert_eq!(record.status, 200);
        assert!(record.body.contains("\"closure\":\"committed\""));
        assert!(record
            .body
            .contains("\"reason_code\":\"outcome_committed\""));

        // The field did not move, so a repeat request is a refused storm.
        let again = service.handle(
            "POST",
            "/v1/drives/drive-huddle/huddles/request",
            &huddle_request_body().to_string(),
        );
        assert_eq!(again.status, 200, "{}", again.body);
        assert!(again.body.contains("\"admission\":\"refused\""));
        assert!(again.body.contains("\"reason_code\":\"unchanged_field\""));

        // The committed assignment survives replay from the journal.
        let events = service.handle("GET", "/v1/drives/drive-huddle/events", "");
        let log: Vec<DriveEvent> = serde_json::from_str(&events.body).unwrap();
        let replay = playbook_state::fold(&log, &Policy::builtin(), ApplyMode::Audit).unwrap();
        assert!(replay.huddles().assignments().contains_key("rust-analyser"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
