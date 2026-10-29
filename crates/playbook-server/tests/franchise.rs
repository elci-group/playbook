use playbook_core::*;
use playbook_franchise::{Availability, Objective, RosterMember};
use playbook_server::{ServerConfig, Service};
use playbook_state::started_budget;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Host {
    directory: PathBuf,
    service: Service,
}
impl Host {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "franchise-api-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let service = Service::open(&ServerConfig {
            journal_dir: directory.clone(),
            ..ServerConfig::default()
        })
        .unwrap();
        Self { directory, service }
    }
    fn request(&self, method: &str, path: &str, body: Value, status: u16) -> Value {
        let response = self.service.handle(method, path, &body.to_string());
        assert_eq!(response.status, status, "{}", response.body);
        serde_json::from_str(&response.body).unwrap()
    }
    fn create(&self) {
        let budget = started_budget(1_000, 1_000);
        self.request(
            "POST",
            "/v1/franchises",
            json!({"franchise_id":"team", "salary_cap":budget}),
            201,
        );
        for personnel in playbook_sim::roster() {
            self.request(
                "POST",
                "/v1/franchises/team/roster",
                json!({"member": RosterMember {
                personnel, position: "engineering".into(), provider: None, model: None,
                availability: Availability::Ready,
            }, "reason":"initial acquisition"}),
                200,
            );
        }
        self.request("POST", "/v1/franchises/team/roster", json!({"member": RosterMember {
            personnel: playbook_sim::person("replacement", PersonnelKind::DeterministicProcedure, 1, None),
            position:"engineering".into(), provider:Some("other".into()), model:None, availability:Availability::Benched,
        }, "reason":"preseason depth"}), 200);
        self.request(
            "POST",
            "/v1/franchises/team/objectives",
            json!(Objective {
                id: ObjectiveId::new("ship").unwrap(),
                task: playbook_sim::task_contract(TaskIntegrity::Sound),
                gates: playbook_sim::standard_gates().unwrap(),
                drive_id: None,
            }),
            200,
        );
        self.request(
            "POST",
            "/v1/franchises/team/drives",
            json!({"drive_id":"drive", "objective_id":"ship", "budget":budget}),
            200,
        );
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).ok();
    }
}

#[test]
fn http_franchise_survives_restart_and_substitutes_a_player() {
    let mut host = Host::new();
    host.create();
    host.request(
        "POST",
        "/v1/franchises/team/drives/drive/preflight",
        json!({}),
        200,
    );
    for _ in 0..2 {
        host.request(
            "POST",
            "/v1/franchises/team/drives/drive/steps",
            json!({}),
            200,
        );
    }
    let before = host.request("GET", "/v1/franchises/team", json!({}), 200);
    host.request(
        "POST",
        "/v1/franchises/team/roster/cargo-toolchain/availability",
        json!({"availability":"unavailable", "reason":"provider lost"}),
        200,
    );
    host.request(
        "POST",
        "/v1/franchises/team/roster/replacement/availability",
        json!({"availability":"ready", "reason":"bench substitution"}),
        200,
    );
    host.service = Service::open(&ServerConfig {
        journal_dir: host.directory.clone(),
        ..ServerConfig::default()
    })
    .unwrap();
    let resumed = host.request("GET", "/v1/franchises/team", json!({}), 200);
    assert_eq!(
        resumed["franchise"]["drives"],
        before["franchise"]["drives"]
    );
    host.request(
        "POST",
        "/v1/franchises/team/drives/drive/steps",
        json!({"player_id":"cargo-toolchain"}),
        400,
    );
    let step = host.request(
        "POST",
        "/v1/franchises/team/drives/drive/steps",
        json!({"player_id":"replacement"}),
        200,
    );
    assert_eq!(step["result"]["selected_personnel"], "replacement");
    assert!(step["snapshot"]["ytg_milli"].as_u64().unwrap() < 6_000);
    let events = host.request(
        "GET",
        "/v1/franchises/team/drives/drive/events",
        json!({}),
        200,
    );
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["type"] == "tool_invoked"));
    let incomplete = host.request(
        "POST",
        "/v1/franchises/team/drives/drive/complete",
        json!({}),
        200,
    );
    assert_eq!(incomplete["result"]["completed"], false);
    // Managed drive ids cannot be dispatched through the legacy drive registry.
    host.request("POST", "/v1/drives/drive/steps", json!({}), 404);
}

#[test]
fn strict_management_inputs_and_shared_cap_are_enforced_over_http() {
    let host = Host::new();
    host.request(
        "POST",
        "/v1/franchises",
        json!({"franchise_id":"../escape", "salary_cap":started_budget(1_000, 1_000)}),
        400,
    );
    host.create();
    host.request(
        "POST",
        "/v1/franchises",
        json!({"franchise_id":"team", "salary_cap":started_budget(1_000, 1_000)}),
        409,
    );
    host.request(
        "POST",
        "/v1/franchises/team/objectives",
        json!(Objective {
            id: ObjectiveId::new("second").unwrap(),
            task: playbook_sim::task_contract(TaskIntegrity::Sound),
            gates: playbook_sim::standard_gates().unwrap(),
            drive_id: None,
        }),
        200,
    );
    host.request("POST", "/v1/franchises/team/drives", json!({"drive_id":"second-drive", "objective_id":"second", "budget":started_budget(1_000, 1_000)}), 409);
    host.request(
        "POST",
        "/v1/franchises/team/roster/cargo-toolchain/availability",
        json!({"availability":"benched", "reason":""}),
        400,
    );
    host.request(
        "POST",
        "/v1/franchises/team/drives/drive/reconcile",
        json!({"key":"unknown"}),
        400,
    );
    host.request(
        "POST",
        "/v1/franchises/team/drives/drive/steps",
        json!({"unknown":true}),
        400,
    );
    host.request("GET", "/v1/franchises/absent", json!({}), 404);
    let list = host.request("GET", "/v1/franchises", json!({}), 200);
    assert_eq!(list["franchises"], json!(["team"]));
}

#[test]
fn corrupt_franchise_blocks_startup_instead_of_losing_objectives() {
    let host = Host::new();
    host.create();
    std::fs::write(host.directory.join("franchises/team.json"), b"broken").unwrap();
    assert!(Service::open(&ServerConfig {
        journal_dir: host.directory.clone(),
        ..ServerConfig::default()
    })
    .is_err());
}

#[test]
fn offline_roster_endpoint_and_transaction_window_enforce_the_plane_boundary() {
    let host = Host::new();
    let input: Value =
        serde_json::from_str(include_str!("../../../fixtures/front_office.json")).unwrap();
    let contract = host.request("POST", "/v1/front-office/rosters", input.clone(), 200);
    assert_eq!(contract["magic"], "ROSTER/1");
    let mut live_payload = input;
    live_payload["task"] = json!("inspect this repository");
    host.request("POST", "/v1/front-office/rosters", live_payload, 400);
    host.request(
        "POST",
        "/v1/franchises",
        json!({"franchise_id":"preseason", "salary_cap":started_budget(1_000, 1_000)}),
        201,
    );
    for personnel in playbook_sim::roster() {
        host.request(
            "POST",
            "/v1/franchises/preseason/roster",
            json!({"member":RosterMember {
            provider:Some(personnel.id.to_string()), personnel, position:"engineering".into(),
            model:None, availability:Availability::Ready,
        }, "reason":"preseason inventory"}),
            200,
        );
    }
    host.request(
        "POST",
        "/v1/franchises/preseason/roster-contracts/stage",
        json!({"contract":contract,"reason":"offline selection"}),
        200,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/roster-contracts/activate",
        json!({"reason":"before kickoff"}),
        200,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/objectives",
        json!(Objective {
            id: ObjectiveId::new("ship").unwrap(),
            task: playbook_sim::task_contract(TaskIntegrity::Sound),
            gates: playbook_sim::standard_gates().unwrap(),
            drive_id: None,
        }),
        200,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/drives",
        json!({"objective_id":"ship", "drive_id":"frozen", "budget":started_budget(1_000, 1_000)}),
        200,
    );
    let before = host.request("GET", "/v1/franchises/preseason", json!({}), 200);
    assert_eq!(
        before["franchise"]["drives"]["frozen"]["frozen_roster"]["contract"],
        contract
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/roster-contracts/activate",
        json!({"reason":"illegal mid-drive change"}),
        400,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/drives/frozen/preflight",
        json!({}),
        200,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/drives/frozen/steps",
        json!({"player_id":"rust-analyser"}),
        400,
    );
    host.request(
        "POST",
        "/v1/franchises/preseason/drives/frozen/steps",
        json!({"player_id":"cargo-toolchain"}),
        200,
    );
}
