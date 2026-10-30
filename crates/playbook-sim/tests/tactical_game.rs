//! The Tactical Expansion acceptance benchmark (§20, §26).
//!
//! One roster, one objective, six phases of a problem that changes shape.
//! Each phase is classified into a `TacticalContext`, the frozen tactical
//! pipeline (admission → composition → formation planning) recommends the
//! organisational shape, and the decision is journalled through the real
//! drive fold. The release definition: same roster, different game state →
//! different tactical formation → different personnel arrangement →
//! different information topology → different measured result.

use playbook_core::{PersonnelId, TacticalId};
use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode};
use playbook_tactics::domain::{RiskBand, TacticalClass, TacticalContext, TacticalTrigger};
use playbook_tactics::{compose, Admission};

fn context(breadth: u32, coupling: u32, risk: RiskBand) -> TacticalContext {
    TacticalContext {
        problem_breadth_ppm: breadth,
        domain_coupling_ppm: coupling,
        provider_instability_ppm: 100,
        coherence_debt: 100,
        repeated_failures: 0,
        risk_band: risk,
        divergence_observed: false,
        critical_domain: None,
        ytg_milli: 10_000,
        budget_remaining_ppm: 900_000,
        short_ytg: false,
        at_ms: 0,
    }
}

fn person(id: &str) -> playbook_core::PersonnelId {
    playbook_core::PersonnelId::new(id).unwrap()
}

fn started() -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: playbook_core::DriveId::new("drive-tactical").unwrap(),
        task: playbook_core::TaskContract {
            statement: "hold one objective across six changing phases".into(),
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

fn roster() -> Vec<PersonnelId> {
    vec![
        person("agent-a"),
        person("agent-b"),
        person("agent-c"),
        person("agent-d"),
    ]
}

/// PHASE 1 — a broad, loosely coupled, unknown problem: Spread. The
/// formation planner deploys broad parallel ownership with minimal context
/// overlap.
#[test]
fn phase_one_spreads_broad_loose_problems() {
    let phase = context(800, 200, RiskBand::Low);
    let spread = playbook_tactics::formation::formation_directive(
        playbook_tactics::formation::TacticalFormation::Spread,
        &phase,
    );
    let people: Vec<PersonnelId> = roster();
    let shape = playbook_tactics::formation::plan(&phase, &people, None).unwrap();
    shape.validate().unwrap();
    // Admission agrees the context calls for it, and the arrangement is
    // genuinely broad: every deployed domain is sole-owned.
    assert!(matches!(
        spread.evaluate_admission(&phase, 100),
        Admission::Admitted
    ));
    assert!(shape
        .assignments
        .iter()
        .all(|assignment| assignment.mode == playbook_tactics::domain::OwnershipMode::Sole));
    let profile = playbook_tactics::formation::profile(
        playbook_tactics::formation::TacticalFormation::Spread,
    );
    assert!(profile.max_concurrent >= 3);
    assert!(profile.context_overlap_ppm <= 100);
}

/// PHASE 2 — a critical domain is discovered: concentration. The same
/// roster deploys numerical superiority on the blocker instead of breadth.
#[test]
fn phase_two_concentrates_on_the_discovered_domain() {
    let mut phase = context(400, 300, RiskBand::Elevated);
    phase.critical_domain = Some("dependency-graph".into());
    let people: Vec<PersonnelId> = roster();
    let shape =
        playbook_tactics::formation::plan(&phase, &people, Some("dependency-graph")).unwrap();
    shape.validate().unwrap();
    // The critical domain carries the concentration: more owners than any
    // other domain.
    let concentrated = shape
        .assignments
        .iter()
        .find(|assignment| assignment.domain == "dependency-graph")
        .expect("the critical domain is owned");
    assert!(
        concentrated.personnel.len() >= 2,
        "trips: at least two personnel on the discovered blocker"
    );
    let trips = playbook_tactics::formation::formation_directive(
        playbook_tactics::formation::TacticalFormation::Trips,
        &phase,
    );
    assert_eq!(trips.class, TacticalClass::Formation);
}

/// PHASE 3 — divergence between concurrent personnel: the formation
/// tightens into a cooperative micro-team with shared context (Bunch), and
/// the information topology actually changes.
#[test]
fn phase_three_bunches_divergent_personnel_into_shared_context() {
    let mut phase = context(400, 600, RiskBand::Guarded);
    phase.divergence_observed = true;
    phase.coherence_debt = 300;
    let people: Vec<PersonnelId> = roster();
    let spread_shape =
        playbook_tactics::formation::plan(&context(800, 200, RiskBand::Low), &people, None)
            .unwrap();
    let bunch_shape = playbook_tactics::formation::plan(&phase, &people, None).unwrap();
    // The release definition, first clause: same roster, different state,
    // different arrangement.
    assert_ne!(spread_shape, bunch_shape);
    let bunch_profile =
        playbook_tactics::formation::profile(playbook_tactics::formation::TacticalFormation::Bunch);
    let spread_profile = playbook_tactics::formation::profile(
        playbook_tactics::formation::TacticalFormation::Spread,
    );
    // Different information topology: Bunch shares context, Spread does not.
    assert!(bunch_profile.context_overlap_ppm > spread_profile.context_overlap_ppm);
}

/// PHASE 4 — high-risk modification: concentration doctrine (Heavy) with a
/// protection-heavy profile outranks breadth.
#[test]
fn phase_four_protects_high_risk_work() {
    let phase = context(300, 800, RiskBand::Critical);
    let heavy = playbook_tactics::formation::formation_directive(
        playbook_tactics::formation::TacticalFormation::Heavy,
        &phase,
    );
    let max_protect = playbook_tactics::composition::compatibility("heavy", "max_protect");
    assert_eq!(
        max_protect,
        playbook_tactics::Compatibility::Allowed,
        "concentration doctrines stack"
    );
    assert!(matches!(
        heavy.evaluate_admission(&phase, 100),
        Admission::Admitted
    ));
    // The protection deltas point the right way: Heavy protects, Spread
    // spreads.
    assert!(heavy.expected_effect.protection_delta_ppm >= 700);
    let spread = playbook_tactics::formation::formation_directive(
        playbook_tactics::formation::TacticalFormation::Spread,
        &phase,
    );
    assert!(spread.expected_effect.breadth_delta_ppm > heavy.expected_effect.breadth_delta_ppm);
    // And the conflict matrix drops the later exclusive directive: the plan
    // keeps the first-listed concentration call and refuses to run both
    // shapes at once.
    let plan = compose(&[heavy, spread]).expect("exclusive pairs compose by dropping");
    assert_eq!(plan.directives.len(), 1);
    assert_eq!(plan.directives[0].name, "heavy");
}

/// PHASE 5 — new runtime information arrives before dispatch: the audible.
/// The plan is replaced lawfully — and after the snap it is not an audible
/// at all.
#[test]
fn phase_five_audibles_before_dispatch_but_never_after() {
    let observed = context(200, 900, RiskBand::Guarded);
    let candidates: Vec<(String, TacticalTrigger)> = vec![
        ("trips".into(), TacticalTrigger::CriticalDomainNamed),
        (
            "bunch".into(),
            TacticalTrigger::DomainCouplingAtLeast { ppm: 600 },
        ),
    ];
    // Pre-dispatch: the coupling observation fires the bunch candidate.
    let decision = playbook_tactics::adjustment::call_audible("spread", &observed, &candidates);
    match decision {
        playbook_tactics::adjustment::AudibleDecision::AudibleTo { new_tactic, .. } => {
            assert_eq!(new_tactic, "bunch");
        }
        other => panic!("pre-dispatch observation must audible: {other:?}"),
    }
    // After the snap (repeated failures recorded): the same observation
    // keeps the plan, because an audible after execution began is not an
    // audible.
    let mut after_snap = observed;
    after_snap.repeated_failures = 1;
    assert!(matches!(
        playbook_tactics::adjustment::call_audible("spread", &after_snap, &candidates),
        playbook_tactics::adjustment::AudibleDecision::KeepPlan
    ));
}

/// PHASE 6 — fragile release state: the conservative doctrine takes over,
/// small validated stages only.
#[test]
fn phase_six_prevents_large_changes_in_fragile_states() {
    let mut phase = context(200, 700, RiskBand::Critical);
    phase.budget_remaining_ppm = 80_000;
    let state = playbook_tactics::conservative::classify(&phase);
    assert_eq!(state, playbook_tactics::conservative::RiskState::Critical);
    let envelope = playbook_tactics::conservative::envelope(state);
    // Prevent blocks large unverified changes outright.
    let big_change = playbook_tactics::conservative::ChangeRequest::Patch {
        files: 40,
        lines: 4_000,
    };
    assert!(matches!(
        playbook_tactics::conservative::adjudicate(&big_change, &envelope),
        playbook_tactics::conservative::ChangeVerdict::Refused { .. }
    ));
    // Make Them Drive: the same work, staged into validated increments.
    let stages = playbook_tactics::conservative::drive_stages(
        40,
        4_000,
        envelope.max_files,
        envelope.max_lines,
    )
    .expect("a real patch stages honestly");
    let staged = playbook_tactics::conservative::ChangeRequest::StagedPatch {
        stages,
        files_per_stage: envelope.max_files,
        lines_per_stage: envelope.max_lines,
    };
    assert!(matches!(
        playbook_tactics::conservative::adjudicate(&staged, &envelope),
        playbook_tactics::conservative::ChangeVerdict::Admitted
    ));
}

/// The full campaign: the six phases play through the real journal fold,
/// the overreaction machinery fires after three repeated Spreads, the
/// constraint names its counter, and every tactical decision replays.
#[test]
fn the_tactical_campaign_journals_replays_and_counters_overreaction() {
    let mut events: Vec<DriveEvent> = vec![started()];
    let policy = Policy::builtin();
    let mut at_ms = 1_000u64;

    // Phase 1–2: Spread called three times; each completes with measured
    // YTG movement.
    for phase in 0..3 {
        at_ms += 100;
        events.push(DriveEvent::TacticAdmitted {
            tactic: "spread".into(),
            at_ms,
        });
        at_ms += 100;
        events.push(DriveEvent::TacticCompleted {
            tactic: "spread".into(),
            // The doctrine's signature: repeated calls, degrading returns —
            // only the first Spread still earns its keep.
            success: phase == 0,
            ytg_before_milli: 10_000 - phase * 500,
            ytg_after_milli: 10_000 - (phase + 1) * 500,
            tokens_spent: 1_000,
            at_ms,
        });
    }

    // The journal is the official: three Spreads are on the record.
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(state.tactics().consecutive("spread"), 3);
    assert_eq!(state.tactics().completed_total(), 3);

    // PHASE 2bis — the constraint doctrine: the third Spread's degraded
    // result is the overreaction signature; the constraint mechanism names
    // the counter, and the journal records it.
    let history_rows: Vec<playbook_tactics::sequencing::HistoryRow> = state
        .tactics()
        .history()
        .iter()
        .filter_map(|record| match &record.kind {
            playbook_state::TacticRecordKind::Completed {
                success,
                ytg_before_milli,
                ytg_after_milli,
                ..
            } => Some(playbook_tactics::sequencing::HistoryRow {
                tactic: record.tactic.clone(),
                success: *success,
                ytg_before_milli: *ytg_before_milli,
                ytg_after_milli: *ytg_after_milli,
                at_ms: record.at_ms,
            }),
            _ => None,
        })
        .collect();
    let mut history = playbook_tactics::sequencing::TacticalHistory::empty();
    for row in history_rows {
        history.record(row);
    }
    let overreaction = playbook_tactics::sequencing::detect_overreaction(&history, 3, 3)
        .expect("three degrading spreads are an overreaction");
    assert_eq!(overreaction.tactic, "spread");
    let counter = playbook_tactics::sequencing::constraint_for(&overreaction);
    at_ms += 100;
    events.push(DriveEvent::TacticCountered {
        tactic: "spread".into(),
        counter: counter.to_string(),
        at_ms,
    });

    // PHASE 3–6 decisions are journalled too: the audible, the
    // concentration doctrine, the conservative stage gate.
    at_ms += 100;
    events.push(DriveEvent::TacticAdmitted {
        tactic: "bunch".into(),
        at_ms,
    });
    at_ms += 100;
    events.push(DriveEvent::TacticAdmitted {
        tactic: "max_protect".into(),
        at_ms,
    });
    at_ms += 100;
    events.push(DriveEvent::TacticAdmitted {
        tactic: "make_them_drive".into(),
        at_ms,
    });
    at_ms += 100;
    events.push(DriveEvent::TacticRejected {
        tactic: "verticals".into(),
        reason_code: "conflicts_with_prevent".into(),
        at_ms,
    });
    at_ms += 100;
    events.push(DriveEvent::TacticFailed {
        tactic: "bunch".into(),
        failure_mode: "shared_context_overload".into(),
        at_ms,
    });
    at_ms += 100;
    events.push(DriveEvent::TacticCompleted {
        tactic: "make_them_drive".into(),
        success: true,
        ytg_before_milli: 8_500,
        ytg_after_milli: 2_000,
        tokens_spent: 2_000,
        at_ms,
    });

    let policy = Policy::builtin();
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(
        &fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        &state,
        "tactical decisions replay"
    );
    let ledger = state.tactics();
    assert_eq!(ledger.admitted_total(), 6);
    assert_eq!(ledger.rejected_total(), 1);
    assert_eq!(ledger.completed_total(), 4);
    assert_eq!(ledger.failed_total(), 1);
    assert_eq!(ledger.countered_total(), 1);
    // Tactical memory: the recent window reads oldest first — the final
    // three records are the rejected Verticals, the failed Bunch, and the
    // completing Make Them Drive.
    let recent = ledger.recent(3);
    assert_eq!(recent[0].tactic, "verticals");
    assert_eq!(recent[1].tactic, "bunch");
    assert_eq!(recent[2].tactic, "make_them_drive");
    assert!(matches!(
        &recent[1].kind,
        playbook_state::TacticRecordKind::Failed { failure_mode }
            if failure_mode == "shared_context_overload"
    ));
    let _: Option<TacticalId> = None;
}
