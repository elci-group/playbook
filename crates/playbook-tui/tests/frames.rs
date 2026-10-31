//! The Matchday campaign: golden frames, depth switching, and film-room
//! replay over a real MVP journal — every frame a pure projection of the
//! authoritative fold.

use playbook_journal::Journal;
use playbook_policy::Policy;
use playbook_presentation::{frames, Frame, PresentationSnapshot};
use playbook_sim::run_mvp_with_journal;
use playbook_tui::{
    render_blitz_overlay, render_broadcast, render_kick_decision, render_lastdown, render_manager,
    render_scorebug, render_three, FormKey,
};

use playbook_core::{DriveBudget, DriveId, TaskContract, TaskIntegrity};
use playbook_events::DriveEvent;
use playbook_presentation::{present_event, ShadowCompare};

fn started() -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: DriveId::new("drive-matchday").unwrap(),
        task: TaskContract {
            statement: "hold the field under projection".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: TaskIntegrity::Sound,
        },
        budget: DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            wall_clock_ms: 600_000,
            tool_calls: 1_000,
            inference_calls: 1_000,
            retry_count: 100,
            context_capacity: 1_000_000,
            provider_quota: 10_000,
        },
        at_ms: 133_000,
    }
}

fn snapshot_with(started_event: DriveEvent) -> PresentationSnapshot {
    let events = vec![
        started_event,
        DriveEvent::FieldConstructed {
            objective_id: "objective-matchday".into(),
            total_weight_ppm: 1_000_000,
            consolidation_boundaries: vec![10],
        },
        DriveEvent::UnvalidatedProgressRecorded {
            yards: 6,
            unvalidated_total: 6,
        },
        DriveEvent::BudgetConsumed {
            cost: DriveBudget {
                wall_clock_ms: 133_000,
                monetary_cost_micros: 21_000,
                tokens: 9_000,
                ..DriveBudget::zero()
            },
            at_ms: 133_000,
        },
        DriveEvent::RedZoneEntered {
            outstanding_mandatory: 1,
        },
        DriveEvent::ValidationBoundaryPassed {
            yard: 10,
            evidence_classes: vec!["test".into()],
        },
        DriveEvent::ProgressBanked {
            banked_yard: 10,
            banked_yards_total: 10,
        },
    ];
    let state = playbook_state::fold(
        &events,
        &Policy::builtin(),
        playbook_state::ApplyMode::Audit,
    )
    .unwrap();
    PresentationSnapshot::from_state(&state)
}

#[test]
fn the_scorebug_frame_is_golden() {
    let snapshot = snapshot_with(started());
    let frame = render_scorebug(&snapshot.scorebug, 86);
    assert!(
        frame.contains("│ PLAYBOOK LIVE │ drive-matchday │"),
        "the scorebug opens with the brand: {frame}"
    );
    assert!(
        frame.contains("YTG 0.0 │ Δ+0.0"),
        "the projected line reads: {frame}"
    );
    assert!(
        frame.contains("02:13"),
        "the clock counts consumed wall time: {frame}"
    );
    assert!(frame.contains("21m¢"), "the spend reads in micros: {frame}");
    assert!(
        frame.contains("│ ●"),
        "live drives carry the ball marker: {frame}"
    );
    assert!(
        frame.ends_with(&format!("└{}┘", "─".repeat(84))),
        "the box closes: {frame}"
    );
    // Deterministic.
    assert_eq!(render_scorebug(&snapshot.scorebug, 86), frame);
}

#[test]
fn the_manager_dugout_renders_the_field_and_squad() {
    let snapshot = snapshot_with(started());
    let dugout = render_manager(&snapshot, 78);
    assert!(dugout.starts_with("┌─ PLAYBOOK "));
    assert!(dugout.contains("RED ZONE") || dugout.contains("OPEN FIELD"));
    assert!(dugout.contains("ROSTER"));
    assert!(dugout.contains("[F] FIELD GOAL"));
    // Deterministic: the same snapshot renders the same dugout.
    assert_eq!(render_manager(&snapshot, 78), dugout);
}

#[test]
fn the_broadcast_renders_the_field_rail_and_evidence() {
    let snapshot = snapshot_with(started());
    let broadcast = render_broadcast(&snapshot, 80);
    assert!(broadcast.contains("PLAYBOOK LIVE"));
    assert!(broadcast.contains("OWN"));
    assert!(broadcast.contains("GOAL"));
    assert!(broadcast.contains("RED ZONE"));
    assert!(broadcast.contains("BANKED         10 yd validated"));
    assert!(broadcast.contains('%'));
}

#[test]
fn the_overlays_render_from_the_projection_alone() {
    let snapshot = snapshot_with(started());
    // The kick decision moment reads the field geometry.
    let kick = render_kick_decision(&snapshot.field, 60);
    assert!(kick.contains("FIELD GOAL RANGE"));
    // The lastdown decision moment marks the chosen call only.
    let lastdown = render_lastdown("field_goal", 40);
    assert!(lastdown.contains("field_goal ← CHOSEN"));
    assert!(!lastdown.contains("punt ← CHOSEN"));
    // The depth switch cycles glance → tactical → forensic.
    let forms = present_event(&DriveEvent::TouchdownScored {
        banked_yards_total: 10,
        evidence_count: 3,
    });
    let frame = render_three(7, "touchdown_scored", &forms, FormKey::Glance);
    assert!(frame.contains("TOUCHDOWN"));
    assert_eq!(
        render_three(7, "touchdown_scored", &forms, FormKey::Glance),
        frame
    );
    // The blitz overlay needs no live blitz: an empty projection is calm.
    let quiet = render_blitz_overlay(&playbook_presentation::BlitzOverlay {
        id: "blitz-1".into(),
        phase: "Racing".into(),
        workers: vec!["astra".into(), "glm".into()],
        candidates: 1,
        winner: None,
        waves: 1,
    });
    assert!(quiet.contains("2 WORKERS DEPLOYED"));
    assert!(quiet.contains("RACING"));
}

#[test]
fn the_shadow_compare_reads_on_the_dugout() {
    let mut snapshot = snapshot_with(started());
    snapshot.shadow = Some(ShadowCompare {
        starters: "Astra + Luna".into(),
        shadow: "GLM + MiniMax".into(),
        starters_epa_deci: 78,
        shadow_epa_deci: 74,
        value_delta_deci: 4,
        actual_on_frontier: true,
    });
    let dugout = render_manager(&snapshot, 78);
    assert!(dugout.contains("SHADOW DEFENCE"));
    assert!(dugout.contains("GLM + MiniMax"));
    assert!(dugout.contains("frontier: yes"));
}

#[test]
fn the_film_room_replays_a_real_mvp_journal() {
    let journal_path = std::env::temp_dir().join("playbook-tui-matchday.jsonl");
    let _ = std::fs::remove_file(&journal_path);
    let report = run_mvp_with_journal(&journal_path).unwrap();
    assert!(report.replay_match);
    let recovery = Journal::open(&journal_path).unwrap();
    assert_eq!(recovery.events.len(), report.events.len());
    let film = frames(&recovery.events, &Policy::builtin()).unwrap();
    assert_eq!(film.len(), recovery.events.len());
    // Frame parity on every frame: the film cannot drift from the fold.
    for (index, frame) in film.iter().enumerate() {
        let state = playbook_state::fold(
            &recovery.events[..=index],
            &Policy::builtin(),
            playbook_state::ApplyMode::Audit,
        )
        .unwrap();
        assert_eq!(
            frame.snapshot,
            PresentationSnapshot::from_state(&state),
            "frame {} drifted",
            frame.index
        );
    }
    // The final frame knows the drive's ending.
    let last: &Frame = film.last().unwrap();
    assert!(last.snapshot.scorebug.status != "live" || report.disposition.is_some());
}
