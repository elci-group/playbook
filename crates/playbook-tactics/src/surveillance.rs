//! High-risk actor and problem surveillance.
//!
//! Two defensive controls for things that can independently lose the game.
//! Spy = one observer assigned specifically to a dynamic, high-risk worker;
//! the spy never executes ordinary work — it watches behavioural drift,
//! scope expansion, resource use, and unusual decisions, and its
//! intervention envelope ends at the recommendation. A spy that intervenes
//! is the failure mode. Bracket = two different kinds of control on one
//! critical domain — a deterministic scanner and a generative adversarial
//! reviewer — because one problem can independently lose the game, and two
//! of the same control is not a bracket.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError};

use crate::domain::{tactics_refusal, PersonnelShape, RiskBand, TacticalContext};

/// One observer assigned to one high-risk subject on one domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpyAssignment {
    /// The watcher. It holds no execution assignment anywhere in the shape.
    pub observer: PersonnelId,
    /// The high-risk worker under watch.
    pub subject: PersonnelId,
    /// The domain the subject is watched on.
    pub domain: String,
}

/// Assign a spy. Non-interference is validated at assignment: the observer
/// must differ from the subject, and must hold no domain assignment in the
/// shape — every assignment in a shape is execution work (an observer's post
/// lives in the observers count, never in the assignment list), and a spy
/// with a job is not watching. The domain must be named.
pub fn assign_spy(
    observer: PersonnelId,
    subject: PersonnelId,
    domain: &str,
    shape: &PersonnelShape,
) -> Result<SpyAssignment, PlaybookError> {
    if observer == subject {
        return Err(tactics_refusal("spy_is_subject"));
    }
    let holds_work = shape.assignments.iter().any(|assignment| {
        assignment
            .personnel
            .iter()
            .any(|holder| holder == &observer)
    });
    if holds_work {
        return Err(tactics_refusal("spy_holds_execution_work"));
    }
    if domain.trim().is_empty() {
        return Err(tactics_refusal("surveillance needs a domain"));
    }
    Ok(SpyAssignment {
        observer,
        subject,
        domain: domain.into(),
    })
}

/// One drift reading over the subject, per-mille scale for rates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriftReading {
    /// Scope expansion: how far the subject has wandered from its
    /// assignment, 0..=1_000 ppm.
    pub scope_expansion_ppm: u32,
    /// Resource use against the envelope, 0..=1_000 ppm.
    pub resource_use_ppm: u32,
    /// Unusual decisions observed this window.
    pub unusual_decisions: u32,
}

/// The policy thresholds the authority sets; the spy only reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DriftPolicy {
    /// Rate at or above which the subject lands in the Watch band, ppm.
    pub watch_ppm: u32,
    /// Rate at or above which the subject lands in the Intervene band, ppm.
    pub intervene_ppm: u32,
    /// Unusual decisions at or above which the subject lands in the
    /// Intervene band.
    pub unusual_decision_limit: u32,
}

impl DriftPolicy {
    /// The frozen default ladder: watch at 300 ppm, intervene at 700 ppm,
    /// intervene on three unusual decisions in one window.
    pub fn builtin() -> Self {
        Self {
            watch_ppm: 300,
            intervene_ppm: 700,
            unusual_decision_limit: 3,
        }
    }
}

/// How far the subject has drifted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftBand {
    Calm,
    Watch,
    Intervene,
}

/// The deterministic bands. Intervene when any rate sits at or above the
/// intervene threshold, or unusual decisions reach the limit; watch when
/// any rate sits at or above the watch threshold, or any unusual decision
/// has been observed; calm otherwise.
pub fn drift_band(reading: &DriftReading, policy: &DriftPolicy) -> DriftBand {
    let rates_intervene = reading.scope_expansion_ppm >= policy.intervene_ppm
        || reading.resource_use_ppm >= policy.intervene_ppm;
    let decisions_intervene = reading.unusual_decisions >= policy.unusual_decision_limit;
    if rates_intervene || decisions_intervene {
        return DriftBand::Intervene;
    }
    let rates_watch = reading.scope_expansion_ppm >= policy.watch_ppm
        || reading.resource_use_ppm >= policy.watch_ppm;
    let decisions_watch = reading.unusual_decisions > 0;
    if rates_watch || decisions_watch {
        return DriftBand::Watch;
    }
    DriftBand::Calm
}

/// The recommendation the watcher hands the AUTHORITY for a band: calm needs
/// nothing, watch earns more observation, intervene earns a halt. Total and
/// deterministic; the spy does nothing itself, and substitution is never
/// recommended here — that call belongs to the Bench, not the watcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Intervention {
    None,
    IncreaseObservation,
    HaltSubject,
    SubstituteSubject,
}

/// Map a band to its recommendation. Every band maps to exactly one.
pub fn intervention(band: DriftBand) -> Intervention {
    match band {
        DriftBand::Calm => Intervention::None,
        DriftBand::Watch => Intervention::IncreaseObservation,
        DriftBand::Intervene => Intervention::HaltSubject,
    }
}

/// A bracket: two named control roles over one critical domain. The struct
/// just carries the roles; that they are genuinely different KINDS of
/// control — a deterministic scanner and a generative adversarial reviewer —
/// is the caller's declaration. Assembly enforces only that the two
/// PERSONNEL are distinct, because two of the same control is not a bracket,
/// it is one control said twice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bracket {
    /// The domain under dual control.
    pub domain: String,
    /// The deterministic role: scans, checks, refuses.
    pub deterministic_control: PersonnelId,
    /// The generative role: probes, reviews, attacks the work.
    pub generative_control: PersonnelId,
}

/// Assemble a bracket. The controls must be distinct personnel (two of the
/// same control is not a bracket, it is one control said twice) and the
/// domain must be named.
pub fn bracket(
    domain: &str,
    deterministic_control: PersonnelId,
    generative_control: PersonnelId,
) -> Result<Bracket, PlaybookError> {
    if domain.trim().is_empty() {
        return Err(tactics_refusal("bracket needs a domain"));
    }
    if deterministic_control == generative_control {
        return Err(tactics_refusal("bracket_controls_identical"));
    }
    Ok(Bracket {
        domain: domain.into(),
        deterministic_control,
        generative_control,
    })
}

/// Dual-control coverage: a domain is bracked when the bracket names it AND
/// both controls hold ownership of that domain in the shape. Ownership is
/// read across every assignment that names the domain, and it is only ever
/// read: the bracket grants nothing, transfers nothing, and edits nothing.
pub fn is_bracked(domain: &str, bracket: &Bracket, shape: &PersonnelShape) -> bool {
    if bracket.domain != domain {
        return false;
    }
    let holds = |control: &PersonnelId| {
        shape
            .assignments
            .iter()
            .any(|assignment| assignment.domain == domain && assignment.personnel.contains(control))
    };
    holds(&bracket.deterministic_control) && holds(&bracket.generative_control)
}

/// Which surveillance doctrine to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurveillanceKind {
    Spy,
    Bracket,
}

/// The doctrine directive. The spy is observe-only by construction — its
/// shape deploys watchers only, which the frozen contract validates — and
/// its budget is a single play, because observation is not work; it admits
/// once the defence reaches Guarded. The bracket is a play: two controls in
/// joint ownership of one domain, admitted from Elevated upward, claiming a
/// protection delta of at least 700 ppm. Both variants pass
/// `crate::admission::validated` as built and admit on their triggers.
pub fn directive(
    kind: SurveillanceKind,
    context: &TacticalContext,
) -> crate::domain::TacticalDirective {
    match kind {
        SurveillanceKind::Spy => crate::domain::TacticalDirective {
            id: playbook_core::TacticalId::new("t-spy").expect("tactic id"),
            name: "spy".into(),
            class: crate::domain::TacticalClass::Adjustment,
            trigger: crate::domain::TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Guarded,
            },
            benefit_ppm: 400,
            cost_ppm: 100,
            risk_ppm: 50,
            shape: crate::domain::PersonnelShape {
                primary_executors: 0,
                support: 0,
                reserves: 0,
                observers: 1,
                assignments: vec![crate::domain::DomainAssignment {
                    domain: "surveillance".into(),
                    personnel: vec![PersonnelId::new("observer").unwrap()],
                    mode: crate::domain::OwnershipMode::Sole,
                }],
            },
            information: crate::domain::InformationPolicy {
                max_context_overlap_ppm: 0,
                huddle_access: crate::domain::HuddleAccess::None,
                disclose_failures_to_replacement: false,
            },
            concurrency: crate::domain::ConcurrencyPolicy {
                max_concurrent: 1,
                allow_domain_overlap: false,
            },
            authority: crate::domain::AuthorityPolicy {
                may_reassign: false,
                may_reallocate_budget: false,
                may_change_information_topology: false,
                observe_only: true,
            },
            budget: crate::domain::TacticalBudget {
                max_tokens: 200,
                max_cost_microunits: 20_000,
                max_duration_ms: 10_000,
                max_plays: 1,
            },
            expected_effect: crate::domain::ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 300,
                tempo_delta_ppm: 0,
                information_delta_ppm: 600,
            },
            termination: crate::domain::TerminationCondition {
                max_plays: 1,
                abort_when: vec![crate::domain::AbortTrigger::BudgetExhausted],
            },
            counter_tactic: None,
        },
        SurveillanceKind::Bracket => crate::domain::TacticalDirective {
            id: playbook_core::TacticalId::new("t-bracket").expect("tactic id"),
            name: "bracket".into(),
            class: crate::domain::TacticalClass::Play,
            trigger: crate::domain::TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Elevated,
            },
            benefit_ppm: 600,
            cost_ppm: 300,
            risk_ppm: 100,
            shape: crate::domain::PersonnelShape {
                primary_executors: 1,
                support: 2,
                reserves: 0,
                observers: 0,
                assignments: vec![crate::domain::DomainAssignment {
                    domain: context
                        .critical_domain
                        .clone()
                        .unwrap_or_else(|| "critical-risk".into()),
                    personnel: vec![
                        PersonnelId::new("scanner").unwrap(),
                        PersonnelId::new("reviewer").unwrap(),
                    ],
                    mode: crate::domain::OwnershipMode::Joint,
                }],
            },
            information: crate::domain::InformationPolicy {
                max_context_overlap_ppm: 500,
                huddle_access: crate::domain::HuddleAccess::Sugar,
                disclose_failures_to_replacement: true,
            },
            concurrency: crate::domain::ConcurrencyPolicy {
                max_concurrent: 2,
                allow_domain_overlap: true,
            },
            authority: crate::domain::AuthorityPolicy {
                may_reassign: false,
                may_reallocate_budget: false,
                may_change_information_topology: true,
                observe_only: false,
            },
            budget: crate::domain::TacticalBudget {
                max_tokens: 5_000,
                max_cost_microunits: 500_000,
                max_duration_ms: 30_000,
                max_plays: 2,
            },
            expected_effect: crate::domain::ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 800,
                tempo_delta_ppm: 0,
                information_delta_ppm: 400,
            },
            termination: crate::domain::TerminationCondition {
                max_plays: 2,
                abort_when: vec![
                    crate::domain::AbortTrigger::BudgetExhausted,
                    crate::domain::AbortTrigger::WinConditionSatisfied,
                ],
            },
            counter_tactic: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape_with(spy_working: bool) -> PersonnelShape {
        let mut assignments = vec![crate::domain::DomainAssignment {
            domain: "runtime".into(),
            personnel: vec![person("subject")],
            mode: crate::domain::OwnershipMode::Sole,
        }];
        if spy_working {
            assignments.push(crate::domain::DomainAssignment {
                domain: "parser".into(),
                personnel: vec![person("watcher")],
                mode: crate::domain::OwnershipMode::Sole,
            });
        }
        PersonnelShape {
            primary_executors: 1,
            support: 0,
            reserves: 0,
            observers: usize::from(spy_working) as u16,
            assignments,
        }
    }

    fn person(id: &str) -> PersonnelId {
        PersonnelId::new(id).unwrap()
    }

    #[test]
    fn spy_assignment_refuses_self_and_working_spies() {
        // The spy watching itself is not surveillance.
        assert_eq!(
            assign_spy(person("w"), person("w"), "runtime", &shape_with(false)).unwrap_err(),
            tactics_refusal("spy_is_subject")
        );
        // A spy holding execution work is a worker, not a watcher.
        assert_eq!(
            assign_spy(
                person("watcher"),
                person("subject"),
                "runtime",
                &shape_with(true)
            )
            .unwrap_err(),
            tactics_refusal("spy_holds_execution_work")
        );
        // The clean assignment: idle watcher, working subject.
        let assignment = assign_spy(
            person("watcher"),
            person("subject"),
            "runtime",
            &shape_with(false),
        )
        .unwrap();
        assert_eq!(assignment.observer, person("watcher"));
        assert_eq!(assignment.subject, person("subject"));
    }

    #[test]
    fn drift_bands_follow_the_builtin_ladder() {
        let policy = DriftPolicy::builtin();
        assert_eq!(
            drift_band(
                &DriftReading {
                    scope_expansion_ppm: 0,
                    resource_use_ppm: 0,
                    unusual_decisions: 0,
                },
                &policy
            ),
            DriftBand::Calm
        );
        // One unusual decision below the limit is watch, not calm.
        assert_eq!(
            drift_band(
                &DriftReading {
                    scope_expansion_ppm: 0,
                    resource_use_ppm: 0,
                    unusual_decisions: 1,
                },
                &policy
            ),
            DriftBand::Watch
        );
        // At the watch threshold: watch.
        assert_eq!(
            drift_band(
                &DriftReading {
                    scope_expansion_ppm: 300,
                    resource_use_ppm: 0,
                    unusual_decisions: 0,
                },
                &policy
            ),
            DriftBand::Watch
        );
        // At the intervene threshold: intervene.
        assert_eq!(
            drift_band(
                &DriftReading {
                    scope_expansion_ppm: 700,
                    resource_use_ppm: 0,
                    unusual_decisions: 0,
                },
                &policy
            ),
            DriftBand::Intervene
        );
        // Reaching the unusual-decision limit intervenes.
        assert_eq!(
            drift_band(
                &DriftReading {
                    scope_expansion_ppm: 0,
                    resource_use_ppm: 0,
                    unusual_decisions: 3,
                },
                &policy
            ),
            DriftBand::Intervene
        );
    }

    #[test]
    fn intervention_recommends_but_never_substitutes() {
        assert_eq!(intervention(DriftBand::Calm), Intervention::None);
        assert_eq!(
            intervention(DriftBand::Watch),
            Intervention::IncreaseObservation
        );
        assert_eq!(
            intervention(DriftBand::Intervene),
            Intervention::HaltSubject
        );
        // SubstituteSubject belongs to the Bench; the watcher never names it.
        assert_ne!(
            intervention(DriftBand::Intervene),
            Intervention::SubstituteSubject
        );
    }

    #[test]
    fn brackets_need_two_different_controls_and_a_domain() {
        assert_eq!(
            bracket("security", person("scanner"), person("scanner")).unwrap_err(),
            tactics_refusal("bracket_controls_identical")
        );
        assert_eq!(
            bracket("  ", person("scanner"), person("reviewer")).unwrap_err(),
            tactics_refusal("bracket needs a domain")
        );
        let pair = bracket("security", person("scanner"), person("reviewer")).unwrap();
        assert_eq!(pair.domain, "security");
    }

    #[test]
    fn dual_control_coverage_requires_both_owners_on_the_domain() {
        let pair = bracket("security", person("scanner"), person("reviewer")).unwrap();
        let mut shape = crate::domain::PersonnelShape {
            primary_executors: 1,
            support: 2,
            reserves: 0,
            observers: 0,
            assignments: vec![crate::domain::DomainAssignment {
                domain: "security".into(),
                personnel: vec![person("scanner"), person("reviewer")],
                mode: crate::domain::OwnershipMode::Joint,
            }],
        };
        assert!(is_bracked("security", &pair, &shape));
        // One control missing: unbracked.
        shape.assignments[0]
            .personnel
            .retain(|id| id != &person("reviewer"));
        assert!(!is_bracked("security", &pair, &shape));
        // A different domain: unbracked.
        assert!(!is_bracked("payments", &pair, &shape));
    }

    #[test]
    fn the_spy_directive_is_observe_only_and_validates() {
        let context = crate::domain::TacticalContext {
            problem_breadth_ppm: 300,
            domain_coupling_ppm: 400,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures: 0,
            risk_band: RiskBand::Guarded,
            divergence_observed: false,
            critical_domain: None,
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 0,
        };
        let spy = directive(SurveillanceKind::Spy, &context);
        crate::admission::validated(&spy).unwrap();
        assert!(spy.authority.observe_only);
        assert_eq!(spy.shape.primary_executors, 0);
        assert_eq!(spy.shape.observers, 1);
        assert_eq!(spy.budget.max_plays, 1);
        assert!(matches!(
            spy.evaluate_admission(&context, 100),
            crate::admission::Admission::Admitted
        ));

        let mut critical = context;
        critical.risk_band = RiskBand::Critical;
        critical.critical_domain = Some("security".into());
        let bracket_directive = directive(SurveillanceKind::Bracket, &critical);
        crate::admission::validated(&bracket_directive).unwrap();
        assert!(bracket_directive.expected_effect.protection_delta_ppm >= 700);
        assert!(bracket_directive.shape.assignments[0].personnel.len() == 2);
    }

    fn context_at(band: RiskBand, critical_domain: Option<&str>) -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 300,
            domain_coupling_ppm: 400,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures: 0,
            risk_band: band,
            divergence_observed: false,
            critical_domain: critical_domain.map(String::from),
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 0,
        }
    }

    #[test]
    fn the_spy_only_reads_and_recommends() {
        // The adversarial rule: a spy that intervenes is the failure mode.
        // Every public function in this module takes shared references and
        // returns owned readings, bands, recommendations, and booleans —
        // none can reassign personnel, mutate a shape, or touch evidence.
        // The shape that goes in is the shape that comes out.
        let shape = shape_with(false);
        let snapshot = shape.clone();
        let _ = assign_spy(person("watcher"), person("subject"), "runtime", &shape).unwrap();
        let pair = bracket("runtime", person("watcher"), person("subject")).unwrap();
        assert!(!is_bracked("runtime", &pair, &shape));
        assert_eq!(shape, snapshot);

        // And the directive claims no acting authority at all.
        let spy = directive(SurveillanceKind::Spy, &context_at(RiskBand::Guarded, None));
        assert!(spy.authority.observe_only);
        assert!(!spy.authority.may_reassign);
        assert!(!spy.authority.may_reallocate_budget);
        assert!(!spy.authority.may_change_information_topology);

        // The watcher's whole output vocabulary is bands and
        // recommendations; substitution is never one of them.
        for band in [DriftBand::Calm, DriftBand::Watch, DriftBand::Intervene] {
            assert_ne!(intervention(band), Intervention::SubstituteSubject);
        }
    }

    #[test]
    fn drift_band_boundaries_are_exact() {
        // The frozen builtin ladder: watch at 300 ppm, intervene at 700 ppm,
        // intervene on three unusual decisions.
        let policy = DriftPolicy::builtin();
        assert_eq!(policy.watch_ppm, 300);
        assert_eq!(policy.intervene_ppm, 700);
        assert_eq!(policy.unusual_decision_limit, 3);
        let reading = |scope: u32, resource: u32, decisions: u32| DriftReading {
            scope_expansion_ppm: scope,
            resource_use_ppm: resource,
            unusual_decisions: decisions,
        };
        // One below watch is calm; exactly at watch is watch — both rates.
        assert_eq!(drift_band(&reading(299, 0, 0), &policy), DriftBand::Calm);
        assert_eq!(drift_band(&reading(0, 299, 0), &policy), DriftBand::Calm);
        assert_eq!(drift_band(&reading(300, 0, 0), &policy), DriftBand::Watch);
        assert_eq!(drift_band(&reading(0, 300, 0), &policy), DriftBand::Watch);
        // One below intervene is still watch; exactly at intervene intervenes.
        assert_eq!(drift_band(&reading(699, 0, 0), &policy), DriftBand::Watch);
        assert_eq!(drift_band(&reading(0, 699, 0), &policy), DriftBand::Watch);
        assert_eq!(
            drift_band(&reading(700, 0, 0), &policy),
            DriftBand::Intervene
        );
        assert_eq!(
            drift_band(&reading(0, 700, 0), &policy),
            DriftBand::Intervene
        );
        // Limit minus one unusual decision with quiet rates is watch; at the
        // limit it intervenes. A decision never calms a watched rate.
        assert_eq!(drift_band(&reading(0, 0, 2), &policy), DriftBand::Watch);
        assert_eq!(drift_band(&reading(0, 0, 3), &policy), DriftBand::Intervene);
        assert_eq!(drift_band(&reading(299, 0, 2), &policy), DriftBand::Watch);
    }

    #[test]
    fn every_band_maps_to_exactly_one_intervention() {
        // The mapping is total: all three bands, element for element.
        let bands = [DriftBand::Calm, DriftBand::Watch, DriftBand::Intervene];
        let expected = [
            Intervention::None,
            Intervention::IncreaseObservation,
            Intervention::HaltSubject,
        ];
        for (band, recommendation) in bands.iter().zip(expected.iter()) {
            assert_eq!(intervention(*band), *recommendation);
        }
        // And substitution is reserved for the Bench, for every band.
        for band in bands {
            assert_ne!(intervention(band), Intervention::SubstituteSubject);
        }
    }

    #[test]
    fn bracket_roles_are_the_callers_declaration() {
        // The struct carries two roles; which person fills the deterministic
        // role and which the generative role is declared by the caller, and
        // assembly inspects only distinctness of personnel, never the kind
        // of control each person actually is.
        let scanner = person("scanner");
        let reviewer = person("reviewer");
        let declared = bracket("security", scanner.clone(), reviewer.clone()).unwrap();
        let swapped = bracket("security", reviewer.clone(), scanner.clone()).unwrap();
        assert_eq!(declared.deterministic_control, scanner);
        assert_eq!(declared.generative_control, reviewer);
        assert_eq!(swapped.deterministic_control, reviewer);
        assert_eq!(swapped.generative_control, scanner);
        // Coverage reads the two roles symmetrically: each must own it.
        let shape = crate::domain::PersonnelShape {
            primary_executors: 1,
            support: 2,
            reserves: 0,
            observers: 0,
            assignments: vec![crate::domain::DomainAssignment {
                domain: "security".into(),
                personnel: vec![person("scanner"), person("reviewer")],
                mode: crate::domain::OwnershipMode::Joint,
            }],
        };
        assert!(is_bracked("security", &declared, &shape));
        assert!(is_bracked("security", &swapped, &shape));
    }

    #[test]
    fn is_bracked_negative_paths_and_split_ownership() {
        let pair = bracket("security", person("scanner"), person("reviewer")).unwrap();
        // An empty shape owns nothing: unbracked.
        let empty = crate::domain::PersonnelShape {
            primary_executors: 1,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: Vec::new(),
        };
        assert!(!is_bracked("security", &pair, &empty));
        // Ownership split across two assignments of the same domain still
        // counts: both controls hold the domain between them.
        let split = crate::domain::PersonnelShape {
            primary_executors: 2,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![
                crate::domain::DomainAssignment {
                    domain: "security".into(),
                    personnel: vec![person("scanner")],
                    mode: crate::domain::OwnershipMode::Sole,
                },
                crate::domain::DomainAssignment {
                    domain: "security".into(),
                    personnel: vec![person("reviewer")],
                    mode: crate::domain::OwnershipMode::Sole,
                },
            ],
        };
        assert!(is_bracked("security", &pair, &split));
        // A different domain: unbracked.
        assert!(!is_bracked("payments", &pair, &split));
    }

    #[test]
    fn surveillance_directives_admit_only_on_their_triggers() {
        // The spy fires from Guarded upward.
        let guarded = context_at(RiskBand::Guarded, None);
        let spy = directive(SurveillanceKind::Spy, &guarded);
        crate::admission::validated(&spy).unwrap();
        assert_eq!(
            spy.evaluate_admission(&guarded, 100),
            crate::admission::Admission::Admitted
        );
        // On a Low band the trigger misses and admission rejects by trigger.
        assert_eq!(
            spy.evaluate_admission(&context_at(RiskBand::Low, None), 100),
            crate::admission::Admission::Rejected {
                reason_code: "trigger_not_fired".into()
            }
        );
        // The bracket fires from Elevated upward.
        let elevated = context_at(RiskBand::Elevated, Some("security"));
        let bracket_directive = directive(SurveillanceKind::Bracket, &elevated);
        crate::admission::validated(&bracket_directive).unwrap();
        assert_eq!(
            bracket_directive.evaluate_admission(&elevated, 100),
            crate::admission::Admission::Admitted
        );
        assert_eq!(
            bracket_directive
                .evaluate_admission(&context_at(RiskBand::Guarded, Some("security")), 100),
            crate::admission::Admission::Rejected {
                reason_code: "trigger_not_fired".into()
            }
        );
    }
}
