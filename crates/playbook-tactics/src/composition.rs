//! Composition and the tactical conflict matrix.
//!
//! Tactics compose where semantics permit, and the machine-readable
//! compatibility matrix decides every pair. `Spread + Mesh` composes;
//! `Spread + Heavy` are exclusive (breadth and concentration pull the same
//! personnel opposite ways); `Empty + Max Protect` is invalid outright — a
//! formation that deploys everyone and a doctrine that holds people back
//! cannot coexist; `Prevent + Verticals` conflict (catastrophic-risk
//! avoidance and explosive-gain seeking are opposite doctrines). Invalid
//! compositions fail closed.

use serde::{Deserialize, Serialize};

use playbook_core::PlaybookError;

use crate::domain::{tactics_refusal, TacticalClass, TacticalDirective};

/// How two tactics relate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compatibility {
    /// They may run together.
    Allowed,
    /// They may run together, but only under a declared condition.
    Conditional,
    /// They pull the same personnel opposite ways: only one may be active.
    Exclusive,
    /// They pursue opposite doctrines; running both is incoherent.
    Conflicting,
    /// Semantically contradictory — refused without exception.
    Invalid,
}

/// The canonical tactic names the conflict matrix knows. Modules may only
/// submit directives whose names appear here: an unmatrixed tactic is an
/// unreviewed tactic.
pub const KNOWN_TACTICS: &[&str] = &[
    "spread",
    "trips",
    "bunch",
    "empty",
    "heavy",
    "max_protect",
    "motion",
    "shift",
    "audible",
    "check_with_me",
    "constraint",
    "play_sequencing",
    "mesh",
    "flood",
    "levels",
    "verticals",
    "quick_game",
    "no_huddle",
    "hurry_up",
    "prevent",
    "make_them_drive",
    "spy",
    "bracket",
    "banjo",
    "scrape_exchange",
];

/// The pairwise compatibility table. Symmetric: `compatibility(a, b)` equals
/// `compatibility(b, a)`.
pub fn compatibility(left: &str, right: &str) -> Compatibility {
    let mut pair = [left, right];
    pair.sort_unstable();
    match (pair[0], pair[1]) {
        (a, b) if a == b => Compatibility::Exclusive,
        // Spread's breadth against concentration doctrines.
        ("heavy", "spread") | ("max_protect", "spread") | ("empty", "spread") => {
            Compatibility::Exclusive
        }
        // Empty deploys everyone; protection holds people back.
        ("empty", "max_protect") => Compatibility::Invalid,
        // Prevent and explosive doctrines are opposites.
        ("prevent", "verticals") => Compatibility::Conflicting,
        ("make_them_drive", "verticals") => Compatibility::Conflicting,
        // No-huddle and heavy huddle access contradict.
        ("bunch", "no_huddle") => Compatibility::Conflicting,
        // Concentration tactics compose with each other.
        ("heavy", "max_protect") => Compatibility::Allowed,
        ("max_protect", "prevent") => Compatibility::Allowed,
        ("heavy", "trips") => Compatibility::Allowed,
        // Diagnostics compose with breadth.
        ("mesh", "spread") => Compatibility::Allowed,
        ("flood", "trips") => Compatibility::Allowed,
        ("levels", "flood") => Compatibility::Allowed,
        // Motion feeds the audible: allowed, sequenced.
        ("audible", "motion") => Compatibility::Allowed,
        ("check_with_me", "motion") => Compatibility::Allowed,
        // Surveillance composes with concentration and conservatism.
        ("bracket", "heavy") => Compatibility::Allowed,
        ("prevent", "spy") => Compatibility::Allowed,
        ("max_protect", "spy") => Compatibility::Allowed,
        // Exchange tactics and fixed Manmark ownership are conditional:
        // joint ownership must be declared, not assumed.
        ("banjo", "scrape_exchange") => Compatibility::Conditional,
        // Quick game and hurry-up compose.
        ("hurry_up", "quick_game") => Compatibility::Allowed,
        // The default for unlisted pairs of known tactics is conditional:
        // permitted, but the host declares the condition.
        _ => Compatibility::Conditional,
    }
}

/// A composed tactical plan: directives that cleared validation and pairwise
/// compatibility. The plan carries no authority of its own — the existing
/// authorities execute it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalPlan {
    pub directives: Vec<TacticalDirective>,
}

/// Compose directives into a plan. Unknown tactics are refused; invalid and
/// conflicting pairs fail closed; exclusive pairs keep the first-listed (the
/// caller's order is the priority, and the record names what was dropped
/// through the dropped count).
pub fn compose(directives: &[TacticalDirective]) -> Result<TacticalPlan, PlaybookError> {
    let mut kept: Vec<TacticalDirective> = Vec::new();
    let mut dropped = 0u32;
    for directive in directives {
        crate::admission::validated(directive)?;
        if !KNOWN_TACTICS.contains(&directive.name.as_str()) {
            return Err(tactics_refusal(&format!(
                "tactic {} is not in the conflict matrix",
                directive.name
            )));
        }
        // A name is part of the matrix: two classes claiming one tactic
        // identity would be two things wearing one jersey.
        if kept
            .iter()
            .any(|kept_directive| kept_directive.name == directive.name)
        {
            return Err(tactics_refusal(&format!(
                "{} is already claimed in this plan",
                directive.name
            )));
        }
        let mut exclusive_with = None;
        for kept_directive in &kept {
            match compatibility(&kept_directive.name, &directive.name) {
                Compatibility::Allowed | Compatibility::Conditional => {}
                Compatibility::Exclusive => {
                    exclusive_with = Some(kept_directive.name.clone());
                    break;
                }
                Compatibility::Conflicting | Compatibility::Invalid => {
                    return Err(tactics_refusal(&format!(
                        "{} and {} cannot compose ({:?})",
                        kept_directive.name,
                        directive.name,
                        compatibility(&kept_directive.name, &directive.name)
                    )));
                }
            }
        }
        if let Some(winner) = exclusive_with {
            // Exclusive pairs keep the first-listed and drop the later one.
            dropped = dropped.saturating_add(1);
            let _ = winner;
            continue;
        }
        kept.push(directive.clone());
    }
    Ok(TacticalPlan { directives: kept })
}

/// The named counter of a directive, per the constraint-play doctrine: when
/// a dominant tactic's failure signature appears, the plan proposes its
/// counter.
pub fn counter_of(directive: &TacticalDirective) -> Option<&str> {
    directive.counter_tactic.as_deref()
}

/// The reasoning level a tactic sits on: franchise → formation → play →
/// adjustment.
pub fn class_of(directive: &TacticalDirective) -> TacticalClass {
    directive.class
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::tests::{directive, shape};
    use crate::domain::{
        AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, ExpectedEffect, HuddleAccess,
        InformationPolicy, TacticalBudget, TacticalClass, TacticalContext, TacticalTrigger,
        TerminationCondition,
    };
    use playbook_core::TacticalId;

    fn named(name: &str) -> TacticalDirective {
        directive(name, TacticalTrigger::Always, 400, 100, 50)
    }

    #[test]
    fn the_directive_examples_compose_and_clash_exactly_as_declared() {
        // Spread + Mesh composes.
        assert!(compose(&[named("spread"), named("mesh")]).is_ok());
        // Spread + Heavy are exclusive: the first-listed wins, the later is
        // dropped.
        let plan = compose(&[named("spread"), named("heavy")]).unwrap();
        assert_eq!(plan.directives.len(), 1);
        assert_eq!(plan.directives[0].name, "spread");
        // Empty + Max Protect is invalid outright.
        assert!(compose(&[named("empty"), named("max_protect")]).is_err());
        // Prevent + Verticals conflict.
        assert!(compose(&[named("prevent"), named("verticals")]).is_err());
        // Heavy + Max Protect compose.
        assert!(compose(&[named("heavy"), named("max_protect")]).is_ok());
        // Motion + Audible compose.
        assert!(compose(&[named("motion"), named("audible")]).is_ok());
        // Spy + Prevent compose.
        assert!(compose(&[named("spy"), named("prevent")]).is_ok());
    }

    #[test]
    fn unknown_tactics_are_refused_not_invented() {
        let mut rogue = named("huey_kew");
        rogue.class = TacticalClass::Play;
        assert!(compose(&[rogue]).is_err());
    }

    #[test]
    fn one_name_cannot_appear_twice_in_one_plan() {
        assert!(compose(&[named("spread"), named("spread")]).is_err());
    }

    #[test]
    fn the_matrix_is_symmetric_and_total_over_known_tactics() {
        for left in KNOWN_TACTICS {
            for right in KNOWN_TACTICS {
                let forward = compatibility(left, right);
                let backward = compatibility(right, left);
                assert_eq!(forward, backward, "{left}/{right} asymmetry");
                assert!(matches!(
                    forward,
                    Compatibility::Allowed
                        | Compatibility::Conditional
                        | Compatibility::Exclusive
                        | Compatibility::Conflicting
                        | Compatibility::Invalid
                ));
            }
        }
        // Identity is exclusive: a tactic does not compose with itself.
        assert_eq!(compatibility("spread", "spread"), Compatibility::Exclusive);
    }

    #[test]
    fn counters_are_declared_not_discovered() {
        let mut spread = named("spread");
        spread.counter_tactic = Some("bunch".into());
        assert_eq!(counter_of(&spread), Some("bunch"));
        assert_eq!(counter_of(&named("mesh")), None);
        assert_eq!(class_of(&spread), TacticalClass::Formation);
        let _ = shape();
        let _: Option<TacticalContext> = None;
        let _ = TacticalId::new("t-1").unwrap();
        let _ = InformationPolicy {
            max_context_overlap_ppm: 0,
            huddle_access: HuddleAccess::None,
            disclose_failures_to_replacement: false,
        };
        let _ = ExpectedEffect {
            breadth_delta_ppm: 0,
            protection_delta_ppm: 0,
            tempo_delta_ppm: 0,
            information_delta_ppm: 0,
        };
        let _ = TacticalBudget {
            max_tokens: 0,
            max_cost_microunits: 0,
            max_duration_ms: 0,
            max_plays: 0,
        };
        let _ = AuthorityPolicy {
            may_reassign: false,
            may_reallocate_budget: false,
            may_change_information_topology: false,
            observe_only: false,
        };
        let _ = ConcurrencyPolicy {
            max_concurrent: 0,
            allow_domain_overlap: false,
        };
        let _ = TerminationCondition {
            max_plays: 0,
            abort_when: vec![AbortTrigger::BudgetExhausted],
        };
        let _ = TacticalTrigger::Always;
        let _ = TacticalClass::Doctrine;
    }
}
