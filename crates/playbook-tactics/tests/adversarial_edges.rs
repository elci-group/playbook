//! §40 adversarial edges of the tactical layer. Eight named scenarios,
//! each pinning a promised refusal or degradation at the layer that owns it:
//!
//! 1. `empty_roster_is_refused_at_play_level` — the planner deploys only
//!    personnel it was given; an empty roster is refused at the play level
//!    while the doctrine directive itself stays valid, and a zero-deployment
//!    shape is legal only through observers.
//! 2. `a_single_player_formation_degrades_to_a_legal_sole_owner` — Bunch at
//!    one available player cannot claim joint ownership; it degrades to sole.
//! 3. `the_maximum_formation_deploys_every_seat_and_stays_legal` — the
//!    franchise-scale roster of twenty-four deploys every seat, one sole
//!    owner per lane, no domain double-booked.
//! 4. `personnel_disappearing_after_formation_invalidates_the_shape` — a
//!    formed shape whose owner is struck after the fact no longer validates;
//!    re-planning from the reduced roster does.
//! 5. `an_audible_after_dispatch_is_never_called` — once execution has begun
//!    (`repeated_failures > 0`) the plan stands against an all-firing
//!    candidate list; the refusal is structural, not circumstantial.
//! 6. `max_formation_with_conflicting_doctrine_fails_closed` — a twenty-four
//!    seat Spread plan against max_protect and prevent resolves exactly as
//!    `composition::compatibility` declares: exclusive pairs drop the later
//!    doctrine, invalid pairs refuse outright.
//! 7. `double_booking_survives_no_path` — a shift that would seat one person
//!    twice on one domain is refused; `apply_shift` carries no dedicated
//!    pre-check and the terminal `validate()` inside it is the guard.
//! 8. `an_incomplete_formation_directive_is_refused_at_admission` — an
//!    unnamed directive, a zero-play budget, and an observe-only authority
//!    claiming executors are each refused at admission.

use playbook_core::{PersonnelId, PlaybookError, TacticalId};
use playbook_tactics::adjustment::{apply_shift, call_audible, AudibleDecision, Shift, ShiftMove};
use playbook_tactics::admission::{admit, validated, Admission};
use playbook_tactics::composition::{compatibility, compose, Compatibility};
use playbook_tactics::domain::{
    tactics_refusal, AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment,
    ExpectedEffect, HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, TacticalBudget,
    TacticalClass, TacticalContext, TacticalDirective, TacticalTrigger, TerminationCondition,
};
use playbook_tactics::formation::{formation_directive, plan as plan_formation, TacticalFormation};

fn person(id: &str) -> PersonnelId {
    PersonnelId::new(id).expect("test personnel ids are valid")
}

/// The roster `p0..p{n-1}`, in order.
fn squad(n: usize) -> Vec<PersonnelId> {
    (0..n).map(|i| person(&format!("p{i}"))).collect()
}

/// The neutral measured context: narrow-ish, loosely coupled, stable.
fn context() -> TacticalContext {
    TacticalContext {
        problem_breadth_ppm: 400,
        domain_coupling_ppm: 300,
        provider_instability_ppm: 100,
        coherence_debt: 100,
        repeated_failures: 0,
        risk_band: playbook_tactics::domain::RiskBand::Low,
        divergence_observed: false,
        critical_domain: None,
        ytg_milli: 10_000,
        budget_remaining_ppm: 900_000,
        short_ytg: false,
        at_ms: 0,
    }
}

/// The broad, independent context the Spread branch of the ladder reads.
fn spread_context() -> TacticalContext {
    TacticalContext {
        problem_breadth_ppm: 800,
        domain_coupling_ppm: 200,
        ..context()
    }
}

/// Assert the error is the tactics refusal carrying exactly `reason`.
fn expect_tactics_refusal(error: PlaybookError, reason: &str) {
    match error {
        PlaybookError::Inadmissible(authority, found) => {
            assert_eq!(authority, "tactics");
            assert_eq!(found, reason);
        }
        other => panic!("expected a tactics refusal ({reason}), found {other:?}"),
    }
}

/// A complete, valid directive under an arbitrary matrix-known name.
fn named_directive(name: &str, class: TacticalClass) -> TacticalDirective {
    TacticalDirective {
        id: TacticalId::new(format!("t-{name}")).expect("directive ids are valid"),
        name: name.to_owned(),
        class,
        trigger: TacticalTrigger::Always,
        benefit_ppm: 400,
        cost_ppm: 100,
        risk_ppm: 50,
        shape: PersonnelShape {
            primary_executors: 2,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![DomainAssignment {
                domain: "primary".into(),
                personnel: vec![person("a"), person("b")],
                mode: OwnershipMode::Shared,
            }],
        },
        information: InformationPolicy {
            max_context_overlap_ppm: 200,
            huddle_access: HuddleAccess::None,
            disclose_failures_to_replacement: true,
        },
        concurrency: ConcurrencyPolicy {
            max_concurrent: 2,
            allow_domain_overlap: false,
        },
        authority: AuthorityPolicy {
            may_reassign: true,
            may_reallocate_budget: false,
            may_change_information_topology: true,
            observe_only: false,
        },
        budget: TacticalBudget {
            max_tokens: 10_000,
            max_cost_microunits: 1_000_000,
            max_duration_ms: 30_000,
            max_plays: 2,
        },
        expected_effect: ExpectedEffect {
            breadth_delta_ppm: 0,
            protection_delta_ppm: 400,
            tempo_delta_ppm: 0,
            information_delta_ppm: 0,
        },
        termination: TerminationCondition {
            max_plays: 2,
            abort_when: vec![AbortTrigger::BudgetExhausted],
        },
        counter_tactic: None,
    }
}

/// §40 scenario 1. The planner never invents personnel, so an empty roster
/// is refused at the play level with the documented reason. The doctrine
/// directive for the same context is context-independent and still validates
/// — its canonical shape carries placeholder personnel, never zero — and a
/// shape deploying nobody validates only when the deployment is observers:
/// the all-observer surveillance shape is legitimate, the empty one is not.
#[test]
fn empty_roster_is_refused_at_play_level() {
    assert_eq!(
        plan_formation(&context(), &[], None),
        Err(tactics_refusal("no personnel available"))
    );
    // The doctrine directive does not consult the roster; it validates for
    // the very context whose play was refused.
    let directive = formation_directive(TacticalFormation::Spread, &context());
    validated(&directive).expect("the doctrine directive validates without a roster");
    assert_eq!(directive.shape.deployed(), 3);
    // A shape deploying nobody is refused, whatever its assignments claim.
    let nobody = PersonnelShape {
        primary_executors: 0,
        support: 0,
        reserves: 0,
        observers: 0,
        assignments: vec![DomainAssignment {
            domain: "parser".into(),
            personnel: vec![person("a")],
            mode: OwnershipMode::Sole,
        }],
    };
    assert_eq!(nobody.deployed(), 0);
    expect_tactics_refusal(
        nobody.validate().expect_err("zero deployment"),
        "formation deploys nobody",
    );
    // The one zero-executor shape that survives: pure observation.
    let watchers = PersonnelShape {
        primary_executors: 0,
        support: 0,
        reserves: 0,
        observers: 2,
        assignments: vec![DomainAssignment {
            domain: "parser".into(),
            personnel: vec![person("a"), person("b")],
            mode: OwnershipMode::Shared,
        }],
    };
    watchers
        .validate()
        .expect("an all-observer shape is legitimate");
}

/// §40 scenario 2. With exactly one player available, the Bunch formation
/// still forms: the shape validates, deploys one, and the single owner is
/// pinned as Sole — joint ownership requires two, so the doctrine degrades
/// to the only legal ownership at n=1.
#[test]
fn a_single_player_formation_degrades_to_a_legal_sole_owner() {
    let mut bunch = context();
    bunch.divergence_observed = true;
    let single = plan_formation(&bunch, &squad(1), None).expect("one player is enough to form");
    single.validate().expect("the degraded shape is legal");
    assert_eq!(single.deployed(), 1);
    assert_eq!(single.primary_executors, 1);
    assert_eq!(single.support + single.reserves + single.observers, 0);
    assert_eq!(single.assignments.len(), 1);
    assert_eq!(single.assignments[0].mode, OwnershipMode::Sole);
    assert_eq!(single.assignments[0].personnel, vec![person("p0")]);
    // The same inputs always degrade the same way.
    assert_eq!(
        single,
        plan_formation(&bunch, &squad(1), None).expect("planned")
    );
}

/// §40 scenario 3. The franchise-scale roster of twenty-four, under the
/// maximum formation (Spread): every seat deploys, no seat is held in
/// support, reserve, or observation, each lane is one sole owner, the
/// domain names are all distinct, and the deployed people are exactly the
/// people given — nobody invented, nobody doubled.
#[test]
fn the_maximum_formation_deploys_every_seat_and_stays_legal() {
    let roster = squad(24);
    let shape = plan_formation(&spread_context(), &roster, None).expect("planned");
    shape.validate().expect("the full deployment validates");
    assert_eq!(shape.deployed(), 24);
    assert_eq!(shape.primary_executors, 24);
    assert_eq!(shape.support + shape.reserves + shape.observers, 0);
    assert_eq!(shape.assignments.len(), 24);
    let mut domains: Vec<&str> = Vec::new();
    let mut holders: Vec<PersonnelId> = Vec::new();
    for assignment in &shape.assignments {
        assert_eq!(assignment.mode, OwnershipMode::Sole, "one owner per lane");
        assert_eq!(assignment.personnel.len(), 1, "no lane shares a context");
        domains.push(assignment.domain.as_str());
        holders.extend(assignment.personnel.iter().cloned());
    }
    domains.sort_unstable();
    domains.dedup();
    assert_eq!(domains.len(), 24, "no domain is double-booked");
    holders.sort();
    holders.dedup();
    let mut expected = roster;
    expected.sort();
    assert_eq!(holders, expected, "exactly the given personnel, once each");
}

/// §40 scenario 4. A shape that validated at the snap stops being legal the
/// moment its personnel disappears: striking the sole owner of a lane by
/// direct mutation leaves the domain unowned and `validate` refuses with the
/// documented reason. The doctrine answer is to re-plan from the reduced
/// roster — the new shape validates and deploys exactly who is left.
#[test]
fn personnel_disappearing_after_formation_invalidates_the_shape() {
    let mut shape = plan_formation(&spread_context(), &squad(3), None).expect("planned");
    shape.validate().expect("formed legally");
    let struck = shape.assignments[0].personnel[0].clone();
    shape.assignments[0]
        .personnel
        .retain(|holder| *holder != struck);
    expect_tactics_refusal(
        shape.validate().expect_err("an unowned domain is a defect"),
        "domain has no owner",
    );
    // Re-plan from the reduced roster: the reduced deployment is legal.
    let remaining: Vec<PersonnelId> = squad(3)
        .into_iter()
        .filter(|holder| *holder != struck)
        .collect();
    assert_eq!(remaining.len(), 2);
    let replanned = plan_formation(&spread_context(), &remaining, None).expect("re-planned");
    replanned.validate().expect("the new shape is legal");
    assert_eq!(replanned.deployed(), 2);
    assert!(replanned
        .assignments
        .iter()
        .all(|assignment| !assignment.personnel.contains(&struck)));
}

/// §40 scenario 5. An audible after dispatch is not an audible: with
/// `repeated_failures == 1` the plan has met the field, and even a candidate
/// list that fires on every observation whatsoever is refused — KeepPlan.
/// The identical list before dispatch does audible, which is what makes the
/// refusal structural (the counter, not a dead candidate list).
#[test]
fn an_audible_after_dispatch_is_never_called() {
    let all_firing: Vec<(String, TacticalTrigger)> = vec![
        ("trips".into(), TacticalTrigger::Always),
        ("bunch".into(), TacticalTrigger::Always),
        ("heavy".into(), TacticalTrigger::Always),
    ];
    let after_dispatch = TacticalContext {
        repeated_failures: 1,
        ..context()
    };
    assert_eq!(
        call_audible("spread", &after_dispatch, &all_firing),
        AudibleDecision::KeepPlan
    );
    // The same inputs always keep the plan.
    assert_eq!(
        call_audible("spread", &after_dispatch, &all_firing),
        call_audible("spread", &after_dispatch, &all_firing)
    );
    // Before dispatch the identical list fires first-listed: the refusal at
    // the line of scrimmage is the repeated-failure reading, nothing else.
    let before_dispatch = TacticalContext {
        repeated_failures: 0,
        ..after_dispatch
    };
    assert_eq!(
        call_audible("spread", &before_dispatch, &all_firing),
        AudibleDecision::AudibleTo {
            new_tactic: "trips".into(),
            reason_code: "observed_state_matches_candidate_trips".into(),
        }
    );
}

/// §40 scenario 6. A maximum Spread plan against conflicting doctrine
/// resolves exactly as the frozen matrix declares, no more and no less:
/// `max_protect` is Exclusive with Spread and is dropped (first-listed
/// wins); `prevent` is merely Conditional with Spread and stays; the same
/// plan in the reversed order keeps `max_protect` and drops Spread; and the
/// matrix's outright Invalid verdict refuses the composition entirely.
#[test]
fn max_formation_with_conflicting_doctrine_fails_closed() {
    let spread_ctx = spread_context();
    let seats = plan_formation(&spread_ctx, &squad(24), None).expect("planned");
    let mut spread = formation_directive(TacticalFormation::Spread, &spread_ctx);
    spread.shape = seats;
    let max_protect = named_directive("max_protect", TacticalClass::Formation);
    let prevent = named_directive("prevent", TacticalClass::Doctrine);
    // The matrix verdicts the outcome must match.
    assert_eq!(
        compatibility("spread", "max_protect"),
        Compatibility::Exclusive
    );
    assert_eq!(
        compatibility("max_protect", "spread"),
        compatibility("spread", "max_protect"),
        "the matrix is symmetric"
    );
    assert_eq!(
        compatibility("spread", "prevent"),
        Compatibility::Conditional
    );
    fn names(plan: &playbook_tactics::composition::TacticalPlan) -> Vec<&str> {
        plan.directives
            .iter()
            .map(|directive| directive.name.as_str())
            .collect()
    }
    // Exclusive: the later doctrine is dropped, the plan still stands.
    let plan = compose(&[spread.clone(), max_protect.clone(), prevent.clone()]).expect("composes");
    assert_eq!(names(&plan), vec!["spread", "prevent"]);
    let reversed = compose(&[max_protect, spread, prevent]).expect("composes");
    assert_eq!(names(&reversed), vec!["max_protect", "prevent"]);
    // Conflicting and Invalid verdicts refuse outright: fail closed.
    assert_eq!(
        compatibility("prevent", "verticals"),
        Compatibility::Conflicting
    );
    assert!(compose(&[
        named_directive("prevent", TacticalClass::Doctrine),
        named_directive("verticals", TacticalClass::Play)
    ])
    .is_err());
    assert_eq!(
        compatibility("empty", "max_protect"),
        Compatibility::Invalid
    );
    assert!(compose(&[
        named_directive("empty", TacticalClass::Formation),
        named_directive("max_protect", TacticalClass::Formation)
    ])
    .is_err());
}

/// §40 scenario 7. There is no path to a double-booked domain through a
/// shift. The move below looks legal to every individual rule — the person
/// holds the from domain, the from domain keeps a second owner, the target
/// exists — yet seating them on a domain they already hold is refused.
/// `apply_shift` carries no dedicated pre-check for this: the refusal is the
/// shared contract's own rule firing in the terminal `validate()` the shift
/// ends with, and the pinned message is the double-booking one.
#[test]
fn double_booking_survives_no_path() {
    let shape = PersonnelShape {
        primary_executors: 3,
        support: 0,
        reserves: 0,
        observers: 0,
        assignments: vec![
            DomainAssignment {
                domain: "alpha".into(),
                personnel: vec![person("a"), person("b")],
                mode: OwnershipMode::Shared,
            },
            DomainAssignment {
                domain: "beta".into(),
                personnel: vec![person("c"), person("a")],
                mode: OwnershipMode::Shared,
            },
        ],
    };
    shape.validate().expect("two lanes, no double booking yet");
    let shift = Shift {
        moves: vec![ShiftMove {
            personnel: person("a"),
            from_domain: "alpha".into(),
            to_domain: "beta".into(),
        }],
    };
    let refused =
        apply_shift(shape.clone(), &shift).expect_err("seating a twice on one domain is refused");
    expect_tactics_refusal(refused, "a person is double-booked on one domain");
    // The refusal is about the double booking, not about a person holding
    // two domains: the same roster shifts legally onto a fresh lane.
    let legal = Shift {
        moves: vec![ShiftMove {
            personnel: person("b"),
            from_domain: "alpha".into(),
            to_domain: "gamma".into(),
        }],
    };
    let shifted = apply_shift(shape, &legal).expect("b shifts onto a fresh lane");
    shifted.validate().expect("and the result stays legal");
    let holds = |shape: &PersonnelShape, who: &str, domain: &str| {
        shape
            .assignments
            .iter()
            .find(|assignment| assignment.domain == domain)
            .is_some_and(|assignment| {
                assignment
                    .personnel
                    .iter()
                    .any(|holder| holder.as_str() == who)
            })
    };
    assert!(holds(&shifted, "b", "gamma"));
    assert!(!holds(&shifted, "b", "alpha"), "the move took b off alpha");
    // Holding two different domains is what remains legal: a still holds
    // both of the lanes it held before any shift ran.
    assert!(holds(&shifted, "a", "alpha"));
    assert!(holds(&shifted, "a", "beta"));
}

/// §40 scenario 8. An incomplete formation directive is refused at
/// admission, each defect by the gate that owns it: an unnamed directive is
/// refused by `validated`; a zero-play budget is refused by `validated` and
/// again by `admit`; an observe-only authority claiming executors passes
/// shape validation but is refused by `admit`, and admits once the
/// deployment is observers.
#[test]
fn an_incomplete_formation_directive_is_refused_at_admission() {
    // No name: the matrix would have nothing to key on.
    let mut unnamed = formation_directive(TacticalFormation::Spread, &context());
    unnamed.name = "   ".into();
    expect_tactics_refusal(
        validated(&unnamed).expect_err("an unnamed directive is refused"),
        "directive names no tactic",
    );
    // No plays: the termination and the budget each declare zero.
    let mut starved = formation_directive(TacticalFormation::Spread, &context());
    starved.termination.max_plays = 0;
    starved.budget.max_plays = 0;
    expect_tactics_refusal(
        validated(&starved).expect_err("a zero-play directive is refused"),
        "termination declares no plays",
    );
    assert_eq!(
        admit(&starved, &context(), 0),
        Admission::Rejected {
            reason_code: "budget_declares_no_plays".into(),
        }
    );
    // Observe-only with executors: the shape itself is legal, the authority
    // clause is what admission refuses.
    let mut spy = formation_directive(TacticalFormation::Spread, &context());
    spy.authority.observe_only = true;
    validated(&spy).expect("the shape and the contract stay legal");
    assert_eq!(
        admit(&spy, &context(), 0),
        Admission::Rejected {
            reason_code: "observe_only_deploys_executors".into(),
        }
    );
    spy.shape = PersonnelShape {
        primary_executors: 0,
        support: 0,
        reserves: 0,
        observers: 3,
        assignments: vec![DomainAssignment {
            domain: "sideline".into(),
            personnel: vec![person("a"), person("b"), person("c")],
            mode: OwnershipMode::Shared,
        }],
    };
    spy.shape.validate().expect("observers only");
    assert_eq!(admit(&spy, &context(), 0), Admission::Admitted);
}
