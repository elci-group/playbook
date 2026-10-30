//! HTTP adapter for persistent franchise management.
use super::{error_of, error_reply, last_at_ms, reply, Response, CLOCK_STEP_MS, MAX_DRIVES};
use playbook_core::{DriveBudget, DriveId, FranchiseId, ObjectiveId, PersonnelId, PlaybookError};
use playbook_elci::FixturePlane;
use playbook_franchise::{Availability, Franchise, Objective, RosterMember};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

struct Hosted {
    franchise: Franchise,
    planes: BTreeMap<String, FixturePlane>,
}
pub(super) struct FranchiseApi {
    directory: PathBuf,
    entries: Mutex<BTreeMap<String, Hosted>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    franchise_id: FranchiseId,
    salary_cap: DriveBudget,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Acquire {
    member: RosterMember,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AvailabilityRequest {
    availability: Availability,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StageRoster {
    contract: playbook_front_office::RosterContract,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reason {
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartDrive {
    objective_id: ObjectiveId,
    drive_id: DriveId,
    budget: DriveBudget,
    #[serde(default)]
    at_ms: Option<u64>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Action {
    player_id: Option<PersonnelId>,
    budget: Option<DriveBudget>,
    cost: Option<DriveBudget>,
    at_ms: Option<u64>,
    key: Option<String>,
    occurred: Option<bool>,
    reason: Option<String>,
}
#[tracing::instrument]
fn parse<T: DeserializeOwned>(body: &str) -> Result<T, PlaybookError> {
    serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(PlaybookError::json)
}
#[tracing::instrument]
fn required<T>(value: Option<T>, name: &str) -> Result<T, PlaybookError> {
    value.ok_or_else(|| PlaybookError::Evidence(format!("{name} is required")))
}

impl FranchiseApi {
    pub(super) #[tracing::instrument]
fn open(directory: &Path) -> Result<Self, PlaybookError> {
        std::fs::create_dir_all(directory).map_err(|e| PlaybookError::Io(e.to_string()))?;
        let mut entries = BTreeMap::new();
        for entry in std::fs::read_dir(directory).map_err(|e| PlaybookError::Io(e.to_string()))? {
            let path = entry.map_err(|e| PlaybookError::Io(e.to_string()))?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let franchise = Franchise::open(&path)?;
                let snapshot = franchise.snapshot();
                if path.file_stem().and_then(|s| s.to_str()) != Some(snapshot.id.as_str()) {
                    return Err(PlaybookError::Evidence(
                        "franchise filename disagrees with id".into(),
                    ));
                }
                entries.insert(
                    snapshot.id.to_string(),
                    Hosted {
                        franchise,
                        planes: BTreeMap::new(),
                    },
                );
            }
        }
        Ok(Self {
            directory: directory.into(),
            entries: Mutex::new(entries),
        })
    }
    pub(super) #[tracing::instrument]
fn handle(&self, method: &str, path: &[&str], body: &str) -> Response {
        match self.route(method, path, body) {
            Ok(response) => response,
            Err(error) => error_of(error),
        }
    }
    #[tracing::instrument]
fn route(&self, method: &str, path: &[&str], body: &str) -> Result<Response, PlaybookError> {
        let mut entries = self.entries.lock().expect("franchise registry");
        if path.is_empty() {
            return match method {
                "GET" => Ok(reply(
                    200,
                    &json!({"franchises": entries.keys().collect::<Vec<_>>()}),
                )),
                "POST" => {
                    let request: Create = parse(body)?;
                    FranchiseId::new(request.franchise_id.as_str())?;
                    let id = request.franchise_id.to_string();
                    let path = self.directory.join(format!("{id}.json"));
                    if entries.contains_key(&id) || path.exists() {
                        return Ok(error_reply(
                            409,
                            "duplicate_franchise",
                            "franchise already exists",
                        ));
                    }
                    if entries.len() >= MAX_DRIVES {
                        return Ok(error_reply(503, "franchise_cap", "franchise cap reached"));
                    }
                    let franchise =
                        Franchise::create(&path, request.franchise_id, request.salary_cap)?;
                    let snapshot = franchise.snapshot();
                    entries.insert(
                        id,
                        Hosted {
                            franchise,
                            planes: BTreeMap::new(),
                        },
                    );
                    Ok(reply(201, &json!({"franchise": snapshot})))
                }
                _ => Ok(error_reply(
                    405,
                    "method_not_allowed",
                    "method is not allowed",
                )),
            };
        }
        let Some(hosted) = entries.get_mut(path[0]) else {
            return Ok(error_reply(404, "unknown_franchise", "no such franchise"));
        };
        let franchise = &mut hosted.franchise;
        match (method, &path[1..]) {
            ("POST", ["roster-contracts", "stage"]) => {
                let request: StageRoster = parse(body)?;
                franchise.stage_roster(request.contract, &request.reason)?;
            }
            ("POST", ["roster-contracts", "activate"]) => {
                franchise.activate_roster(&parse::<Reason>(body)?.reason)?;
            }
            ("GET", []) => {
                let snapshot = franchise.snapshot();
                return Ok(reply(
                    200,
                    &json!({"record": snapshot.record()?, "scouting": snapshot.scouting()?, "franchise": snapshot}),
                ));
            }
            ("POST", ["roster"]) => {
                let request: Acquire = parse(body)?;
                franchise.acquire(request.member, &request.reason)?;
            }
            ("POST", ["roster", player, "availability"]) => {
                let request: AvailabilityRequest = parse(body)?;
                franchise.set_availability(
                    &PersonnelId::new(*player)?,
                    request.availability,
                    &request.reason,
                )?;
            }
            ("POST", ["objectives"]) => franchise.declare_objective(parse::<Objective>(body)?)?,
            ("POST", ["drives"]) => {
                let request: StartDrive = parse(body)?;
                if franchise.snapshot().drives.len() >= MAX_DRIVES {
                    return Ok(error_reply(503, "drive_cap", "drive cap reached"));
                }
                franchise.start_drive(
                    &request.objective_id,
                    request.drive_id,
                    request.budget,
                    request.at_ms.unwrap_or(1_000),
                )?;
            }
            ("GET", ["drives", id]) => {
                let snapshot = franchise.snapshot();
                let Some(drive) = snapshot.drives.get(*id) else {
                    return Ok(error_reply(404, "unknown_drive", "no such drive"));
                };
                return Ok(reply(
                    200,
                    &json!({"objective_id": drive.objective_id,
                    "snapshot": drive.state()?.snapshot(&playbook_policy::Policy::builtin())}),
                ));
            }
            ("GET", ["drives", id, "events"]) => {
                let snapshot = franchise.snapshot();
                let Some(drive) = snapshot.drives.get(*id) else {
                    return Ok(error_reply(404, "unknown_drive", "no such drive"));
                };
                return Ok(reply(200, &json!(drive.events)));
            }
            ("POST", ["drives", raw_id, action]) => {
                let id = DriveId::new(*raw_id)?;
                let request: Action = parse(body)?;
                let snapshot = franchise.snapshot();
                let Some(drive) = snapshot.drives.get(*raw_id) else {
                    return Ok(error_reply(404, "unknown_drive", "no such drive"));
                };
                let at_ms = request
                    .at_ms
                    .unwrap_or_else(|| last_at_ms(&drive.events).saturating_add(CLOCK_STEP_MS));
                let mut result = Value::Null;
                match *action {
                    "preflight" | "steps" => {
                        let plane = hosted.planes.entry((*raw_id).into()).or_insert_with(|| {
                            let mut plane = FixturePlane::mvp();
                            plane.resume_generation(
                                drive
                                    .state()
                                    .expect("audited drive")
                                    .capability_generation(),
                            );
                            plane
                        });
                        if *action == "preflight" {
                            franchise.preflight(&id, plane)?;
                        } else {
                            result = json!(franchise.step(
                                &id,
                                request.player_id.as_ref(),
                                plane,
                                at_ms
                            )?);
                        }
                    }
                    "punt" => franchise.punt(&id)?,
                    "pressure" => {
                        franchise.pressure(&id, required(request.cost, "cost")?, at_ms)?
                    }
                    "receive" => franchise.receive(
                        &id,
                        &required(request.player_id, "player_id")?,
                        required(request.budget, "budget")?,
                        at_ms,
                    )?,
                    "reconcile" => franchise.reconcile(
                        &id,
                        &required(request.key, "key")?,
                        required(request.occurred, "occurred")?,
                    )?,
                    "complete" => result = json!({"completed": franchise.complete(&id, at_ms)?}),
                    "abandon" => franchise.abandon(&id, &required(request.reason, "reason")?)?,
                    _ => return Ok(error_reply(404, "unknown_action", "no such action")),
                }
                let snapshot = franchise.snapshot();
                return Ok(reply(
                    200,
                    &json!({"result": result,
                    "snapshot": snapshot.drives[*raw_id].state()?.snapshot(&playbook_policy::Policy::builtin())}),
                ));
            }
            _ => return Ok(error_reply(404, "not_found", "no such franchise route")),
        }
        Ok(reply(200, &json!({"franchise": franchise.snapshot()})))
    }
}
