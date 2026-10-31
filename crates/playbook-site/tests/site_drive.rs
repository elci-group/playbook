//! The real-world workflow: the Playbook website is built under Playbook's
//! own authority. The gates are the site's actual acceptance criteria, the
//! evidence is the actual output of the actual commands — sealed, accepted
//! by the drive, and replay-checked. No staged fixtures: the JSON bodies
//! carry the real measurements of the real artifact.

use playbook_core::{Evidence, EvidenceClass, ObjectiveId};
use playbook_sim::{gate_one, open_drive, FixturePlane};
use std::process::Command;

fn render_site(out: &std::path::Path) -> Result<(), String> {
    let binary = env!("CARGO_BIN_EXE_playbook-site");
    let workspace = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(workspace)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot locate the repository root")?;
    let output = Command::new(binary)
        .arg("--workspace")
        .arg(repo_root)
        .arg("--out")
        .arg(out)
        .output()
        .map_err(|error| format!("spawn failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "render failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn seal(command: &str, passed: u64, failed: u64, gate_id: &str, at: u64) -> Evidence {
    Evidence::seal(
        EvidenceClass::TestReport,
        serde_json::json!({
            "command": command,
            "repository_hash": "playbook-workspace",
            "test_suite": gate_id,
            "exit_code": if failed == 0 { 0 } else { 1 },
            "passed": passed,
            "failed": failed,
            "ignored": 0,
            "timestamp": at.to_string(),
            "gate_id": gate_id
        }),
        at,
    )
    .expect("real evidence seals")
}

#[test]
fn the_website_is_completed_under_the_authority() {
    let out = std::env::temp_dir().join("playbook-site-governed");
    let _ = std::fs::remove_dir_all(&out);

    // The drive: three gates, exactly the site's acceptance criteria.
    let (mut drive, _) = open_drive(
        FixturePlane::ready(),
        vec![
            gate_one("site_renders", EvidenceClass::TestReport),
            gate_one("site_deterministic", EvidenceClass::TestReport),
            gate_one("site_content_complete", EvidenceClass::TestReport),
        ],
    )
    .unwrap();

    // GATE 1 — the real generator renders the real site.
    render_site(&out).expect("the render must succeed");
    let index = std::fs::read_to_string(out.join("index.html")).expect("index.html");
    let rendered_markers = ["PLAYBOOK", "What it is", "The ", "crates", "Watch it play"]
        .iter()
        .filter(|marker| index.contains(*marker))
        .count() as u64;
    drive
        .accept_evidence(seal(
            "playbook-site --workspace . --out site",
            rendered_markers,
            5 - rendered_markers,
            "site_renders",
            1,
        ))
        .unwrap();

    // GATE 2 — the render is deterministic: two runs, byte-identical.
    let out_again = std::env::temp_dir().join("playbook-site-governed-2");
    let _ = std::fs::remove_dir_all(&out_again);
    render_site(&out_again).expect("the second render must succeed");
    let first = std::fs::read(out.join("index.html")).unwrap();
    let second = std::fs::read(out_again.join("index.html")).unwrap();
    let identical = (first == second) as u64;
    drive
        .accept_evidence(seal(
            "playbook-site determinism check",
            identical,
            1 - identical,
            "site_deterministic",
            2,
        ))
        .unwrap();

    // GATE 3 — the content is extracted, not claimed: every crate name the
    // generator read from the manifest appears in the rendered HTML.
    let workspace = playbook_site::extract(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repo root"),
    );
    assert!(workspace.crates.len() > 20, "the workspace is real");
    let missing: Vec<&str> = workspace
        .crates
        .iter()
        .filter(|info| !index.contains(&info.name))
        .map(|info| info.name.as_str())
        .collect();
    drive
        .accept_evidence(seal(
            "playbook-site content extraction check",
            (workspace.crates.len() - missing.len()) as u64,
            missing.len() as u64,
            "site_content_complete",
            3,
        ))
        .unwrap();
    assert!(
        missing.is_empty(),
        "every extracted crate must render: {missing:?}"
    );

    // Every gate satisfied, the drive completes — on evidence, not claims.
    for gate in drive.state().gates() {
        assert_eq!(
            gate.state,
            playbook_core::GateState::Satisfied,
            "{}",
            gate.id.as_str()
        );
    }
    assert!(drive.claim_complete(4).expect("completion claim"));

    // And the record replays: the same journal folds to the same state.
    let replayed = playbook_sim::fold(
        drive.log(),
        &playbook_sim::Policy::builtin(),
        playbook_sim::ApplyMode::Resimulate,
    )
    .unwrap();
    assert_eq!(&replayed, drive.state(), "the governed workflow replays");
}

#[test]
fn the_franchise_governs_the_site_objective() {
    // The governance layer: a franchise declares the objective with the
    // same gates and reserves a bounded budget before any work dispatches.
    use playbook_core::{DriveBudget, DriveId, FranchiseId, Gate, TaskContract, TaskIntegrity};
    use playbook_franchise::{Franchise, Objective};

    let path = std::env::temp_dir().join("playbook-site-franchise.json");
    let _ = std::fs::remove_file(&path);
    let mut franchise = Franchise::create(
        &path,
        FranchiseId::new("site-franchise").unwrap(),
        DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            wall_clock_ms: 600_000,
            tool_calls: 1_000,
            inference_calls: 1_000,
            retry_count: 100,
            context_capacity: 1_000_000,
            provider_quota: 10_000,
        },
    )
    .unwrap();
    let gates: Vec<Gate> = [
        "site_renders",
        "site_deterministic",
        "site_content_complete",
    ]
    .iter()
    .map(|id| gate_one(id, EvidenceClass::TestReport))
    .collect();
    franchise
        .declare_objective(Objective {
            id: ObjectiveId::new("objective-site").unwrap(),
            task: TaskContract {
                statement: "publish the Playbook website, generated from the workspace".into(),
                workload_class: "web".into(),
                constraints: Vec::new(),
                integrity: TaskIntegrity::Sound,
            },
            gates,
            drive_id: None,
        })
        .unwrap();
    franchise
        .start_drive(
            &ObjectiveId::new("objective-site").unwrap(),
            DriveId::new("drive-site").unwrap(),
            DriveBudget {
                tokens: 100_000,
                monetary_cost_micros: 10_000_000,
                wall_clock_ms: 60_000,
                tool_calls: 100,
                inference_calls: 100,
                retry_count: 10,
                context_capacity: 100_000,
                provider_quota: 1_000,
            },
            0,
        )
        .unwrap();
    let snapshot = franchise.snapshot();
    let objective = snapshot
        .objectives
        .get("objective-site")
        .expect("the objective is on the record");
    assert_eq!(objective.gates.len(), 3);
    assert_eq!(objective.drive_id.as_ref().unwrap().as_str(), "drive-site");
    // The allocation is bounded by the franchise cap: governance held.
    assert!(snapshot.allocated.fits_within(snapshot.salary_cap));
    let _ = std::fs::remove_file(&path);
}
