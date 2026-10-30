use playbook_core::*;
use playbook_elci::{CapabilityPlane, CapabilitySnapshot, FixturePlane, PlaneResult};
use playbook_franchise::*;
use playbook_state::started_budget;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    directory: PathBuf,
    franchise: Franchise,
}
impl Fixture {
    #[tracing::instrument]
fn new(grants: u64) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "franchise-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let mut cap = DriveBudget::zero();
        for _ in 0..grants {
            cap = cap.checked_add(started_budget(1_000, 1_000)).unwrap();
        }
        let mut franchise = Franchise::create(
            &directory.join("team.json"),
            FranchiseId::new("team").unwrap(),
            cap,
        )
        .unwrap();
        for personnel in playbook_sim::roster() {
            franchise
                .acquire(
                    RosterMember {
                        provider: Some(personnel.id.to_string()),
                        personnel,
                        position: "engineering".into(),
                        model: None,
                        availability: Availability::Ready,
                    },
                    "initial roster",
                )
                .unwrap();
        }
        Self {
            directory,
            franchise,
        }
    }
    #[tracing::instrument]
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
    #[tracing::instrument]
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
    #[tracing::instrument]
fn reopen(&mut self) {
        self.franchise = Franchise::open(&self.directory.join("team.json")).unwrap();
    }
}
impl Drop for Fixture {
    #[tracing::instrument]
fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).ok();
    }
}

#[test]
#[tracing::instrument]
fn vanished_player_is_replaced_without_losing_accepted_progress() {
    let mut f = Fixture::new(2);
    f.objective("ship");
    let replacement =
        playbook_sim::person("substitute", PersonnelKind::DeterministicProcedure, 1, None);
    f.franchise
        .acquire(
            RosterMember {
                personnel: replacement.clone(),
                position: "engineering".into(),
                provider: Some("alternative".into()),
                model: None,
                availability: Availability::Benched,
            },
            "replacement acquired",
        )
        .unwrap();
    let id = f.start("ship", "drive");
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    for at in [1_100, 2_000] {
        f.franchise.step(&id, None, &mut plane, at).unwrap();
    }
    let before = f.franchise.snapshot().drives["drive"].state().unwrap();
    assert!(before.ytg_milli() < 10_000);
    let old = before.personnel().unwrap().id.clone();
    f.franchise
        .set_availability(
            &replacement.id,
            Availability::Ready,
            "bench substitute is ready",
        )
        .unwrap();
    f.franchise
        .set_availability(&old, Availability::Unavailable, "provider disappeared")
        .unwrap();
    f.reopen();
    let recovered = f.franchise.snapshot();
    assert_eq!(recovered.drives["drive"].state().unwrap(), before);
    assert_eq!(recovered.objectives["ship"].drive_id.as_ref(), Some(&id));
    assert!(f
        .franchise
        .step(&id, Some(&old), &mut plane, 3_000)
        .is_err());
    let decision = f
        .franchise
        .step(&id, Some(&replacement.id), &mut plane, 3_000)
        .unwrap();
    assert_eq!(decision.selected_personnel, Some(replacement.id));
    let after = f.franchise.snapshot().drives["drive"].state().unwrap();
    assert!(after.ytg_milli() < before.ytg_milli());
    assert!(before
        .budget_consumed()
        .fits_within(after.budget_consumed()));
    f.reopen();
    assert_eq!(
        f.franchise.snapshot().drives["drive"].state().unwrap(),
        after
    );
}

struct Disappear;
impl CapabilityPlane for Disappear {
    #[tracing::instrument]
fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        FixturePlane::mvp().discover()
    }
    #[tracing::instrument]
fn execute(&mut self, _: &PlayExecution) -> Result<PlaneResult, PlaybookError> {
        panic!("player process disappeared")
    }
}

#[test]
#[tracing::instrument]
fn crash_at_dispatch_is_durable_and_requires_reconciliation_before_substitution() {
    let mut f = Fixture::new(3);
    f.objective("first");
    f.objective("second");
    let id = f.start("first", "first-drive");
    let other = f.start("second", "second-drive");
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    let mut other_plane = FixturePlane::mvp();
    f.franchise.preflight(&other, &mut other_plane).unwrap();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.franchise.step(&id, None, &mut Disappear, 2_000).unwrap();
    }));
    assert!(crashed.is_err());
    f.reopen();
    let state = f.franchise.snapshot().drives["first-drive"]
        .state()
        .unwrap();
    let execution = state.open_execution().unwrap();
    let key = execution.idempotency_key.as_ref().unwrap();
    assert!(matches!(
        f.franchise.step(&id, None, &mut plane, 3_000),
        Err(PlaybookError::Reconcile(_))
    ));
    assert!(f
        .franchise
        .step(
            &other,
            Some(&execution.personnel_id),
            &mut other_plane,
            3_000
        )
        .is_err());
    f.franchise.reconcile(&id, key, false).unwrap();
    f.franchise.step(&id, None, &mut plane, 3_000).unwrap();
    assert!(f.franchise.snapshot().drives["first-drive"]
        .state()
        .unwrap()
        .open_execution()
        .is_none());
}

#[test]
#[tracing::instrument]
fn all_objectives_share_one_cap_and_failed_allocations_are_atomic() {
    let mut f = Fixture::new(1);
    f.objective("first");
    f.objective("second");
    f.start("first", "first-drive");
    let before = f.franchise.snapshot();
    assert_eq!(
        f.franchise.start_drive(
            &ObjectiveId::new("second").unwrap(),
            DriveId::new("second-drive").unwrap(),
            started_budget(1_000, 1_000),
            2_000
        ),
        Err(PlaybookError::BudgetCeiling)
    );
    assert_eq!(before, f.franchise.snapshot());
    f.reopen();
    assert_eq!(before, f.franchise.snapshot());
}

#[test]
#[tracing::instrument]
fn fourth_down_renews_possession_without_resetting_franchise_spend_or_gates() {
    let mut f = Fixture::new(2);
    f.objective("ship");
    let id = f.start("ship", "drive");
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    for at in [1_100, 2_000, 3_000, 4_000] {
        f.franchise.step(&id, None, &mut plane, at).unwrap();
    }
    f.franchise
        .pressure(
            &id,
            DriveBudget {
                provider_quota: 900,
                ..DriveBudget::zero()
            },
            5_000,
        )
        .unwrap();
    let before = f.franchise.snapshot();
    assert_eq!(
        before.drives["drive"].state().unwrap().down(),
        Down::FourthDown
    );
    f.franchise.punt(&id).unwrap();
    f.reopen();
    let contract = playbook_front_office::construct(
        &serde_json::from_str(include_str!("../../../fixtures/front_office.json")).unwrap(),
    )
    .unwrap();
    f.franchise
        .stage_roster(contract, "next contest only")
        .unwrap();
    assert!(f
        .franchise
        .activate_roster("punt is not a roster window")
        .is_err());
    let punted = f.franchise.snapshot();
    let oversized = started_budget(1_000, 1_000)
        .checked_add(started_budget(1_000, 1_000))
        .unwrap();
    assert_eq!(
        f.franchise.receive(
            &id,
            &PersonnelId::new("cargo-toolchain").unwrap(),
            oversized,
            6_000
        ),
        Err(PlaybookError::BudgetCeiling)
    );
    assert_eq!(f.franchise.snapshot(), punted);
    f.franchise
        .receive(
            &id,
            &PersonnelId::new("cargo-toolchain").unwrap(),
            started_budget(1_000, 1_000),
            6_000,
        )
        .unwrap();
    let renewed = f.franchise.snapshot();
    assert_eq!(
        renewed.drives["drive"].frozen_roster,
        punted.drives["drive"].frozen_roster
    );
    assert_eq!(
        renewed.drives["drive"].state().unwrap().ytg_milli(),
        before.drives["drive"].state().unwrap().ytg_milli()
    );
    assert_eq!(renewed.allocated, renewed.salary_cap);
    assert_eq!(
        renewed.record().unwrap().observed_spend,
        before.record().unwrap().observed_spend
    );
    f.franchise.preflight(&id, &mut plane).unwrap();
    f.franchise.step(&id, None, &mut plane, 9_000).unwrap();
    assert!(f.franchise.complete(&id, 10_000).unwrap());
    f.reopen();
    assert_eq!(f.franchise.snapshot().record().unwrap().completed, 1);
}

#[test]
#[tracing::instrument]
fn scouting_counts_new_observations_once_across_drives_and_restarts() {
    let mut f = Fixture::new(3);
    f.objective("first");
    f.objective("second");
    f.objective("third");
    let id = f.start("first", "first-drive");
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    f.franchise.step(&id, None, &mut plane, 1_100).unwrap();
    let learned = f.franchise.snapshot().scouting().unwrap();
    assert!(learned.samples() > 0);
    let second = f.start("second", "second-drive");
    assert_eq!(f.franchise.snapshot().drives["second-drive"].seed, learned);
    assert_eq!(f.franchise.snapshot().scouting().unwrap(), learned);
    f.reopen();
    assert_eq!(f.franchise.snapshot().scouting().unwrap(), learned);
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&second, &mut plane).unwrap();
    f.franchise.step(&second, None, &mut plane, 2_000).unwrap();
    assert_eq!(
        f.franchise.snapshot().scouting().unwrap().samples(),
        learned.samples() + 1
    );
    f.start("third", "third-drive");
    f.reopen();
    assert_eq!(
        f.franchise.snapshot().scouting().unwrap().samples(),
        learned.samples() + 1
    );
}

#[test]
#[tracing::instrument]
fn claimed_completion_and_forged_gate_satisfaction_cannot_create_yardage() {
    let mut f = Fixture::new(1);
    let mut gates = playbook_sim::standard_gates().unwrap();
    gates[0].state = GateState::Satisfied;
    gates[0].satisfaction_ppm = PPM_SCALE;
    assert!(f
        .franchise
        .declare_objective(Objective {
            id: ObjectiveId::new("forged").unwrap(),
            task: playbook_sim::task_contract(TaskIntegrity::Sound),
            gates,
            drive_id: None
        })
        .is_err());
    f.objective("real");
    let id = f.start("real", "drive");
    let before = f.franchise.snapshot().drives["drive"]
        .state()
        .unwrap()
        .ytg_milli();
    assert!(!f.franchise.complete(&id, 2_000).unwrap());
    assert_eq!(
        f.franchise.snapshot().drives["drive"]
            .state()
            .unwrap()
            .ytg_milli(),
        before
    );
    f.reopen();
    assert_eq!(f.franchise.snapshot().record().unwrap().completed, 0);
}

#[test]
#[tracing::instrument]
fn tampered_franchise_is_rejected_without_repairing_away_ownership() {
    let f = Fixture::new(1);
    let path = f.directory.join("team.json");
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    envelope["franchise"]["salary_cap"]["tokens"] = serde_json::json!(9_000_000);
    let bytes = serde_json::to_vec(&envelope).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        Franchise::open(&path),
        Err(PlaybookError::DigestMismatch)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
#[tracing::instrument]
fn confirming_a_lost_result_reserves_its_unknown_cost_before_clearing_the_invocation() {
    let mut f = Fixture::new(1);
    f.objective("ship");
    let id = f.start("ship", "drive");
    f.franchise
        .preflight(&id, &mut FixturePlane::mvp())
        .unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.franchise.step(&id, None, &mut Disappear, 2_000).unwrap();
    }));
    f.reopen();
    let key = f.franchise.snapshot().drives["drive"]
        .state()
        .unwrap()
        .open_execution()
        .unwrap()
        .idempotency_key
        .clone()
        .unwrap();
    f.franchise.reconcile(&id, &key, true).unwrap();
    f.reopen();
    let snapshot = f.franchise.snapshot();
    let state = snapshot.drives["drive"].state().unwrap();
    assert!(state.open_execution().is_none());
    assert_eq!(state.budget_consumed(), state.budget_initial());
    assert_eq!(
        snapshot.record().unwrap().observed_spend,
        DriveBudget::zero()
    );
    assert_eq!(
        snapshot.record().unwrap().unmeasured_spend_reserved,
        state.budget_initial()
    );
    let before = snapshot.clone();
    assert!(f.franchise.reconcile(&id, &key, true).is_err());
    assert_eq!(f.franchise.snapshot(), before);
}

struct Overrun(FixturePlane);
impl CapabilityPlane for Overrun {
    #[tracing::instrument]
fn discover(&mut self) -> Result<CapabilitySnapshot, PlaybookError> {
        self.0.discover()
    }
    #[tracing::instrument]
fn execute(&mut self, execution: &PlayExecution) -> Result<PlaneResult, PlaybookError> {
        let mut result = self.0.execute(execution)?;
        if let PlaneResult::Done(done) = &mut result {
            done.cost.tokens = 2_000;
        }
        Ok(result)
    }
}

#[test]
#[tracing::instrument]
fn reported_overrun_stays_in_franchise_accounting_after_reconciliation() {
    let mut f = Fixture::new(1);
    f.objective("ship");
    let id = f.start("ship", "drive");
    let mut plane = Overrun(FixturePlane::mvp());
    f.franchise.preflight(&id, &mut plane).unwrap();
    assert!(f.franchise.step(&id, None, &mut plane, 2_000).is_err());
    let snapshot = f.franchise.snapshot();
    assert_eq!(snapshot.record().unwrap().observed_spend.tokens, 2_000);
    let key = snapshot.drives["drive"]
        .state()
        .unwrap()
        .open_execution()
        .unwrap()
        .idempotency_key
        .clone()
        .unwrap();
    f.franchise.reconcile(&id, &key, true).unwrap();
    f.reopen();
    let record = f.franchise.snapshot().record().unwrap();
    assert_eq!(record.observed_spend.tokens, 2_000);
    assert_eq!(record.unmeasured_spend_reserved, DriveBudget::zero());
    assert_eq!(
        f.franchise.step(&id, None, &mut plane, 3_000),
        Err(PlaybookError::BudgetExhausted)
    );
}

#[test]
#[tracing::instrument]
fn future_contracts_are_quarantined_and_live_acquisitions_cannot_change_eligibility() {
    let mut f = Fixture::new(3);
    let input: playbook_front_office::PortfolioInput =
        serde_json::from_str(include_str!("../../../fixtures/front_office.json")).unwrap();
    let current = playbook_front_office::construct(&input).unwrap();
    f.franchise
        .stage_roster(current.clone(), "preseason selection")
        .unwrap();
    f.franchise.activate_roster("kickoff window").unwrap();
    f.franchise
        .declare_objective(Objective {
            id: ObjectiveId::new("ship").unwrap(),
            task: playbook_sim::task_contract(TaskIntegrity::Sound),
            gates: vec![playbook_sim::gate_one(
                "implementation",
                EvidenceClass::CompilerResult,
            )],
            drive_id: None,
        })
        .unwrap();
    let id = f.start("ship", "drive");
    let frozen = f.franchise.snapshot().drives["drive"]
        .frozen_roster
        .clone()
        .unwrap();
    assert_eq!(frozen.contract, Some(current.clone()));
    let late = playbook_sim::person(
        "late-acquisition",
        PersonnelKind::DeterministicProcedure,
        1,
        None,
    );
    f.franchise
        .acquire(
            RosterMember {
                personnel: late.clone(),
                position: "engineering".into(),
                provider: Some("future-provider".into()),
                model: None,
                availability: Availability::Ready,
            },
            "future inventory",
        )
        .unwrap();
    let mut future_input = input;
    future_input.effective_season = 3;
    future_input.evidence_through_season = 2;
    let future = playbook_front_office::construct(&future_input).unwrap();
    f.franchise
        .stage_roster(future.clone(), "quarantined planning")
        .unwrap();
    let before = f.franchise.snapshot();
    assert!(f.franchise.activate_roster("not during play").is_err());
    assert_eq!(f.franchise.snapshot(), before);
    f.reopen();
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    for player in [&late.id, &PersonnelId::new("rust-analyser").unwrap()] {
        assert!(f
            .franchise
            .step(&id, Some(player), &mut plane, 1_100)
            .is_err());
    }
    for at in [1_100, 2_000] {
        f.franchise.step(&id, None, &mut plane, at).unwrap();
    }
    assert!(f.franchise.complete(&id, 3_000).unwrap());
    assert_eq!(f.franchise.snapshot().current_roster, Some(current)); // Never auto-activate.
    f.franchise.activate_roster("between contests").unwrap();
    assert_eq!(f.franchise.snapshot().current_roster, Some(future.clone()));
    f.reopen();
    assert_eq!(
        f.franchise.snapshot().drives["drive"].frozen_roster,
        Some(frozen)
    );
    f.objective("next-contest");
    f.start("next-contest", "next-drive");
    assert_eq!(
        f.franchise.snapshot().drives["next-drive"]
            .frozen_roster
            .as_ref()
            .unwrap()
            .contract,
        Some(future)
    );
}

#[test]
#[tracing::instrument]
fn legacy_snapshots_freeze_once_at_recovery_and_keep_the_original_drive_evidence() {
    let mut f = Fixture::new(1);
    f.objective("ship");
    let id = f.start("ship", "drive");
    let mut plane = FixturePlane::mvp();
    f.franchise.preflight(&id, &mut plane).unwrap();
    f.franchise.step(&id, None, &mut plane, 1_100).unwrap();
    let original = f.franchise.snapshot();
    let path = f.directory.join("team.json");
    let mut envelope: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    envelope["franchise"]["drives"]["drive"]
        .as_object_mut()
        .unwrap()
        .remove("frozen_roster");
    envelope["digest"] = serde_json::json!(sha256_hex(
        &to_canonical_bytes(&envelope["franchise"]).unwrap()
    ));
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    f.reopen();
    assert!(f.franchise.snapshot().drives["drive"]
        .frozen_roster
        .is_some());
    assert_eq!(
        f.franchise.snapshot().drives["drive"].events,
        original.drives["drive"].events
    );
    let migrated = f.franchise.snapshot();
    f.reopen();
    assert_eq!(f.franchise.snapshot(), migrated);
}
