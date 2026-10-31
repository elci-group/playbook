//! The field-geometry replay campaign: the 9+1 rhythm, banked progress,
//! causal adjustments, and the kick outcome set — every state transition
//! replay-deterministic (I-17).

use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode};

fn policy() -> Policy {
    Policy::builtin()
}

fn started() -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: playbook_core::DriveId::new("drive-field").unwrap(),
        task: playbook_core::TaskContract {
            statement: "hold the field under the 9+1 rhythm".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        budget: playbook_core::DriveBudget {
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

/// The 9+1 rhythm: nine productive yards accepted, the tenth ordinary yard
/// refused, the validation boundary passed, and the segment banked.
#[test]
fn the_nine_plus_one_rhythm_banks_progress_at_the_boundary() {
    let events = vec![
        started(),
        DriveEvent::FieldConstructed {
            objective_id: "objective-field".into(),
            total_weight_ppm: 1_000_000,
            consolidation_boundaries: vec![10, 20, 30],
        },
        // Nine productive yards in three steps.
        DriveEvent::UnvalidatedProgressRecorded {
            yards: 3,
            unvalidated_total: 3,
        },
        DriveEvent::UnvalidatedProgressRecorded {
            yards: 3,
            unvalidated_total: 6,
        },
        DriveEvent::UnvalidatedProgressRecorded {
            yards: 3,
            unvalidated_total: 9,
        },
        // The validation boundary passes: the segment banks.
        DriveEvent::ValidationBoundaryPassed {
            yard: 10,
            evidence_classes: vec!["test".into()],
        },
        DriveEvent::ProgressBanked {
            banked_yard: 10,
            banked_yards_total: 10,
        },
    ];
    // The tenth ordinary productive yard is refused by the geometry:
    // journalled as an attempted overstep that the fold rejects.
    let mut overstepped = events.clone();
    overstepped.push(DriveEvent::UnvalidatedProgressRecorded {
        yards: 1,
        unvalidated_total: 10,
    });
    let policy = policy();
    // The overstep is refused at the fold: the ninth-yard ceiling is real.
    assert!(fold(&overstepped, &policy, ApplyMode::Audit).is_err());
    // Without the overstep, the rhythm replays cleanly.
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(state.field().position.banked, 10);
    assert_eq!(state.field().position.unvalidated, 0);
    assert_eq!(
        fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        state,
        "the 9+1 rhythm replays identically"
    );
}

/// The kick outcome set: scored, missed, blocked, and turnover each journal
/// deterministically, and the field record carries the outcome.
#[test]
fn kick_outcomes_are_journalled_deterministically() {
    let base = || {
        vec![
            started(),
            DriveEvent::FieldConstructed {
                objective_id: "objective-field".into(),
                total_weight_ppm: 1_000_000,
                consolidation_boundaries: vec![10],
            },
        ]
    };
    for (outcome, expected_kind) in [
        ("score", "kick_scored"),
        ("miss", "kick_missed"),
        ("block", "kick_blocked"),
        ("turnover", "kick_turnover"),
    ] {
        let mut events = base();
        events.push(DriveEvent::KickAttempted {
            personnel: "kicker-a".to_string(),
            validated_frontier_yard: 70,
        });
        match outcome {
            "score" => events.push(DriveEvent::KickScored {
                personnel: "kicker-a".to_string(),
                evidence_count: 2,
            }),
            "miss" => events.push(DriveEvent::KickMissed {
                personnel: "kicker-a".to_string(),
                possession_recoverable: true,
            }),
            "block" => events.push(DriveEvent::KickBlocked {
                personnel: "kicker-a".to_string(),
                reason: "capability unavailable".into(),
            }),
            _ => events.push(DriveEvent::KickTurnover {
                personnel: "kicker-a".to_string(),
                harm: "destructive side effect".into(),
            }),
        }
        let state = fold(&events, &policy(), ApplyMode::Audit).unwrap();
        assert_eq!(
            fold(&events, &policy(), ApplyMode::Resimulate).unwrap(),
            state,
            "{outcome} replays"
        );
        assert!(
            state.integrations().total(expected_kind) > 0,
            "{outcome} is on the journal"
        );
    }
}

/// Causal field adjustments: an offensive penalty extends the field, a
/// defensive penalty contracts it, and both carry provenance (I-06/I-08).
#[test]
fn field_adjustments_are_attributed_and_applied() {
    let events = vec![
        started(),
        DriveEvent::FieldAdjustmentDeclared {
            adjustment_id: "adj-1".into(),
            direction: "harder".into(),
            magnitude_yards: 6,
            cause: "compatibility_constraint_discovered".into(),
            foreseeability: "discovery".into(),
        },
        DriveEvent::FieldAdjustmentDeclared {
            adjustment_id: "adj-2".into(),
            direction: "easier".into(),
            magnitude_yards: 3,
            cause: "reusable_module_found".into(),
            foreseeability: "discovery".into(),
        },
        DriveEvent::OffensivePenaltyApplied {
            adjustment_id: "adj-3".into(),
            magnitude_yards: 2,
            cause: "execution_created_regression".into(),
        },
        DriveEvent::DefensivePenaltyApplied {
            adjustment_id: "adj-4".into(),
            magnitude_yards: 1,
            cause: "existing_module_discovered".into(),
        },
    ];
    let state = fold(&events, &policy(), ApplyMode::Audit).unwrap();
    assert_eq!(
        fold(&events, &policy(), ApplyMode::Resimulate).unwrap(),
        state
    );
    assert_eq!(state.integrations().total("field_adjustment_declared"), 2);
    assert_eq!(state.integrations().total("offensive_penalty_applied"), 1);
    assert_eq!(state.integrations().total("defensive_penalty_applied"), 1);
}

/// Red Zone and field-goal range journal independently and never conflate
/// (§25): both entered, one exited, the other held.
#[test]
fn red_zone_and_field_goal_range_remain_independent() {
    let events = vec![
        started(),
        DriveEvent::RedZoneEntered {
            outstanding_mandatory: 2,
        },
        DriveEvent::FieldGoalRangeEntered {
            candidate_action: "one decisive benchmark".into(),
        },
        // The red zone closes but field-goal range is HELD: the two states
        // are independent (§25).
        DriveEvent::RedZoneExited {
            outstanding_mandatory: 2,
        },
    ];
    let state = fold(&events, &policy(), ApplyMode::Audit).unwrap();
    assert!(!state.field().red_zone_entered);
    assert!(state.field().field_goal_range_entered);
    assert_eq!(
        fold(&events, &policy(), ApplyMode::Resimulate).unwrap(),
        state
    );
}
