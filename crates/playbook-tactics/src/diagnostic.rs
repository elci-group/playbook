//! The parallel diagnostic attack patterns: Mesh, Flood, and Levels.
//!
//! Where formations decide who covers breadth and conservative doctrines
//! decide who absorbs risk, the diagnostic patterns decide how a blocker is
//! *found*. Each is a football concept translated into a deterministic
//! planning function over personnel and domains (directive §6.15–§6.17):
//!
//! - Mesh (§6.15): two receivers run crossing routes through the same zone.
//!   Computationally, two agents independently traverse overlapping problem
//!   domains from opposite directions, so contradictory assumptions surface
//!   where the routes meet; the crossing point becomes the reconciliation
//!   target both readings must agree on.
//! - Flood (§6.16): a flood concept overloads one zone with routes at
//!   several depths. Computationally, one domain is attacked at multiple
//!   abstraction levels simultaneously; whichever depth reports the finding
//!   tells the staff which layer holds the blocker.
//! - Levels (§6.17): a levels concept stocks short, intermediate, and deep
//!   routes on the same play. Computationally, the same agents investigate
//!   one problem at progressively different granularity — diagnostic depth,
//!   not parallel implementation.
//!
//! All planning here is deterministic and total over its inputs: the same
//! roster and domain always produce the same plan, and every refusal names
//! its reason. Plans recommend; the existing authorities execute.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError, TacticalId};

use crate::domain::{
    tactics_refusal, AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment,
    ExpectedEffect, HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, TacticalBudget,
    TacticalClass, TacticalDirective, TacticalTrigger, TerminationCondition,
};

/// The three diagnostic attack patterns a staff may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackPattern {
    /// Two agents cross through one reconciliation domain from opposite
    /// sides.
    Mesh,
    /// One domain attacked at several abstraction levels at once.
    Flood,
    /// The same agents drilling one problem at progressively deeper
    /// granularity.
    Levels,
}

/// One crossing route: an agent leaves `from_domain` and runs toward
/// `toward_domain`. Two of these meeting in the same domain is a mesh.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CrossingAssignment {
    /// The agent running the crossing route.
    pub agent: PersonnelId,
    /// The domain the agent starts from.
    pub from_domain: String,
    /// The domain the agent runs into (the shared zone).
    pub toward_domain: String,
}

/// A mesh plan: two crossings whose routes meet, plus the domain where the
/// readings must reconcile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshPlan {
    /// The crossing routes, one per agent, in caller order.
    pub crossings: Vec<CrossingAssignment>,
    /// The shared domain both agents cross through; the readings compared
    /// here expose contradictory assumptions.
    pub reconciliation_target: String,
}

/// Mesh: two agents cross through the same reconciliation domain from
/// opposite sides. The crossing point is the reconciliation target.
pub fn plan_mesh(
    agent_a: PersonnelId,
    agent_b: PersonnelId,
    outer_a: &str,
    outer_b: &str,
    shared: &str,
) -> MeshPlan {
    MeshPlan {
        crossings: vec![
            CrossingAssignment {
                agent: agent_a,
                from_domain: outer_a.to_string(),
                toward_domain: shared.to_string(),
            },
            CrossingAssignment {
                agent: agent_b,
                from_domain: outer_b.to_string(),
                toward_domain: shared.to_string(),
            },
        ],
        reconciliation_target: shared.to_string(),
    }
}

/// The five abstraction levels of a diagnostic attack, from the shallowest
/// architecture reading to the deepest line reading. Depth rank is 0..=4
/// via [`AbstractionLevel::rank`]; the derived ordering matches rank order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbstractionLevel {
    /// Rank 0: the shape of the whole system and its boundaries.
    Architecture,
    /// Rank 1: crate responsibilities and dependency edges.
    Crate,
    /// Rank 2: module boundaries and internal cohesion.
    Module,
    /// Rank 3: individual function behaviour and contracts.
    Function,
    /// Rank 4: single lines — the deepest reading a flood or a levels run
    /// can take.
    Line,
}

impl AbstractionLevel {
    /// The depth rank, 0..=4: Architecture is 0, Line is 4.
    pub fn rank(self) -> u8 {
        match self {
            Self::Architecture => 0,
            Self::Crate => 1,
            Self::Module => 2,
            Self::Function => 3,
            Self::Line => 4,
        }
    }

    /// The snake_case label used in owned-domain names of a personnel
    /// shape.
    fn label(self) -> &'static str {
        match self {
            Self::Architecture => "architecture",
            Self::Crate => "crate",
            Self::Module => "module",
            Self::Function => "function",
            Self::Line => "line",
        }
    }
}

/// The level a depth ladder starts from (rank 0) and the level at rank
/// `depth % 5`; used by both the flood and the levels ladders.
fn level_at_depth(depth: usize) -> AbstractionLevel {
    match depth % 5 {
        1 => AbstractionLevel::Crate,
        2 => AbstractionLevel::Module,
        3 => AbstractionLevel::Function,
        4 => AbstractionLevel::Line,
        _ => AbstractionLevel::Architecture,
    }
}

/// One agent holding one depth of one domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthAssignment {
    /// The agent assigned to read at this depth.
    pub agent: PersonnelId,
    /// The abstraction level the agent is responsible for.
    pub level: AbstractionLevel,
}

/// A flood plan: many runners over one zone, one abstraction level each.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloodPlan {
    /// The single domain under attack.
    pub domain: String,
    /// The depth assignments, shallow-to-deep in plan order.
    pub assignments: Vec<DepthAssignment>,
}

/// Flood: assign agents to consecutive abstraction levels of ONE domain,
/// deepest-first allocation (first agent gets the SHALLOWEST level —
/// architecture — so coverage runs shallow→deep in assignment order).
/// Refuse when agents or domain empty; at most 5 agents (one per level);
/// extra agents are left out of the flood (documented, not an error).
///
/// A zone has exactly five depths, so the sixth and later runners have no
/// depth to hold: they are left out of the flood rather than refused.
pub fn plan_flood(domain: &str, agents: &[PersonnelId]) -> Result<FloodPlan, PlaybookError> {
    if domain.trim().is_empty() {
        return Err(tactics_refusal("flood names no domain"));
    }
    if agents.is_empty() {
        return Err(tactics_refusal("flood deploys no personnel"));
    }
    let assignments: Vec<DepthAssignment> = (0..agents.len().min(5))
        .map(|depth| DepthAssignment {
            agent: agents[depth].clone(),
            level: level_at_depth(depth),
        })
        .collect();
    Ok(FloodPlan {
        domain: domain.to_string(),
        assignments,
    })
}

/// A levels plan: the same target drilled at progressively deeper
/// granularity, round-robin across the depth ladder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelsPlan {
    /// The problem under investigation.
    pub target: String,
    /// The depth assignments; assignment i holds rank `i % 5`.
    pub assignments: Vec<DepthAssignment>,
}

/// Levels: the same agents at progressively DEEPER granularity over multiple
/// rounds — assignment i gets level rank (i % 5), so repeated rounds walk
/// the depth ladder. Refuse empty agents/target.
pub fn plan_levels(target: &str, agents: &[PersonnelId]) -> Result<LevelsPlan, PlaybookError> {
    if target.trim().is_empty() {
        return Err(tactics_refusal("levels name no target"));
    }
    if agents.is_empty() {
        return Err(tactics_refusal("levels deploy no personnel"));
    }
    let assignments: Vec<DepthAssignment> = agents
        .iter()
        .enumerate()
        .map(|(depth, agent)| DepthAssignment {
            agent: agent.clone(),
            level: level_at_depth(depth),
        })
        .collect();
    Ok(LevelsPlan {
        target: target.to_string(),
        assignments,
    })
}

/// Overlap detection: two plans conflict when the same personnel appears in
/// both. Returns the duplicated personnel ids, sorted, deduped.
pub fn overlapping_personnel(
    left: &[DepthAssignment],
    right: &[DepthAssignment],
) -> Vec<PersonnelId> {
    let mut overlap: Vec<PersonnelId> = left
        .iter()
        .filter(|assignment| right.iter().any(|other| other.agent == assignment.agent))
        .map(|assignment| assignment.agent.clone())
        .collect();
    overlap.sort();
    overlap.dedup();
    overlap
}

/// Merge semantics: combine two flood plans over the SAME domain into one,
/// deepest-level-first ordered; refuse different domains; deduplicate an
/// agent that appears in both plans at the SAME level (keep first), but
/// refuse an agent appearing at DIFFERENT levels (an agent cannot hold two
/// depths of one domain).
///
/// The merged ordering is deepest level first; equal ranks keep the left
/// plan's runners ahead of the right plan's (a stable sort).
pub fn merge_flood(left: FloodPlan, right: FloodPlan) -> Result<FloodPlan, PlaybookError> {
    if left.domain != right.domain {
        return Err(tactics_refusal(&format!(
            "cannot merge floods over {} and {}: one zone at a time",
            left.domain, right.domain
        )));
    }
    let mut assignments = left.assignments;
    for assignment in right.assignments {
        if let Some(held) = assignments
            .iter()
            .find(|kept| kept.agent == assignment.agent)
        {
            if held.level != assignment.level {
                return Err(tactics_refusal(&format!(
                    "{} cannot hold both {:?} and {:?} of {}",
                    assignment.agent, held.level, assignment.level, left.domain
                )));
            }
            // Same agent, same depth: a duplicate jersey; keep the first.
            continue;
        }
        assignments.push(assignment);
    }
    assignments.sort_by_key(|assignment| std::cmp::Reverse(assignment.level));
    Ok(FloodPlan {
        domain: left.domain,
        assignments,
    })
}

/// Saturating personnel count: a shape never counts above `u16::MAX`.
fn saturating_executors(count: usize) -> u16 {
    u16::try_from(count).unwrap_or(u16::MAX)
}

/// Build a personnel shape from `(owned domain, agent)` pairs: one sole
/// assignment per pair, the pair count as executors, nobody in support,
/// reserves, or surveillance.
fn shape_over(owned: impl IntoIterator<Item = (String, PersonnelId)>) -> PersonnelShape {
    let assignments: Vec<DomainAssignment> = owned
        .into_iter()
        .map(|(domain, agent)| DomainAssignment {
            domain,
            personnel: vec![agent],
            mode: OwnershipMode::Sole,
        })
        .collect();
    let executors = saturating_executors(assignments.len());
    PersonnelShape {
        primary_executors: executors,
        support: 0,
        reserves: 0,
        observers: 0,
        assignments,
    }
}

/// The shared body of the three diagnostic directives: class Play, sugar
/// huddle (only changed facts cross at the meeting point), overlapping
/// domains allowed (the patterns exist to overlap them), no reassignment
/// authority (diagnostics recommend; authorities execute), and a tiny
/// budget — the budget funds a single diagnostic pass (`max_plays` 1) while
/// termination tolerates two plays, so one repeat of the pass is possible
/// before the abort triggers govern.
fn diagnostic_directive(
    name: &str,
    trigger: TacticalTrigger,
    shape: PersonnelShape,
    abort_when: Vec<AbortTrigger>,
) -> TacticalDirective {
    let max_concurrent = shape.primary_executors;
    TacticalDirective {
        id: TacticalId::new(format!("t-{name}"))
            .expect("diagnostic tactic names are static identifiers"),
        name: name.to_string(),
        class: TacticalClass::Play,
        trigger,
        benefit_ppm: 700,
        cost_ppm: 300,
        risk_ppm: 200,
        shape,
        information: InformationPolicy {
            max_context_overlap_ppm: 300,
            huddle_access: HuddleAccess::Sugar,
            disclose_failures_to_replacement: true,
        },
        concurrency: ConcurrencyPolicy {
            max_concurrent,
            allow_domain_overlap: true,
        },
        authority: AuthorityPolicy {
            may_reassign: false,
            may_reallocate_budget: false,
            may_change_information_topology: true,
            observe_only: false,
        },
        budget: TacticalBudget {
            max_tokens: 20_000,
            max_cost_microunits: 2_000_000,
            max_duration_ms: 60_000,
            max_plays: 1,
        },
        expected_effect: ExpectedEffect {
            breadth_delta_ppm: 300,
            protection_delta_ppm: 0,
            tempo_delta_ppm: 200,
            information_delta_ppm: 800,
        },
        termination: TerminationCondition {
            max_plays: 2,
            abort_when,
        },
        counter_tactic: None,
    }
}

/// Build the directive for one diagnostic attack pattern over `domain` with
/// `agents`. Names are the matrix names `mesh`, `flood`, and `levels`; the
/// class is Play; the trigger is deterministic per pattern (`Any` of
/// `DivergenceObserved` for mesh, `CriticalDomainNamed` for flood,
/// `ProblemBreadthAtLeast` 500 ppm for levels); the information claim is
/// high (800 ppm, diagnostics spend to learn); the shape comes from the
/// plan with sole ownership per level/domain; and mesh termination
/// includes `DivergenceExceedsTolerance` — the crossing must reconcile or
/// abort. Mesh needs exactly two crossers; flood and levels refuse empty
/// rosters through their planners.
pub fn directive(
    pattern: AttackPattern,
    domain: &str,
    agents: &[PersonnelId],
) -> Result<TacticalDirective, PlaybookError> {
    if domain.trim().is_empty() {
        return Err(tactics_refusal("diagnostic names no domain"));
    }
    match pattern {
        AttackPattern::Mesh => {
            let [agent_a, agent_b, ..] = agents else {
                return Err(tactics_refusal("mesh needs two agents to cross"));
            };
            // The frozen contract has no measured outer domains at
            // directive time, so each crosser starts from a deterministically
            // named approach lane of the shared domain.
            let plan = plan_mesh(
                agent_a.clone(),
                agent_b.clone(),
                &format!("{domain}.approach.a"),
                &format!("{domain}.approach.b"),
                domain,
            );
            let shape = shape_over(
                plan.crossings
                    .iter()
                    .map(|crossing| (crossing.toward_domain.clone(), crossing.agent.clone())),
            );
            Ok(diagnostic_directive(
                "mesh",
                TacticalTrigger::Any(vec![TacticalTrigger::DivergenceObserved]),
                shape,
                vec![
                    AbortTrigger::BudgetExhausted,
                    AbortTrigger::DivergenceExceedsTolerance,
                ],
            ))
        }
        AttackPattern::Flood => {
            let plan = plan_flood(domain, agents)?;
            let shape = shape_over(plan.assignments.iter().map(|assignment| {
                (
                    format!("{}/{}", plan.domain, assignment.level.label()),
                    assignment.agent.clone(),
                )
            }));
            Ok(diagnostic_directive(
                "flood",
                TacticalTrigger::CriticalDomainNamed,
                shape,
                vec![AbortTrigger::BudgetExhausted],
            ))
        }
        AttackPattern::Levels => {
            let plan = plan_levels(domain, agents)?;
            let shape = shape_over(plan.assignments.iter().map(|assignment| {
                (
                    format!("{}/{}", plan.target, assignment.level.label()),
                    assignment.agent.clone(),
                )
            }));
            Ok(diagnostic_directive(
                "levels",
                TacticalTrigger::ProblemBreadthAtLeast { ppm: 500 },
                shape,
                vec![AbortTrigger::BudgetExhausted],
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RiskBand, TacticalContext};

    fn agent(name: &str) -> PersonnelId {
        PersonnelId::new(name).unwrap()
    }

    fn roster() -> Vec<PersonnelId> {
        ["a", "b", "c", "d", "e", "f", "g"]
            .into_iter()
            .map(agent)
            .collect()
    }

    fn ranks(plan: &[DepthAssignment]) -> Vec<u8> {
        plan.iter()
            .map(|assignment| assignment.level.rank())
            .collect()
    }

    #[test]
    fn mesh_crossings_converge_on_the_reconciliation_target() {
        let plan = plan_mesh(agent("x"), agent("y"), "frontend", "storage", "parser");
        assert_eq!(plan.crossings.len(), 2);
        assert_eq!(plan.crossings[0].agent, agent("x"));
        assert_eq!(plan.crossings[0].from_domain, "frontend");
        assert_eq!(plan.crossings[0].toward_domain, "parser");
        assert_eq!(plan.crossings[1].agent, agent("y"));
        assert_eq!(plan.crossings[1].from_domain, "storage");
        assert_eq!(plan.crossings[1].toward_domain, "parser");
        assert_eq!(plan.reconciliation_target, "parser");
        // The same call always returns the same plan.
        assert_eq!(
            plan,
            plan_mesh(agent("x"), agent("y"), "frontend", "storage", "parser")
        );
    }

    #[test]
    fn flood_allocates_shallow_first_and_refuses_empty_inputs() {
        let roster = roster();
        let plan = plan_flood("parser", &roster[..4]).unwrap();
        assert_eq!(plan.domain, "parser");
        assert_eq!(plan.assignments[0].level, AbstractionLevel::Architecture);
        assert_eq!(plan.assignments[0].agent, agent("a"));
        assert_eq!(ranks(&plan.assignments), vec![0, 1, 2, 3]);
        // The same roster always floods the same way.
        assert_eq!(plan, plan_flood("parser", &roster[..4]).unwrap());
        assert!(plan_flood("  ", &roster).is_err());
        assert!(plan_flood("parser", &[]).is_err());
    }

    #[test]
    fn flood_caps_the_zone_at_five_depths_and_leaves_extras_out() {
        let roster = roster();
        let plan = plan_flood("parser", &roster).unwrap();
        assert_eq!(plan.assignments.len(), 5);
        assert_eq!(plan.assignments[4].level, AbstractionLevel::Line);
        // The sixth runner is left out, not refused.
        assert!(!plan.assignments.iter().any(|a| a.agent == agent("f")));
        assert_eq!(plan, plan_flood("parser", &roster).unwrap());
    }

    #[test]
    fn levels_round_robin_walks_the_depth_ladder() {
        let roster = roster();
        let plan = plan_levels("refactor-parser", &roster).unwrap();
        assert_eq!(plan.target, "refactor-parser");
        // Seven agents: ranks 0..=4, then the ladder restarts at 0.
        assert_eq!(ranks(&plan.assignments), vec![0, 1, 2, 3, 4, 0, 1]);
        assert_eq!(plan.assignments[5].level, AbstractionLevel::Architecture);
        assert_eq!(plan, plan_levels("refactor-parser", &roster).unwrap());
        assert!(plan_levels("  ", &roster).is_err());
        assert!(plan_levels("parser", &[]).is_err());
    }

    #[test]
    fn overlapping_personnel_finds_sorted_deduped_overlap() {
        let left = vec![
            DepthAssignment {
                agent: agent("a"),
                level: AbstractionLevel::Architecture,
            },
            DepthAssignment {
                agent: agent("b"),
                level: AbstractionLevel::Crate,
            },
            DepthAssignment {
                agent: agent("a"),
                level: AbstractionLevel::Line,
            },
        ];
        let right = vec![
            DepthAssignment {
                agent: agent("c"),
                level: AbstractionLevel::Module,
            },
            DepthAssignment {
                agent: agent("a"),
                level: AbstractionLevel::Function,
            },
            DepthAssignment {
                agent: agent("b"),
                level: AbstractionLevel::Line,
            },
        ];
        // "a" appears twice on each side but is reported once; sorted.
        assert_eq!(
            overlapping_personnel(&left, &right),
            vec![agent("a"), agent("b")]
        );
        assert!(overlapping_personnel(&left, &[]).is_empty());
    }

    #[test]
    fn merge_flood_combines_one_domain_deepest_level_first() {
        let left = plan_flood("parser", &[agent("a"), agent("b"), agent("c")]).unwrap();
        let right = plan_flood("parser", &[agent("d"), agent("e")]).unwrap();
        let merged = merge_flood(left, right).unwrap();
        assert_eq!(merged.domain, "parser");
        assert_eq!(merged.assignments.len(), 5);
        assert_eq!(ranks(&merged.assignments), vec![2, 1, 1, 0, 0]);
        // Equal ranks stay stable: the left plan's runner comes first.
        assert_eq!(merged.assignments[1].agent, agent("b"));
        assert_eq!(merged.assignments[2].agent, agent("e"));
        assert_eq!(merged.assignments[3].agent, agent("a"));
        assert_eq!(merged.assignments[4].agent, agent("d"));
    }

    #[test]
    fn merge_flood_refuses_different_domains() {
        let left = plan_flood("parser", &[agent("a")]).unwrap();
        let right = plan_flood("storage", &[agent("b")]).unwrap();
        assert!(merge_flood(left, right).is_err());
    }

    #[test]
    fn merge_flood_deduplicates_an_agent_at_the_same_level() {
        let left = FloodPlan {
            domain: "parser".into(),
            assignments: vec![
                DepthAssignment {
                    agent: agent("a"),
                    level: AbstractionLevel::Architecture,
                },
                DepthAssignment {
                    agent: agent("b"),
                    level: AbstractionLevel::Function,
                },
            ],
        };
        let right = FloodPlan {
            domain: "parser".into(),
            assignments: vec![
                // "b" again at the same depth: a duplicate jersey, kept once.
                DepthAssignment {
                    agent: agent("b"),
                    level: AbstractionLevel::Function,
                },
                DepthAssignment {
                    agent: agent("c"),
                    level: AbstractionLevel::Line,
                },
            ],
        };
        let merged = merge_flood(left, right).unwrap();
        assert_eq!(merged.assignments.len(), 3);
        let runners: Vec<&str> = merged
            .assignments
            .iter()
            .map(|assignment| assignment.agent.as_str())
            .collect();
        assert_eq!(runners, vec!["c", "b", "a"]);
    }

    #[test]
    fn merge_flood_refuses_an_agent_at_two_depths_of_one_domain() {
        let left = FloodPlan {
            domain: "parser".into(),
            assignments: vec![DepthAssignment {
                agent: agent("a"),
                level: AbstractionLevel::Architecture,
            }],
        };
        let right = FloodPlan {
            domain: "parser".into(),
            assignments: vec![DepthAssignment {
                agent: agent("a"),
                level: AbstractionLevel::Function,
            }],
        };
        assert!(merge_flood(left, right).is_err());
    }

    #[test]
    fn diagnostic_directives_pass_validation_and_fire_on_their_triggers() {
        let roster = roster();
        let mesh = directive(AttackPattern::Mesh, "parser", &roster[..2]).unwrap();
        let flood = directive(AttackPattern::Flood, "parser", &roster[..3]).unwrap();
        let levels = directive(AttackPattern::Levels, "parser", &roster[..4]).unwrap();
        for built in [&mesh, &flood, &levels] {
            crate::admission::validated(built).unwrap();
            assert_eq!(built.class, TacticalClass::Play);
            assert!(built.expected_effect.information_delta_ppm >= 600);
            assert_eq!(built.budget.max_plays, 1);
            assert_eq!(built.termination.max_plays, 2);
            assert!(built
                .shape
                .assignments
                .iter()
                .all(|assignment| assignment.mode == OwnershipMode::Sole));
        }
        assert_eq!(mesh.name, "mesh");
        assert_eq!(flood.name, "flood");
        assert_eq!(levels.name, "levels");
        assert!(mesh
            .termination
            .abort_when
            .contains(&AbortTrigger::DivergenceExceedsTolerance));
        // Every trigger fires on its own reading and stays silent otherwise.
        let mut context = TacticalContext {
            problem_breadth_ppm: 500,
            domain_coupling_ppm: 300,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures: 0,
            risk_band: RiskBand::Low,
            divergence_observed: true,
            critical_domain: Some("parser".into()),
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 0,
        };
        assert!(mesh.trigger.evaluate(&context));
        assert!(flood.trigger.evaluate(&context));
        assert!(levels.trigger.evaluate(&context));
        context.divergence_observed = false;
        context.critical_domain = None;
        context.problem_breadth_ppm = 499;
        assert!(!mesh.trigger.evaluate(&context));
        assert!(!flood.trigger.evaluate(&context));
        assert!(!levels.trigger.evaluate(&context));
        // Mesh needs two crossers and a named zone.
        assert!(directive(AttackPattern::Mesh, "parser", &roster[..1]).is_err());
        assert!(directive(AttackPattern::Mesh, "  ", &roster).is_err());
        // The same inputs always build the same directive.
        assert_eq!(
            mesh,
            directive(AttackPattern::Mesh, "parser", &roster[..2]).unwrap()
        );
    }
}
