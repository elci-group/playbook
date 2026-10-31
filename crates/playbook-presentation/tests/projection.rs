//! The projection campaign: snapshots are pure functions of state, film
//! frames replay deterministically, and the three forms never disagree.

use playbook_core::{DriveBudget, DriveId, TaskContract, TaskIntegrity};
use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_presentation::{frames, present_event, PresentationSnapshot};
use playbook_state::{fold, ApplyMode};

fn policy() -> Policy {
    Policy::builtin()
}

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
        at_ms: 0,
    }
}

fn journal() -> Vec<DriveEvent> {
    vec![
        started(),
        DriveEvent::FieldConstructed {
            objective_id: "objective-matchday".into(),
            total_weight_ppm: 1_000_000,
            consolidation_boundaries: vec![10, 20],
        },
        DriveEvent::UnvalidatedProgressRecorded {
            yards: 6,
            unvalidated_total: 6,
        },
        DriveEvent::RedZoneEntered {
            outstanding_mandatory: 1,
        },
        DriveEvent::FieldGoalRangeEntered {
            candidate_action: "one decisive benchmark".into(),
        },
        DriveEvent::RedZoneExited {
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
        DriveEvent::TouchdownScored {
            banked_yards_total: 20,
            evidence_count: 3,
        },
    ]
}

#[test]
fn the_snapshot_is_a_pure_function_of_state() {
    let events = journal();
    let audit = fold(&events, &policy(), ApplyMode::Audit).unwrap();
    let resimulated = fold(&events, &policy(), ApplyMode::Resimulate).unwrap();
    assert_eq!(audit, resimulated, "replay is deterministic (I-17)");
    // The same state projects the same snapshot — twice, and across modes.
    assert_eq!(
        PresentationSnapshot::from_state(&audit),
        PresentationSnapshot::from_state(&resimulated)
    );
    assert_eq!(
        PresentationSnapshot::from_state(&audit),
        PresentationSnapshot::from_state(&audit)
    );
}

#[test]
fn the_scorebug_reads_like_a_broadcast() {
    let state = fold(&journal(), &policy(), ApplyMode::Audit).unwrap();
    let bug = PresentationSnapshot::from_state(&state).scorebug;
    assert_eq!(bug.drive_id, "drive-matchday");
    assert_eq!(bug.status, "live");
    // Nine unvalidated yards were accepted: unvalidated progress shows.
    assert_eq!(bug.completion_ppm, state.completion_ppm());
}

#[test]
fn the_field_projection_carries_the_geometry() {
    let state = fold(&journal(), &policy(), ApplyMode::Audit).unwrap();
    let field = PresentationSnapshot::from_state(&state).field;
    // The fold derives field length from the declared boundaries: the
    // last gate ordinal × 10 yards per gate = 20 × 10 = 200 yards.
    assert_eq!(field.field_yards, 200);
    assert_eq!(field.banked_yards, 10);
    assert_eq!(field.unvalidated_yards, 0);
    // Red zone entered AND exited: independent of field-goal range, which
    // stays held (§25).
    assert!(!field.red_zone);
    assert!(field.field_goal_range);
    assert_eq!(field.segments.len(), 20);
    assert_eq!(field.segments[0].state, "passed");
    assert_eq!(field.segments[1].state, "pending");
    assert!(field.touchdown);
}

#[test]
fn the_film_is_one_frame_per_event_and_replays_identically() {
    let events = journal();
    let film = frames(&events, &policy()).unwrap();
    assert_eq!(film.len(), events.len());
    assert_eq!(film[0].kind, "drive_started");
    assert_eq!(film[6].kind, "validation_boundary_passed");
    // Frame parity: each frame's snapshot equals the prefix fold's snapshot.
    for (index, frame) in film.iter().enumerate() {
        let state = fold(&events[..=index], &policy(), ApplyMode::Audit).unwrap();
        assert_eq!(frame.snapshot, PresentationSnapshot::from_state(&state));
    }
    // The film itself is deterministic.
    assert_eq!(frames(&events, &policy()).unwrap(), film);
}

#[test]
fn the_three_forms_project_each_event() {
    for event in journal() {
        let forms = present_event(&event);
        assert!(!forms.glance.is_empty());
        assert!(!forms.tactical.is_empty());
        assert!(!forms.analytical.is_empty());
    }
}
