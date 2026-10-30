//! The Bench: the deterministic substitution service (FC-4).
//!
//! Substitution is a service, not a debate. The same measured inputs always
//! decide the same way: an ordered ladder of five classes, most urgent
//! first, every rung carrying machine reason codes. Inference never sits on
//! this ladder — the ladder reads measured fields and picks.
//!
//! The ladder, evaluated top to bottom. A rung fires only when its
//! situational condition holds *and* an eligible replacement exists; if no
//! eligible replacement exists the ladder keeps descending, and a ladder
//! that reaches its floor keeps the starter playing ([`decide`] returns
//! [`None`]).
//!
//! 1. **Emergency** — `emergency` is declared and the starter is not
//!    available, or the starter's provider health sits below
//!    [`MIN_PROVIDER_HEALTH_PPM`]. Best eligible replacement wins.
//! 2. **Availability** — the starter is not available, or its provider
//!    health sits below [`MIN_PROVIDER_HEALTH_PPM`]. Best eligible
//!    replacement wins.
//! 3. **Performance** — the starter has accumulated at least
//!    [`PERFORMANCE_FAILURE_FLOOR`] recent gate failures and a replacement
//!    exists with strictly fewer failures and fit at least the starter's.
//! 4. **Economic** — the remaining drive budget's monetary cost is at most
//!    half the initial grant's (`remaining * 2 <= initial`, saturating) and
//!    the starter costs more than zero per play, and a replacement exists
//!    costing at most half the starter's per-play cost with fit at least
//!    the play's [`BenchInput::required_fit_ppm`].
//! 5. **Tactical** — the starter's capability fit sits below the play's
//!    requirement and a replacement meets the requirement. Fit decides;
//!    prestige does not — personnel kind and tier are never consulted.
//!
//! A replacement is eligible only while it is `available` (`false` covers
//! both benched and unavailable roster states), is not carrying an
//! unresolved invocation (`busy` — default policy: unresolved means no
//! reassignment until reconciliation), and is never the starter
//! itself. Every candidate is ranked by the same total order — fit
//! descending, provider health descending, gate failures ascending, per-play
//! cost ascending, personnel id ascending — so bench order never matters
//! and ties always resolve to the lower personnel id.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

use playbook_core::{DriveBudget, Personnel, PersonnelId};

/// Provider health at or below which a starter is treated as fallen. Below
/// this floor the availability rung fires; exactly at it the starter is
/// still playable.
pub const MIN_PROVIDER_HEALTH_PPM: u32 = 100_000;

/// Recent gate failures at which the performance rung fires.
pub const PERFORMANCE_FAILURE_FLOOR: u32 = 3;

/// Why a substitution happened. Five classes, per the directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubstitutionClass {
    /// The measured fit no longer covers the play's requirement.
    Tactical,
    /// Repeated gate failures made the starter the weakest signal on the field.
    Performance,
    /// The starter cannot take the next snap at all.
    Availability,
    /// Half the drive's money is spent and a cheaper qualified player exists.
    Economic,
    /// A critical game state demands an immediate replacement.
    Emergency,
}

/// Everything measured about one roster member relevant to substitution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchCandidate {
    pub personnel: Personnel,
    /// Measured provider health, `0..=1_000_000`.
    pub provider_health_ppm: u32,
    /// Recent gate failures attributed to this member.
    pub recent_gate_failures: u32,
    /// Measured cost of one play by this member, in monetary micros.
    pub cost_micros_per_play: u64,
    /// Ready in the roster sense; `false` covers benched and unavailable.
    pub available: bool,
    /// The member has an unresolved invocation: its effect occurrence is
    /// unknown. Default policy forbids reassignment until reconciliation.
    pub busy: bool,
}

/// The measured situation the ladder decides over.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchInput {
    pub current: BenchCandidate,
    /// The play's measured capability requirement, `0..=1_000_000`.
    pub required_fit_ppm: u32,
    pub budget_remaining: DriveBudget,
    pub budget_initial: DriveBudget,
    /// A critical game state demands immediate replacement.
    pub emergency: bool,
    /// Measured stagnation context. The ladder itself never consults it;
    /// callers journal it alongside the decision.
    pub stall_cycles: u32,
}

/// One decided substitution. `replacement` is always `Some` — the ladder
/// never fires without an eligible replacement — but the field stays
/// optional to keep the record honest about the keep-playing default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionDecision {
    pub class: SubstitutionClass,
    pub out: PersonnelId,
    pub replacement: Option<PersonnelId>,
    /// Machine reason codes, in ladder order.
    pub reasons: Vec<String>,
}

/// The total ranking order over candidates: fit, health, failures, cost,
/// then personnel id ascending. Lower is better.
fn rank(candidate: &BenchCandidate) -> (Reverse<u32>, Reverse<u32>, u32, u64, PersonnelId) {
    (
        Reverse(candidate.personnel.capability_fit_ppm),
        Reverse(candidate.provider_health_ppm),
        candidate.recent_gate_failures,
        candidate.cost_micros_per_play,
        candidate.personnel.id.clone(),
    )
}

/// The best eligible replacement under the total ranking order, or none.
/// Eligibility requires an available member who is not the starter and who
/// satisfies the rung's own qualification.
fn best_replacement(
    bench: &[BenchCandidate],
    current: &PersonnelId,
    qualifies: impl Fn(&BenchCandidate) -> bool,
) -> Option<PersonnelId> {
    bench
        .iter()
        .filter(|candidate| {
            candidate.available && !candidate.busy && &candidate.personnel.id != current
        })
        .filter(|candidate| qualifies(candidate))
        .min_by_key(|candidate| rank(candidate))
        .map(|candidate| candidate.personnel.id.clone())
}

/// Deterministic ladder, most urgent first; the same inputs always decide
/// the same way. Every branch carries machine reason codes.
pub fn decide(input: &BenchInput, bench: &[BenchCandidate]) -> Option<SubstitutionDecision> {
    let current = &input.current;
    let current_id = &current.personnel.id;
    // Default policy (busy exclusion): an unresolved invocation means the
    // player is busy — its effect occurrence is unknown, and substituting
    // around an unknown effect corrupts the record. Reconciliation comes
    // first; the ladder does not run at all until then.
    if current.busy {
        return None;
    }
    let starter_fallen =
        !current.available || current.provider_health_ppm < MIN_PROVIDER_HEALTH_PPM;

    if input.emergency && starter_fallen {
        if let Some(replacement) = best_replacement(bench, current_id, |_| true) {
            return Some(SubstitutionDecision {
                class: SubstitutionClass::Emergency,
                out: current_id.clone(),
                replacement: Some(replacement.clone()),
                reasons: vec![
                    "emergency_declared".into(),
                    if current.available {
                        "provider_health_below_floor".into()
                    } else {
                        "starter_unavailable".into()
                    },
                    "replacement_best_by_fit_health_failures_cost".into(),
                ],
            });
        }
    }

    if starter_fallen {
        if let Some(replacement) = best_replacement(bench, current_id, |_| true) {
            return Some(SubstitutionDecision {
                class: SubstitutionClass::Availability,
                out: current_id.clone(),
                replacement: Some(replacement.clone()),
                reasons: vec![
                    if current.available {
                        "provider_health_below_floor".into()
                    } else {
                        "starter_unavailable".into()
                    },
                    "replacement_best_by_fit_health_failures_cost".into(),
                ],
            });
        }
    }

    if current.recent_gate_failures >= PERFORMANCE_FAILURE_FLOOR {
        let floor = current.recent_gate_failures;
        let starter_fit = current.personnel.capability_fit_ppm;
        if let Some(replacement) = best_replacement(bench, current_id, |candidate| {
            candidate.recent_gate_failures < floor
                && candidate.personnel.capability_fit_ppm >= starter_fit
        }) {
            return Some(SubstitutionDecision {
                class: SubstitutionClass::Performance,
                out: current_id.clone(),
                replacement: Some(replacement.clone()),
                reasons: vec![
                    "gate_failures_at_floor".into(),
                    "replacement_has_fewer_failures".into(),
                    "replacement_fit_covers_starter".into(),
                ],
            });
        }
    }

    let half_spent = input
        .budget_remaining
        .monetary_cost_micros
        .saturating_mul(2)
        <= input.budget_initial.monetary_cost_micros;
    if half_spent && current.cost_micros_per_play > 0 {
        let starter_cost = current.cost_micros_per_play;
        let required_fit = input.required_fit_ppm;
        if let Some(replacement) = best_replacement(bench, current_id, |candidate| {
            candidate.cost_micros_per_play.saturating_mul(2) <= starter_cost
                && candidate.personnel.capability_fit_ppm >= required_fit
        }) {
            return Some(SubstitutionDecision {
                class: SubstitutionClass::Economic,
                out: current_id.clone(),
                replacement: Some(replacement.clone()),
                reasons: vec![
                    "budget_half_consumed".into(),
                    "replacement_costs_at_most_half".into(),
                    "replacement_fit_meets_requirement".into(),
                ],
            });
        }
    }

    if current.personnel.capability_fit_ppm < input.required_fit_ppm {
        let required_fit = input.required_fit_ppm;
        if let Some(replacement) = best_replacement(bench, current_id, |candidate| {
            candidate.personnel.capability_fit_ppm >= required_fit
        }) {
            return Some(SubstitutionDecision {
                class: SubstitutionClass::Tactical,
                out: current_id.clone(),
                replacement: Some(replacement.clone()),
                reasons: vec![
                    "starter_fit_below_requirement".into(),
                    "replacement_fit_meets_requirement".into(),
                ],
            });
        }
    }

    // Default: keep playing.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{PersonnelKind, PPM_SCALE};

    fn personnel(id: &str, fit_ppm: u32) -> Personnel {
        Personnel {
            id: PersonnelId::new(id).unwrap(),
            kind: PersonnelKind::Model,
            display_name: id.into(),
            cost_tokens: 1_000,
            latency_ms: 100,
            capability_fit_ppm: fit_ppm,
            reliability_ppm: 900_000,
            quota_remaining_ppm: PPM_SCALE,
            evidence_quality_ppm: 800_000,
            own_ytg_milli: None,
        }
    }

    fn candidate(id: &str, fit_ppm: u32) -> BenchCandidate {
        BenchCandidate {
            personnel: personnel(id, fit_ppm),
            provider_health_ppm: 900_000,
            recent_gate_failures: 0,
            cost_micros_per_play: 500,
            available: true,
            busy: false,
        }
    }

    fn input(current: BenchCandidate, required_fit_ppm: u32) -> BenchInput {
        BenchInput {
            current,
            required_fit_ppm,
            budget_remaining: DriveBudget::zero(),
            budget_initial: DriveBudget::zero(),
            emergency: false,
            stall_cycles: 0,
        }
    }

    fn budget(monetary_cost_micros: u64) -> DriveBudget {
        DriveBudget {
            monetary_cost_micros,
            ..DriveBudget::zero()
        }
    }

    #[test]
    fn emergency_substitutes_immediately_and_ranks_fit_first() {
        let mut starter = candidate("starter", 900_000);
        starter.available = false;
        let pricey_fit = candidate("b-expensive-fit", 950_000);
        let cheap = BenchCandidate {
            cost_micros_per_play: 10,
            ..candidate("a-cheap-slower", 800_000)
        };
        let mut situation = input(starter.clone(), 800_000);
        situation.emergency = true;
        let decision = decide(&situation, &[cheap, pricey_fit]).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Emergency);
        assert_eq!(
            decision.replacement,
            Some(PersonnelId::new("b-expensive-fit").unwrap())
        );
        assert!(decision
            .reasons
            .contains(&"starter_unavailable".to_string()));
        // The emergency rung needs the declaration: a fallen starter without
        // one lands on plain availability, never emergency.
        assert_eq!(
            decide(&input(starter, 800_000), &[candidate("sub", 950_000)])
                .unwrap()
                .class,
            SubstitutionClass::Availability
        );
    }

    #[test]
    fn availability_replaces_an_unready_or_unhealthy_starter() {
        let mut benched = candidate("starter", 900_000);
        benched.available = false;
        let decision = decide(&input(benched, 0), &[candidate("sub", 100_000)]).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Availability);
        assert_eq!(decision.replacement, Some(PersonnelId::new("sub").unwrap()));

        let mut fallen = candidate("starter", 900_000);
        fallen.provider_health_ppm = MIN_PROVIDER_HEALTH_PPM - 1;
        let decision = decide(&input(fallen, 0), &[candidate("sub", 100_000)]).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Availability);
        // Exactly at the floor the starter is still playable.
        let mut playable = candidate("starter", 900_000);
        playable.provider_health_ppm = MIN_PROVIDER_HEALTH_PPM;
        assert_eq!(
            decide(&input(playable, 0), &[candidate("sub", 100_000)]),
            None
        );
    }

    #[test]
    fn performance_replaces_repeated_gate_failures() {
        let mut starter = candidate("starter", 700_000);
        starter.recent_gate_failures = PERFORMANCE_FAILURE_FLOOR;
        let mut peer = candidate("sub", 700_000);
        peer.recent_gate_failures = 2;
        let decision = decide(&input(starter.clone(), 0), &[peer]).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Performance);
        assert_eq!(decision.replacement, Some(PersonnelId::new("sub").unwrap()));
        // A peer with equal failures does not qualify, and nothing else
        // fires: the starter keeps playing.
        let mut twin = candidate("sub", 700_000);
        twin.recent_gate_failures = PERFORMANCE_FAILURE_FLOOR;
        assert_eq!(decide(&input(starter, 0), &[twin]), None);
    }

    #[test]
    fn economic_substitutes_at_half_budget() {
        let starter = BenchCandidate {
            cost_micros_per_play: 400,
            ..candidate("starter", 800_000)
        };
        let cheaper = BenchCandidate {
            cost_micros_per_play: 200,
            ..candidate("sub", 800_000)
        };
        let mut situation = input(starter.clone(), 800_000);
        situation.budget_remaining = budget(500);
        situation.budget_initial = budget(1_000);
        let qualified = [cheaper.clone()];
        let decision = decide(&situation, &qualified).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Economic);
        assert_eq!(decision.replacement, Some(PersonnelId::new("sub").unwrap()));

        // Not yet half spent: keep playing.
        let mut early = input(starter.clone(), 800_000);
        early.budget_remaining = budget(501);
        early.budget_initial = budget(1_000);
        assert_eq!(decide(&early, &qualified), None);
        // Half spent, but the cheaper player cannot cover the requirement.
        let mut unqualified = cheaper.clone();
        unqualified.personnel.capability_fit_ppm = 799_999;
        let mut late = input(starter, 800_000);
        late.budget_remaining = budget(500);
        late.budget_initial = budget(1_000);
        assert_eq!(decide(&late, &[unqualified]), None);
    }

    #[test]
    fn tactical_improves_fit_and_ignores_prestige() {
        let starter = BenchCandidate {
            cost_micros_per_play: 500,
            ..candidate("starter", 400_000)
        };
        // A deterministic procedure outranks no prestige tier: fit alone
        // decides, and the ladder never reads `PersonnelKind`.
        let tool = BenchCandidate {
            cost_micros_per_play: 500,
            personnel: Personnel {
                kind: PersonnelKind::DeterministicProcedure,
                ..personnel("sub", 600_000)
            },
            ..candidate("sub", 600_000)
        };
        let bench = [tool.clone()];
        let decision = decide(&input(starter, 600_000), &bench).unwrap();
        assert_eq!(decision.class, SubstitutionClass::Tactical);
        assert_eq!(decision.replacement, Some(PersonnelId::new("sub").unwrap()));
        // A requirement the starter already covers needs no substitution.
        assert_eq!(
            decide(&input(candidate("starter", 600_000), 400_000), &bench),
            None
        );
    }

    #[test]
    fn no_rung_firing_keeps_the_starter_playing() {
        let situation = input(candidate("starter", 900_000), 100_000);
        assert_eq!(decide(&situation, &[candidate("sub", 900_000)]), None);
        assert_eq!(decide(&situation, &[]), None);
    }

    #[test]
    fn decide_never_selects_an_unavailable_replacement() {
        let starter = candidate("starter", 400_000);
        let mut benched = candidate("b-higher-fit", 900_000);
        benched.available = false;
        let mut sidelined = candidate("a-lower-fit", 650_000);
        sidelined.available = false;
        // Neither rung may fire on an unavailable or benched player.
        assert_eq!(
            decide(&input(starter.clone(), 600_000), &[benched, sidelined]),
            None
        );
        // The available but weaker player is the only legal substitution.
        let mut ready = candidate("a-lower-fit", 650_000);
        ready.available = true;
        let decision = decide(&input(starter, 600_000), &[ready]).unwrap();
        assert_eq!(
            decision.replacement,
            Some(PersonnelId::new("a-lower-fit").unwrap())
        );
    }

    #[test]
    fn ties_break_by_personnel_id_ascending() {
        let starter = candidate("starter", 400_000);
        let left = candidate("twin-b", 600_000);
        let mut right = candidate("twin-a", 600_000);
        right.provider_health_ppm = left.provider_health_ppm;
        right.recent_gate_failures = left.recent_gate_failures;
        right.cost_micros_per_play = left.cost_micros_per_play;
        // Either bench order; the lower id wins both times.
        for bench in [
            vec![left.clone(), right.clone()],
            vec![right.clone(), left.clone()],
        ] {
            let decision = decide(&input(starter.clone(), 600_000), &bench).unwrap();
            assert_eq!(
                decision.replacement,
                Some(PersonnelId::new("twin-a").unwrap())
            );
        }
    }

    #[test]
    fn identical_inputs_decide_identically() {
        let starter = candidate("starter", 300_000);
        let mut situation = input(starter, 800_000);
        situation.emergency = true;
        situation.budget_remaining = budget(100);
        situation.budget_initial = budget(1_000);
        situation.current.provider_health_ppm = 0;
        let bench = vec![
            candidate("sub-1", 850_000),
            candidate("sub-2", 850_000),
            candidate("sub-3", 950_000),
        ];
        let first = decide(&situation, &bench).unwrap();
        for _ in 0..3 {
            assert_eq!(decide(&situation, &bench), Some(first.clone()));
        }
        // A permuted bench is the same input set and must decide the same.
        let mut permuted = bench.clone();
        permuted.reverse();
        assert_eq!(decide(&situation, &permuted), Some(first.clone()));
        assert_eq!(first.class, SubstitutionClass::Emergency);
    }

    #[test]
    fn a_busy_starter_is_not_substituted_until_reconciliation() {
        // Default policy: an unresolved invocation means the player is busy.
        // The ladder keeps the starter — reconciliation answers first.
        let mut starter = candidate("starter", 400_000);
        starter.busy = true;
        assert!(decide(&input(starter, 600_000), &[candidate("bench-a", 900_000)]).is_none());
        // Reconciliation answered: the invocation resolved as a failed
        // attempt. The starter is no longer busy and the ladder may fire.
        let mut resolved = candidate("starter", 400_000);
        resolved.busy = false;
        let decision = decide(&input(resolved, 600_000), &[candidate("bench-a", 900_000)])
            .expect("a resolved failure substitutes");
        assert_eq!(
            decision.replacement,
            Some(PersonnelId::new("bench-a").unwrap())
        );
    }

    #[test]
    fn busy_players_are_never_selected_as_replacements() {
        // The perfect-fit replacement is mid-invocation: ineligible until
        // its own effect reconciles, even though it is available.
        let mut busy = candidate("perfect-fit", 900_000);
        busy.busy = true;
        assert_eq!(
            decide(&input(candidate("starter", 400_000), 600_000), &[busy]),
            None
        );
    }
}
