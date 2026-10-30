//! Pre-dispatch tactical adaptation.
//!
//! Three adjustment plays, each with the same constitutional line: they
//! happen BEFORE irreversible execution or they do not happen at all.
//!
//! Motion sends personnel out to probe before the snap, revealing the
//! defensive structure — probes are diagnostic, bounded, tiny, never a
//! full play, and they change nothing they inspect. Shift
//! reconfigures responsibility distribution without changing the roster:
//! the formation changes, the people do not. Audible replaces the called
//! tactic on newly observed state — and an audible after the snap is not an
//! audible, so a plan that has already begun executing keeps its plan.
//! Check-With-Me precomputes the admissible alternatives and selects
//! deterministically at dispatch time.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError};

use crate::domain::{
    tactics_refusal, TacticalContext, TacticalTrigger, TacticalTrigger as Trigger,
};

/// What a motion probe investigates. The directive's three canonical probes:
/// the API surface, the tests, the dependency graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeKind {
    Api,
    Tests,
    DependencyGraph,
}

/// One diagnostic probe: one worker, one domain, before the play. A probe
/// observes; it never reassigns anyone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionProbe {
    pub domain: String,
    pub personnel: PersonnelId,
    pub probe_kind: ProbeKind,
}

/// The motion plan. Motion is diagnostic by construction: the budget is
/// tiny, because a probe that costs a play is a play. A plan is pure
/// observation — personnel, domains, and probe kinds — and carries no
/// personnel shape and no authority to alter one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionPlan {
    pub probes: Vec<MotionProbe>,
    pub max_tokens: u64,
    pub max_duration_ms: u64,
}

/// Assign probes round-robin across the available personnel: domain `i` is
/// probed by `available[i % available.len()]`, and the probe kind cycles
/// Api → Tests → DependencyGraph in order. Refuses an empty roster or an
/// empty domain list — motion with nobody or nowhere is nothing.
pub #[tracing::instrument]
fn plan_motion(
    available: &[PersonnelId],
    domains: &[&str],
) -> Result<MotionPlan, PlaybookError> {
    if available.is_empty() {
        return Err(tactics_refusal("motion needs personnel"));
    }
    if domains.is_empty() {
        return Err(tactics_refusal("motion needs domains to probe"));
    }
    let kinds = [ProbeKind::Api, ProbeKind::Tests, ProbeKind::DependencyGraph];
    let probes = domains
        .iter()
        .enumerate()
        .map(|(index, domain)| MotionProbe {
            domain: (*domain).to_string(),
            personnel: available[index % available.len()].clone(),
            probe_kind: kinds[index % kinds.len()],
        })
        .collect();
    Ok(MotionPlan {
        probes,
        // Motion is diagnostic: bounded and tiny by doctrine.
        max_tokens: 500,
        max_duration_ms: 2_000,
    })
}

/// The information gain of the motion: the domains whose probing CHANGED
/// the coupling reading, in probe order; empty when the reading is
/// unchanged. The probe set is read as one diagnostic against a single
/// before/after coupling pair: a moved reading reports every probed
/// domain in probe order, an unmoved reading reports nothing. A probe
/// that revealed nothing is still cheap evidence — but only changed
/// readings are reported.
pub #[tracing::instrument]
fn motion_reveals(
    probes: &[MotionProbe],
    coupling_after_ppm: u32,
    coupling_before_ppm: u32,
) -> Vec<String> {
    if coupling_after_ppm == coupling_before_ppm {
        return Vec::new();
    }
    probes
        .iter()
        .map(|probe| probe.domain.clone())
        .collect::<Vec<_>>()
}

/// One responsibility move in a shift: the roster is constant, the
/// formation changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShiftMove {
    pub personnel: PersonnelId,
    pub from_domain: String,
    pub to_domain: String,
}

/// A whole shift: multiple moves applied in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shift {
    pub moves: Vec<ShiftMove>,
}

/// Apply a shift to a personnel shape without changing the roster. Each
/// move takes one person out of the `from_domain` and puts them in the
/// `to_domain` (created with sole ownership when absent). Moves apply in
/// order, so a later move may reference a domain an earlier move created
/// — or be refused by a gap an earlier move opened. Refusals: moving the
/// last owner of a domain (an unowned domain is a gap), moving to the
/// same domain, and moving someone who does not hold the from domain.
/// Each legal move removes one membership and adds one, so the roster
/// total across domains is invariant under any legal shift, and the
/// shifted shape must still validate.
pub #[tracing::instrument]
fn apply_shift(
    mut shape: crate::domain::PersonnelShape,
    shift: &Shift,
) -> Result<crate::domain::PersonnelShape, PlaybookError> {
    for move_request in &shift.moves {
        if move_request.from_domain == move_request.to_domain {
            return Err(tactics_refusal("a shift to the same domain moves nothing"));
        }
        let from_index = shape
            .assignments
            .iter()
            .position(|assignment| assignment.domain == move_request.from_domain)
            .ok_or_else(|| tactics_refusal("shift from an unknown domain"))?;
        let holding = shape.assignments[from_index]
            .personnel
            .iter()
            .any(|holder| holder == &move_request.personnel);
        if !holding {
            return Err(tactics_refusal(
                "shifting personnel who do not hold the domain",
            ));
        }
        if shape.assignments[from_index].personnel.len() == 1 {
            return Err(tactics_refusal("shift would leave a domain unowned"));
        }
        shape.assignments[from_index]
            .personnel
            .retain(|holder| holder != &move_request.personnel);
        match shape
            .assignments
            .iter_mut()
            .find(|assignment| assignment.domain == move_request.to_domain)
        {
            Some(target) => target.personnel.push(move_request.personnel.clone()),
            None => shape.assignments.push(crate::domain::DomainAssignment {
                domain: move_request.to_domain.clone(),
                personnel: vec![move_request.personnel.clone()],
                mode: crate::domain::OwnershipMode::Sole,
            }),
        }
    }
    shape.validate()?;
    Ok(shape)
}

/// The audible outcome. `KeepPlan` is the structural refusal: an audible
/// after execution began is not an audible.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudibleDecision {
    KeepPlan,
    AudibleTo {
        new_tactic: String,
        reason_code: String,
    },
}

/// Call the audible. The hard rule, enforced structurally: once the plan
/// has begun executing (`repeated_failures > 0` — the plan has met the
/// field), the plan stands. Before dispatch, the first candidate whose
/// trigger fires on the observed state — and who is not the planned tactic
/// itself — wins.
pub #[tracing::instrument]
fn call_audible(
    planned_tactic: &str,
    observed: &TacticalContext,
    candidates: &[(String, Trigger)],
) -> AudibleDecision {
    // Execution has begun: the play is live, and live plays are not
    // re-called from the sideline.
    if observed.repeated_failures > 0 {
        return AudibleDecision::KeepPlan;
    }
    for (name, trigger) in candidates {
        if name == planned_tactic {
            continue;
        }
        if trigger.evaluate(observed) {
            return AudibleDecision::AudibleTo {
                new_tactic: name.clone(),
                reason_code: format!("observed_state_matches_candidate_{name}"),
            };
        }
    }
    AudibleDecision::KeepPlan
}

/// Check-With-Me: the precomputed alternatives. Candidates are (name,
/// trigger, net value in ppm); the selection is the first candidate whose
/// trigger fires on the observed context AND whose net value is strictly
/// above the threshold — a net value exactly equal to the threshold does
/// not win. Nothing qualifies → nothing is called, and the planned tactic
/// stands.
pub #[tracing::instrument]
fn check_with_me(
    candidates: &[(String, TacticalTrigger, i64)],
    context: &TacticalContext,
    threshold_ppm: u32,
) -> Option<(String, i64)> {
    candidates
        .iter()
        .find(|(_, trigger, net)| trigger.evaluate(context) && *net > i64::from(threshold_ppm))
        .map(|(name, _, net)| (name.clone(), *net))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracing::instrument]
fn person(id: &str) -> PersonnelId {
        PersonnelId::new(id).unwrap()
    }

    #[tracing::instrument]
fn context(repeated_failures: u32) -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 400,
            domain_coupling_ppm: 900,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures,
            risk_band: crate::domain::RiskBand::Guarded,
            divergence_observed: false,
            critical_domain: None,
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 0,
        }
    }

    #[tracing::instrument]
fn shape() -> crate::domain::PersonnelShape {
        crate::domain::PersonnelShape {
            primary_executors: 3,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![
                crate::domain::DomainAssignment {
                    domain: "architecture".into(),
                    personnel: vec![person("a")],
                    mode: crate::domain::OwnershipMode::Sole,
                },
                crate::domain::DomainAssignment {
                    domain: "tests".into(),
                    personnel: vec![person("b"), person("c")],
                    mode: crate::domain::OwnershipMode::Shared,
                },
            ],
        }
    }

    #[test]
    #[tracing::instrument]
fn motion_probes_assign_round_robin_with_a_tiny_budget() {
        let plan = plan_motion(
            &[person("a"), person("b")],
            &["api", "tests", "deps", "schema"],
        )
        .unwrap();
        assert_eq!(plan.probes.len(), 4);
        assert_eq!(plan.probes[0].personnel, person("a"));
        assert_eq!(plan.probes[1].personnel, person("b"));
        assert_eq!(plan.probes[2].personnel, person("a"));
        assert_eq!(plan.probes[3].personnel, person("b"));
        assert_eq!(plan.probes[0].probe_kind, ProbeKind::Api);
        assert_eq!(plan.probes[1].probe_kind, ProbeKind::Tests);
        assert_eq!(plan.probes[2].probe_kind, ProbeKind::DependencyGraph);
        assert_eq!(plan.probes[3].probe_kind, ProbeKind::Api);
        assert_eq!(plan.max_tokens, 500);
        assert_eq!(plan.max_duration_ms, 2_000);
        assert!(plan_motion(&[], &["api"]).is_err());
        assert!(plan_motion(&[person("a")], &[]).is_err());
    }

    #[test]
    #[tracing::instrument]
fn motion_reports_exactly_the_changed_readings() {
        let plan = plan_motion(&[person("a")], &["api", "tests"]).unwrap();
        assert!(motion_reveals(&plan.probes, 600, 600).is_empty());
        assert_eq!(motion_reveals(&plan.probes, 600, 900).len(), 2);
    }

    #[test]
    #[tracing::instrument]
fn shifts_move_people_without_changing_the_roster() {
        let shift = Shift {
            moves: vec![ShiftMove {
                personnel: person("c"),
                from_domain: "tests".into(),
                to_domain: "build chain".into(),
            }],
        };
        let shifted = apply_shift(shape(), &shift).unwrap();
        // The roster is constant: three people, still.
        let total: usize = shifted
            .assignments
            .iter()
            .map(|assignment| assignment.personnel.len())
            .sum();
        assert_eq!(total, 3);
        // The moved person holds the new domain and not the old.
        assert!(shifted
            .assignments
            .iter()
            .find(|assignment| assignment.domain == "build chain")
            .is_some_and(|assignment| assignment.personnel.contains(&person("c"))));
        assert!(!shifted
            .assignments
            .iter()
            .find(|assignment| assignment.domain == "tests")
            .is_some_and(|assignment| assignment.personnel.contains(&person("c"))));
        assert!(shifted.validate().is_ok());
    }

    #[test]
    #[tracing::instrument]
fn shifts_refuse_gaps_same_domains_and_absent_people() {
        // Moving the last owner leaves a gap: refused.
        let gap = Shift {
            moves: vec![ShiftMove {
                personnel: person("a"),
                from_domain: "architecture".into(),
                to_domain: "tests".into(),
            }],
        };
        assert!(apply_shift(shape(), &gap).is_err());
        // Same domain: a no-op pretending to be a move.
        let same = Shift {
            moves: vec![ShiftMove {
                personnel: person("b"),
                from_domain: "tests".into(),
                to_domain: "tests".into(),
            }],
        };
        assert!(apply_shift(shape(), &same).is_err());
        // The person does not hold the domain.
        let absent = Shift {
            moves: vec![ShiftMove {
                personnel: person("d"),
                from_domain: "tests".into(),
                to_domain: "build chain".into(),
            }],
        };
        assert!(apply_shift(shape(), &absent).is_err());
    }

    #[test]
    #[tracing::instrument]
fn audibles_fire_before_dispatch_and_never_after_the_snap() {
        let candidates: Vec<(String, Trigger)> = vec![
            ("trips".into(), Trigger::CriticalDomainNamed),
            ("bunch".into(), Trigger::DomainCouplingAtLeast { ppm: 600 }),
        ];
        let observed = context(0);
        match call_audible("spread", &observed, &candidates) {
            AudibleDecision::AudibleTo {
                new_tactic,
                reason_code,
            } => {
                // Trips' trigger (a named critical domain) does not fire;
                // the coupling reading fires Bunch.
                assert_eq!(new_tactic, "bunch");
                assert!(reason_code.contains("bunch"));
            }
            other => panic!("pre-dispatch observation audibles: {other:?}"),
        }
        // After the snap: the same observation keeps the plan. This is the
        // structural refusal, not a judgement.
        let after_snap = context(1);
        assert_eq!(
            call_audible("spread", &after_snap, &candidates),
            AudibleDecision::KeepPlan
        );
        // The planned tactic is never audible to itself.
        let self_firing: Vec<(String, Trigger)> =
            vec![("spread".into(), Trigger::DomainCouplingAtLeast { ppm: 600 })];
        assert_eq!(
            call_audible("spread", &observed, &self_firing),
            AudibleDecision::KeepPlan
        );
    }

    #[test]
    #[tracing::instrument]
fn check_with_me_selects_the_first_candidate_above_the_threshold() {
        let candidates: Vec<(String, Trigger, i64)> = vec![
            (
                "heavy".into(),
                Trigger::RiskBandAtLeast {
                    band: crate::domain::RiskBand::Critical,
                },
                300,
            ),
            (
                "spread".into(),
                Trigger::ProblemBreadthAtLeast { ppm: 300 },
                500,
            ),
            (
                "bunch".into(),
                Trigger::DomainCouplingAtLeast { ppm: 600 },
                450,
            ),
        ];
        // Heavy's trigger misses; Spread fires above the threshold and wins
        // over the higher-scoring but non-firing Bunch.
        let picked = check_with_me(&candidates, &context(0), 300).unwrap();
        assert_eq!(picked, ("spread".into(), 500));
        // Raising the threshold to Spread's own net demotes it (500 is not
        // above 500), and Bunch's 450 cannot clear 500 either: the plan
        // stands.
        assert!(check_with_me(&candidates, &context(0), 500).is_none());
        // Nothing fires above a prohibitive threshold either.
        assert!(check_with_me(&candidates, &context(0), 900).is_none());
    }

    #[test]
    #[tracing::instrument]
fn motion_is_a_diagnostic_with_a_budget_well_below_a_play() {
        // A reference full play (the admission fixture's budget) runs
        // 10_000 tokens over 30_000 ms. Motion must sit far under that: a
        // probe that costs a play is a play.
        let plan = plan_motion(&[person("a"), person("b")], &["api", "tests"]).unwrap();
        assert!(plan.max_tokens < 10_000);
        assert!(plan.max_duration_ms < 30_000);
        // And the plan is pure observation: personnel, domains, and kinds.
        // Two identical calls read identically, and the probes name people
        // without reassigning them — no personnel shape enters or leaves
        // the planner.
        assert_eq!(
            plan,
            plan_motion(&[person("a"), person("b")], &["api", "tests"]).unwrap()
        );
        for probe in &plan.probes {
            assert!(matches!(
                probe.probe_kind,
                ProbeKind::Api | ProbeKind::Tests | ProbeKind::DependencyGraph
            ));
        }
    }

    #[test]
    #[tracing::instrument]
fn shifts_apply_in_order_and_may_reference_created_domains() {
        let four = crate::domain::PersonnelShape {
            primary_executors: 4,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![
                crate::domain::DomainAssignment {
                    domain: "architecture".into(),
                    personnel: vec![person("a")],
                    mode: crate::domain::OwnershipMode::Sole,
                },
                crate::domain::DomainAssignment {
                    domain: "tests".into(),
                    personnel: vec![person("b"), person("c"), person("d")],
                    mode: crate::domain::OwnershipMode::Shared,
                },
            ],
        };
        let shift = Shift {
            moves: vec![
                // Move 1 creates "build chain" with c as its sole owner.
                ShiftMove {
                    personnel: person("c"),
                    from_domain: "tests".into(),
                    to_domain: "build chain".into(),
                },
                // Move 2 joins b into the domain move 1 created.
                ShiftMove {
                    personnel: person("b"),
                    from_domain: "tests".into(),
                    to_domain: "build chain".into(),
                },
                // Move 3 moves c back out of it — legal only because moves
                // one and two ran first and left a second owner behind.
                ShiftMove {
                    personnel: person("c"),
                    from_domain: "build chain".into(),
                    to_domain: "deps".into(),
                },
            ],
        };
        let shifted = apply_shift(four.clone(), &shift).unwrap();
        let owners_of = |domain: &str| {
            shifted
                .assignments
                .iter()
                .find(|assignment| assignment.domain == domain)
                .map(|assignment| assignment.personnel.clone())
                .unwrap_or_default()
        };
        assert_eq!(owners_of("architecture"), vec![person("a")]);
        assert_eq!(owners_of("tests"), vec![person("d")]);
        assert_eq!(owners_of("build chain"), vec![person("b")]);
        assert_eq!(owners_of("deps"), vec![person("c")]);
        // Roster constancy under a multi-move shift: every legal move
        // removes one membership and adds one, so the total across domains
        // is invariant, and the distinct people are the same people.
        let total = |shape: &crate::domain::PersonnelShape| {
            shape
                .assignments
                .iter()
                .map(|assignment| assignment.personnel.len())
                .sum::<usize>()
        };
        assert_eq!(total(&four), 4);
        assert_eq!(total(&shifted), 4);
        assert!(shifted.validate().is_ok());
        // The ordering is real: move 3 alone, on the unshifted shape,
        // names a domain nobody has created yet.
        let late_move_alone = Shift {
            moves: vec![shift.moves[2].clone()],
        };
        assert!(apply_shift(four, &late_move_alone).is_err());
    }

    #[test]
    #[tracing::instrument]
fn the_audible_refusal_is_structural_even_when_every_candidate_fires() {
        // A candidate list that fires on every observation whatsoever:
        // whatever the field shows, these alternatives want in.
        let always_firing: Vec<(String, Trigger)> = vec![
            ("trips".into(), Trigger::Always),
            ("bunch".into(), Trigger::Always),
        ];
        // Before dispatch they audible, first-listed first, with the
        // documented reason code.
        assert_eq!(
            call_audible("spread", &context(0), &always_firing),
            AudibleDecision::AudibleTo {
                new_tactic: "trips".into(),
                reason_code: "observed_state_matches_candidate_trips".into(),
            }
        );
        // After the snap the identical list, still firing on everything,
        // is refused. The refusal is structural — execution began — not
        // circumstantial: no candidate list can talk the sideline out of
        // it.
        assert_eq!(
            call_audible("spread", &context(1), &always_firing),
            AudibleDecision::KeepPlan
        );
        // The same inputs decide the same way.
        assert_eq!(
            call_audible("spread", &context(1), &always_firing),
            call_audible("spread", &context(1), &always_firing)
        );
    }

    #[test]
    #[tracing::instrument]
fn check_with_me_wins_only_strictly_above_the_threshold() {
        let candidates: Vec<(String, Trigger, i64)> =
            vec![("counter".into(), Trigger::Always, 400)];
        // Exactly at the threshold: the plan stands.
        assert_eq!(check_with_me(&candidates, &context(0), 400), None);
        // One ppm above it: the alternative wins.
        assert_eq!(
            check_with_me(&candidates, &context(0), 399),
            Some(("counter".into(), 400))
        );
        // The same inputs decide the same way.
        assert_eq!(
            check_with_me(&candidates, &context(0), 399),
            check_with_me(&candidates, &context(0), 399)
        );
    }

    #[test]
    #[tracing::instrument]
fn check_with_me_walks_the_documented_ladder_in_order() {
        // The mirror of the check_with_me.tov ladder: instability
        // concentrates first (heavy), independent breadth spreads second,
        // and bunch is the unconditional fallback. Order decides — every
        // candidate carries the same clearing net value, so a
        // highest-score rule would read the ladder differently.
        let ladder: Vec<(String, Trigger, i64)> = vec![
            (
                "heavy".into(),
                Trigger::ProviderInstabilityAbove { ppm: 700 },
                100,
            ),
            (
                "spread".into(),
                Trigger::All(vec![
                    Trigger::ProblemBreadthAtLeast { ppm: 600 },
                    Trigger::DomainCouplingAtMost { ppm: 400 },
                ]),
                100,
            ),
            ("bunch".into(), Trigger::Always, 100),
        ];
        let mut observed = context(0);
        // (200, 700, 200): independent broad problems spread.
        observed.provider_instability_ppm = 200;
        observed.problem_breadth_ppm = 700;
        observed.domain_coupling_ppm = 200;
        assert_eq!(
            check_with_me(&ladder, &observed, 0),
            Some(("spread".into(), 100))
        );
        // Instability above 700 concentrates first — and exactly 700 is
        // not ABOVE 700: the ladder is walked in order, not re-sorted.
        observed.provider_instability_ppm = 800;
        assert_eq!(
            check_with_me(&ladder, &observed, 0),
            Some(("heavy".into(), 100))
        );
        observed.provider_instability_ppm = 700;
        assert_eq!(
            check_with_me(&ladder, &observed, 0),
            Some(("spread".into(), 100))
        );
        // (200, 300, 700): coupled, narrow problems fall to the fallback.
        observed.problem_breadth_ppm = 300;
        observed.domain_coupling_ppm = 700;
        assert_eq!(
            check_with_me(&ladder, &observed, 0),
            Some(("bunch".into(), 100))
        );
    }

    #[test]
    #[tracing::instrument]
fn every_planning_function_is_deterministic() {
        // Motion plans: identical calls, identical plans.
        let first = plan_motion(&[person("a"), person("b")], &["api", "tests", "deps"]).unwrap();
        assert_eq!(
            first,
            plan_motion(&[person("a"), person("b")], &["api", "tests", "deps"]).unwrap()
        );
        // Motion readings: identical readings, reported in probe order.
        assert_eq!(
            motion_reveals(&first.probes, 600, 900),
            vec!["api".to_string(), "tests".to_string(), "deps".to_string()]
        );
        assert_eq!(
            motion_reveals(&first.probes, 600, 900),
            motion_reveals(&first.probes, 600, 900)
        );
        // Shifts: the same shift on equal shapes lands in the same place.
        let shift = Shift {
            moves: vec![ShiftMove {
                personnel: person("c"),
                from_domain: "tests".into(),
                to_domain: "build chain".into(),
            }],
        };
        assert_eq!(
            apply_shift(shape(), &shift).unwrap(),
            apply_shift(shape(), &shift).unwrap()
        );
        // Audibles.
        let candidates: Vec<(String, Trigger)> =
            vec![("bunch".into(), Trigger::DomainCouplingAtLeast { ppm: 600 })];
        assert_eq!(
            call_audible("spread", &context(0), &candidates),
            call_audible("spread", &context(0), &candidates)
        );
        // Check-with-me selections.
        let options: Vec<(String, Trigger, i64)> = vec![("heavy".into(), Trigger::Always, 250)];
        assert_eq!(
            check_with_me(&options, &context(0), 100),
            check_with_me(&options, &context(0), 100)
        );
    }
}
