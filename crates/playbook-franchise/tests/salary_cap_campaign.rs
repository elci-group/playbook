//! The cap is a constitutional boundary, not a report.
//!
//! v0.2.6 verification of real franchise economics, in four articles:
//!
//! * observed spend and reserved capacity are distinct ledgers — measured
//!   cost books what the provider reported; an unknown dimension reserves
//!   the envelope ceiling instead of booking a fictional zero;
//! * a budget only shrinks inside a possession — remaining capacity is
//!   saturating and monotonic, never a refund;
//! * the franchise persists its cap state — roster, availability and
//!   lifetime grants survive a reopen exactly;
//! * the boundary is enforced at the envelope, before dispatch — an
//!   invocation that widens its play's ceiling is refused outright, and
//!   benched or unknown personnel are refused at the gate.
//!
//! Reservation model implemented by `CostReport::reserved`: only the token
//! and monetary dimensions are lifted to the envelope ceiling when unknown;
//! every other dimension passes through as measured. This file pins that
//! model so it cannot silently become zero-booking.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use playbook_core::*;
use playbook_franchise::*;
use playbook_live::{CostReport, ExecutionEnvelope};
use playbook_state::started_budget;

/// Names one budget dimension and sets it to an exact value.
type DimensionSetter = (&'static str, fn(&mut DriveBudget, u64));

/// The eight constitutional dimensions, in declarable order.
fn dimensions(budget: DriveBudget) -> [(&'static str, u64); 8] {
    [
        ("tokens", budget.tokens),
        ("monetary_cost_micros", budget.monetary_cost_micros),
        ("wall_clock_ms", budget.wall_clock_ms),
        ("tool_calls", budget.tool_calls),
        ("inference_calls", budget.inference_calls),
        ("retry_count", budget.retry_count),
        ("context_capacity", budget.context_capacity),
        ("provider_quota", budget.provider_quota),
    ]
}

/// Reserved capacity must dominate observed spend in every dimension.
fn reserved_covers_observed(reserved: DriveBudget, observed: DriveBudget) {
    for ((name, reserved), (_, observed)) in
        dimensions(reserved).into_iter().zip(dimensions(observed))
    {
        assert!(
            reserved >= observed,
            "reserved {name} fell below observed spend ({reserved} < {observed})"
        );
    }
}

fn play_ceiling() -> DriveBudget {
    DriveBudget {
        tokens: 10_000,
        monetary_cost_micros: 2_000_000,
        wall_clock_ms: 120_000,
        tool_calls: 20,
        inference_calls: 10,
        retry_count: 5,
        context_capacity: 200_000,
        provider_quota: 1_000,
    }
}

/// A complete, otherwise-honest live envelope for a read-only play.
fn cap_envelope() -> ExecutionEnvelope {
    ExecutionEnvelope {
        execution_id: ExecutionId::new("exec-cap-1").unwrap(),
        execution: PlayExecution {
            drive_id: DriveId::new("cap-drive").unwrap(),
            play_id: PlayId::new("play-cap-1").unwrap(),
            play_kind: PlayKind::RunDeterministicTest,
            down: Down::FirstDown,
            operation: "test.unit".into(),
            capability_id: CapabilityId::new("elci.cargo").unwrap(),
            capability_generation: 1,
            personnel_id: PersonnelId::new("cap-starter-v1").unwrap(),
            inputs: serde_json::json!({}),
            expected_evidence: vec![EvidenceClass::TestReport],
            expected_ytg_reduction_milli: 4_000,
            survey: false,
            budget_ceiling: play_ceiling(),
            timeout_ms: 5_000,
            retry_policy: RetryPolicy::NoRetry,
            side_effect: SideEffectClass::ReadOnly,
            idempotency_key: Some("play-cap-1".into()),
        },
        personnel_id: PersonnelId::new("cap-starter-v1").unwrap(),
        objective_id: None,
        allowed_operations: vec!["test.unit".into()],
        allowed_side_effects: vec![SideEffectClass::ReadOnly],
        expected_result_schema: "test_report/1".into(),
        expected_evidence: vec![EvidenceClass::TestReport],
        budget_envelope: DriveBudget {
            tokens: 5_000,
            monetary_cost_micros: 1_000_000,
            wall_clock_ms: 60_000,
            tool_calls: 10,
            inference_calls: 5,
            retry_count: 2,
            context_capacity: 100_000,
            provider_quota: 500,
        },
        timeout_ms: 10_000,
        working_directory: Some("/repo".into()),
    }
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A durable franchise on disk with a cap of `grants` full possessions.
struct Fixture {
    directory: PathBuf,
    franchise: Franchise,
}

impl Fixture {
    fn new(grants: u64) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "franchise-cap-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let mut cap = DriveBudget::zero();
        for _ in 0..grants {
            cap = cap.checked_add(started_budget(1_000, 1_000)).unwrap();
        }
        let franchise = Franchise::create(
            &directory.join("team.json"),
            FranchiseId::new("cap-team").unwrap(),
            cap,
        )
        .unwrap();
        Self {
            directory,
            franchise,
        }
    }

    fn acquire(&mut self, id: &str, kind: PersonnelKind, availability: Availability, reason: &str) {
        let personnel = playbook_sim::person(id, kind, 100, None);
        self.franchise
            .acquire(
                RosterMember {
                    personnel,
                    position: "engineering".into(),
                    provider: Some("cap-fixture".into()),
                    model: None,
                    availability,
                },
                reason,
            )
            .unwrap();
    }

    fn objective(&mut self, id: &str) {
        self.franchise
            .declare_objective(Objective {
                id: ObjectiveId::new(id).unwrap(),
                task: playbook_sim::task_contract(TaskIntegrity::Sound),
                gates: playbook_sim::standard_gates().unwrap(),
                drive_id: None,
            })
            .unwrap();
    }

    fn start(&mut self, objective: &str, drive: &str) -> DriveId {
        let id = DriveId::new(drive).unwrap();
        self.franchise
            .start_drive(
                &ObjectiveId::new(objective).unwrap(),
                id.clone(),
                started_budget(1_000, 1_000),
                1_000,
            )
            .unwrap();
        id
    }

    fn reopen(&mut self) {
        self.franchise = Franchise::open(&self.directory.join("team.json")).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).ok();
    }
}

#[test]
fn observed_spend_and_reserved_capacity_are_distinct_ledgers() {
    let envelope = cap_envelope();
    let spend = DriveBudget {
        tokens: 4_000,
        monetary_cost_micros: 250_000,
        wall_clock_ms: 21_000,
        tool_calls: 3,
        inference_calls: 2,
        retry_count: 1,
        context_capacity: 40_000,
        provider_quota: 300,
    };

    // A fully measured report books exactly the reported spend, no more.
    let measured = CostReport::measured(spend, true);
    let booked = measured.reserved(&envelope);
    assert_eq!(booked, spend);

    // The same invocation with unknown dimensions: tokens and money are
    // reserved at the envelope ceiling; the knowable dimensions stay
    // measured.
    let unknown = CostReport::partially_unknown(21_000, 3);
    let reserved = unknown.reserved(&envelope);
    assert_eq!(reserved.tokens, envelope.budget_envelope.tokens);
    assert_eq!(
        reserved.monetary_cost_micros,
        envelope.budget_envelope.monetary_cost_micros
    );
    assert_eq!(reserved.wall_clock_ms, 21_000);
    assert_eq!(reserved.tool_calls, 3);

    // The ledgers never merge: unknown dimensions sit strictly above the
    // zeros the report carries for them.
    assert!(reserved.tokens > unknown.measured.tokens);
    assert!(reserved.monetary_cost_micros > unknown.measured.monetary_cost_micros);

    // Reserved capacity dominates observed spend everywhere, on both books.
    reserved_covers_observed(booked, measured.measured);
    reserved_covers_observed(reserved, unknown.measured);
}

#[test]
fn unknown_cost_is_never_zero() {
    let envelope = cap_envelope();
    let report = CostReport::partially_unknown(12_345, 4);
    assert!(!report.tokens_known);
    assert!(!report.monetary_cost_known);
    assert!(!report.provider_reported);

    // The report's own ledger carries zeros for what it could not measure.
    // Booking them would be fiction; the authority books the ceiling.
    assert_eq!(report.measured.tokens, 0);
    assert_eq!(report.measured.monetary_cost_micros, 0);
    let reserved = report.reserved(&envelope);
    assert_eq!(reserved.tokens, envelope.budget_envelope.tokens);
    assert_eq!(
        reserved.monetary_cost_micros,
        envelope.budget_envelope.monetary_cost_micros
    );
    assert!(reserved.tokens > 0);
    assert!(reserved.monetary_cost_micros > 0);

    // Wall clock and tool calls stay measured, never inflated.
    assert_eq!(reserved.wall_clock_ms, 12_345);
    assert_eq!(reserved.tool_calls, 4);
}

#[test]
fn budget_remaining_is_monotonic_inside_a_possession() {
    let possession = started_budget(2_000, 20);
    let steps = [
        DriveBudget {
            tokens: 200,
            monetary_cost_micros: 100_000,
            wall_clock_ms: 5_000,
            tool_calls: 2,
            inference_calls: 1,
            retry_count: 0,
            context_capacity: 10_000,
            provider_quota: 2,
        },
        DriveBudget {
            tokens: 1_000,
            monetary_cost_micros: 500_000,
            wall_clock_ms: 30_000,
            tool_calls: 50,
            inference_calls: 10,
            retry_count: 1,
            context_capacity: 50_000,
            provider_quota: 10,
        },
        DriveBudget {
            tokens: 1_999,
            monetary_cost_micros: 999_999,
            wall_clock_ms: 59_999,
            tool_calls: 99,
            inference_calls: 19,
            retry_count: 5,
            context_capacity: 99_999,
            provider_quota: 19,
        },
        // The overrun: beyond the ceiling in every dimension.
        DriveBudget {
            tokens: 2_001,
            monetary_cost_micros: 1_000_001,
            wall_clock_ms: 60_001,
            tool_calls: 101,
            inference_calls: 21,
            retry_count: 6,
            context_capacity: 100_001,
            provider_quota: 21,
        },
    ];

    // Growing consumption can only shrink what remains, dimension by
    // dimension. The arithmetic saturates at the zero floor.
    let mut previous = dimensions(possession);
    for (step_number, consumed) in steps.iter().enumerate() {
        let current = dimensions(possession.remaining(*consumed));
        for index in 0..current.len() {
            let (name, now) = current[index];
            let (_, was) = previous[index];
            assert!(
                now <= was,
                "possession step {step_number} grew the {name} remaining ({now} > {was})"
            );
        }
        previous = current;
    }

    // The overrun leaves nothing, and further overrun refunds nothing.
    let overrun = steps[3];
    let exhausted = possession.remaining(overrun);
    assert_eq!(exhausted, DriveBudget::zero());
    assert_eq!(exhausted.remaining(overrun), exhausted);
}

#[test]
fn franchise_persistence_round_trips_the_cap_state() {
    let mut f = Fixture::new(3);
    f.acquire(
        "starter",
        PersonnelKind::DeterministicProcedure,
        Availability::Ready,
        "opening day acquisition",
    );
    f.acquire(
        "substitute",
        PersonnelKind::Tool,
        Availability::Benched,
        "depth chart",
    );
    f.objective("deliver");
    f.start("deliver", "drive");
    let before = f.franchise.snapshot();

    // Lifetime grants reserve the cap before any dispatch.
    assert!(before.allocated.fits_within(before.salary_cap));
    assert_eq!(before.allocated, started_budget(1_000, 1_000));
    let expected_free = started_budget(1_000, 1_000)
        .checked_add(started_budget(1_000, 1_000))
        .unwrap();
    assert_eq!(before.record().unwrap().cap_unallocated, expected_free);
    assert_eq!(before.roster["starter"].availability, Availability::Ready);
    assert_eq!(
        before.roster["substitute"].availability,
        Availability::Benched
    );
    for (index, record) in before.management.iter().enumerate() {
        assert_eq!(record.sequence, index as u64 + 1);
    }

    f.reopen();
    let after = f.franchise.snapshot();
    assert_eq!(after, before);
    assert_eq!(after.salary_cap, before.salary_cap);
    assert_eq!(after.allocated, before.allocated);
    assert_eq!(after.roster, before.roster);
    assert_eq!(after.roster["starter"].availability, Availability::Ready);
    assert_eq!(
        after.roster["substitute"].availability,
        Availability::Benched
    );
    assert_eq!(after.management, before.management);
    assert_eq!(after.record().unwrap(), before.record().unwrap());
    assert_eq!(after.record().unwrap().cap_unallocated, expected_free);
}

#[test]
fn availability_gating_refuses_benched_personnel() {
    let mut f = Fixture::new(2);
    f.acquire(
        "starter",
        PersonnelKind::DeterministicProcedure,
        Availability::Ready,
        "opening day acquisition",
    );
    f.acquire(
        "substitute",
        PersonnelKind::Tool,
        Availability::Ready,
        "depth chart",
    );
    f.objective("deliver");
    let id = f.start("deliver", "drive");
    let player = PersonnelId::new("starter").unwrap();
    let mut plane = playbook_elci::FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();

    // An unknown member cannot be activated at all, and the attempt
    // changes nothing.
    let untouched = f.franchise.snapshot();
    assert!(f
        .franchise
        .set_availability(
            &PersonnelId::new("free-agent").unwrap(),
            Availability::Ready,
            "never acquired"
        )
        .is_err());
    assert_eq!(f.franchise.snapshot(), untouched);

    // A benched player is refused at dispatch, and the refusal writes
    // nothing.
    f.franchise
        .set_availability(&player, Availability::Benched, "coach benches the starter")
        .unwrap();
    let benched = f.franchise.snapshot();
    assert!(f
        .franchise
        .step(&id, Some(&player), &mut plane, 1_100)
        .is_err());
    assert_eq!(f.franchise.snapshot(), benched);

    // Activation is a recorded management action; the player then plays.
    f.franchise
        .set_availability(&player, Availability::Ready, "coach activates the starter")
        .unwrap();
    let decision = f
        .franchise
        .step(&id, Some(&player), &mut plane, 1_100)
        .unwrap();
    assert_eq!(decision.selected_personnel, Some(player.clone()));

    // Deactivation removes the player again.
    f.franchise
        .set_availability(&player, Availability::Unavailable, "injured on the play")
        .unwrap();
    assert!(f
        .franchise
        .step(&id, Some(&player), &mut plane, 1_200)
        .is_err());

    // Every availability transition carries its reason in the management
    // history, in order, under the player's own subject.
    let history = f.franchise.snapshot();
    let transitions: Vec<(&str, &str)> = history
        .management
        .iter()
        .filter(|record| record.subject == "starter")
        .map(|record| (record.action.as_str(), record.reason.as_str()))
        .collect();
    assert_eq!(
        transitions,
        vec![
            ("acquire", "opening day acquisition"),
            ("bench", "coach benches the starter"),
            ("activate", "coach activates the starter"),
            ("unavailable", "injured on the play"),
        ]
    );
}

#[test]
fn an_overcommitted_envelope_is_refused_before_it_bites() {
    let policy_effects = [SideEffectClass::ReadOnly];

    // An envelope that narrows the play's ceiling is admissible.
    cap_envelope().validate(&policy_effects).unwrap();

    // The boundary is inclusive: exactly the ceiling is still inside the cap.
    let mut boundary = cap_envelope();
    boundary.budget_envelope = boundary.execution.budget_ceiling;
    boundary.validate(&policy_effects).unwrap();

    // One dimension at a time, an envelope that would widen the play's
    // ceiling is refused at validation — before dispatch, before any cost
    // can bite.
    let setters: [DimensionSetter; 8] = [
        ("tokens", |budget, value| budget.tokens = value),
        ("monetary_cost_micros", |budget, value| {
            budget.monetary_cost_micros = value
        }),
        ("wall_clock_ms", |budget, value| {
            budget.wall_clock_ms = value
        }),
        ("tool_calls", |budget, value| budget.tool_calls = value),
        ("inference_calls", |budget, value| {
            budget.inference_calls = value
        }),
        ("retry_count", |budget, value| budget.retry_count = value),
        ("context_capacity", |budget, value| {
            budget.context_capacity = value
        }),
        ("provider_quota", |budget, value| {
            budget.provider_quota = value
        }),
    ];
    for ((dimension, set), (ceiling_dimension, ceiling_value)) in
        setters.into_iter().zip(dimensions(play_ceiling()))
    {
        assert_eq!(dimension, ceiling_dimension);
        let mut greedy = cap_envelope();
        set(&mut greedy.budget_envelope, ceiling_value + 1);
        assert_eq!(
            greedy.validate(&policy_effects),
            Err(PlaybookError::BudgetCeiling),
            "widening {dimension} past the play ceiling must be refused at the envelope boundary"
        );
    }
}
