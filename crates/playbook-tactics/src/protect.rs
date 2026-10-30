//! The concentration doctrines: Heavy and Max Protect.
//!
//! Football: when the margin is thin or the state is precious, a coach
//! stops spreading the field and packages protection instead. Heavy (jumbo
//! personnel) brings extra blockers and drives slowly behind one back —
//! migrations, security-sensitive patches, release closure, and short-YTG
//! completion call for it. Max Protect keeps a back and a tight end in to
//! block, rolls the quarterback away from pressure, and stations a safety
//! deep — production changes, destructive operations, and state migrations
//! call for it. Playbook translates both the same way: generative breadth
//! collapses to one primary executor, and every other roster seat is spent
//! on deterministic verification, specialist support, held-back reserves,
//! and observation.
//!
//! The deterministic splits (integer arithmetic only, saturating at what
//! the roster provides):
//!
//! * Heavy fields one executor and splits the remainder three-two-one-one
//!   across validators, tool specialists, test workers, and reserves, with
//!   largest-remainder rounding and leftovers assigned in that order.
//!   Consequences: validators always outnumber tool specialists, and a
//!   tool specialist is staffed whenever at least two seats exist beyond
//!   the executor. Heavy fields no observers.
//! * Max Protect fields one executor, one observer from four available,
//!   one reserve from five, and no tool specialists at all (depth, not
//!   breadth). What remains splits three-to-one between validators
//!   (rounded up) and test workers, so verification dominates: from eight
//!   available upward Max Protect staffs strictly more validators than
//!   Heavy at the same availability. Below that the two converge as the
//!   protection earmarks (observer, reserve) absorb the difference.
//!
//! Scoring is a plain ratio on the contract's 0..=1_000 ppm scale:
//! [`protection_score`] weighs validators, observers, and reserves over
//! the head-count; [`generative_breadth`] weighs executors over the
//! head-count. The two move opposite ways — that is the whole doctrine.
//!
//! Deployment topology: the executors Sole-own the primary domain (the
//! context's named critical domain when there is one), all support shares
//! a `verification` domain, observers share an `observation` domain, and
//! reserves are assigned to no domain at all — they exist so a failure
//! never forces an improvised re-shape.
//!
//! Counters: Heavy names `spread` — when concentration overcommits and the
//! problem turns out to be broad, spreading the field is the correction.
//! Max Protect names no counter in this sprint: it runs at Critical risk
//! with reassignment forbidden (`may_reassign: false`), so a failed
//! protection has no cheaper reorganisation to propose — termination and
//! budget exhaustion own what follows, and declaring a counter would
//! promise a mid-critical pivot the authority policy forbids.
//!
//! The short-YTG policy: when completion is near, concentration outranks
//! breadth — but never concentrate into an empty wallet. The policy holds
//! only while at least [`WALLET_FLOOR_PPM`] of the budget remains.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError, TacticalId};

use crate::domain::{
    tactics_refusal, AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment,
    ExpectedEffect, HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, RiskBand,
    TacticalBudget, TacticalClass, TacticalContext, TacticalDirective, TacticalTrigger,
    TerminationCondition,
};

/// The short-YTG wallet floor: concentration is called only while at least
/// this much of the budget remains (ten percent of the cap, in ppm).
/// Validators, reserves, and observation are paid for like any other
/// capacity; a protection package assembled into an empty wallet collapses
/// at the first failure.
pub const WALLET_FLOOR_PPM: u32 = 100_000;

/// The primary domain a concentration shape owns when the context names no
/// critical domain.
const PRIMARY_DOMAIN_FALLBACK: &str = "primary";

/// How the concentration doctrines distribute a roster. Every field counts
/// personnel; the profile is a pure function of availability and never
/// exceeds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcentrationProfile {
    /// Generative executors. The concentration doctrines field exactly one.
    pub executors: u16,
    /// Deterministic verification personnel: they check the executor's
    /// work against the contract.
    pub validators: u16,
    /// Specialist tool operators (schema drivers, migration runners).
    pub tool_specialists: u16,
    /// Personnel who exercise the result: they run the tests so the
    /// executor does not grade its own work.
    pub test_workers: u16,
    /// Held-back capacity: a failure of the executor is absorbed by a
    /// reserve, never by an improvised re-shape.
    pub reserves: u16,
    /// Watch-only personnel. They never execute ordinary work.
    pub observers: u16,
}

/// Total personnel the profile fields.
fn headcount(profile: &ConcentrationProfile) -> u32 {
    u32::from(profile.executors)
        .saturating_add(u32::from(profile.validators))
        .saturating_add(u32::from(profile.tool_specialists))
        .saturating_add(u32::from(profile.test_workers))
        .saturating_add(u32::from(profile.reserves))
        .saturating_add(u32::from(profile.observers))
}

/// The Heavy concentration profile: one executor, and the remainder split
/// three-two-one-one across validators, tool specialists, test workers,
/// and reserves. Leftover units from integer division go to validators
/// first, then tool specialists, then test workers, then reserves. With
/// one available this is the executor alone; with none it is the empty
/// profile.
pub fn heavy_profile(available: u16) -> ConcentrationProfile {
    let available = u32::from(available);
    let executors = available.min(1);
    let remainder = available.saturating_sub(executors);
    let (validators, tool_specialists, test_workers, reserves) = split_heavy_remainder(remainder);
    ConcentrationProfile {
        executors: executors as u16,
        validators,
        tool_specialists,
        test_workers,
        reserves,
        observers: 0,
    }
}

/// Split the Heavy remainder three-two-one-one (validators, tool
/// specialists, test workers, reserves) with largest-remainder rounding;
/// leftover units go in that priority order. The parts always sum to
/// `remainder`, which is at most `u16::MAX`, so every part fits a `u16`.
fn split_heavy_remainder(remainder: u32) -> (u16, u16, u16, u16) {
    let mut validators = remainder.saturating_mul(3) / 7;
    let mut tool_specialists = remainder.saturating_mul(2) / 7;
    let mut test_workers = remainder / 7;
    let mut reserves = remainder / 7;
    let mut leftover = remainder.saturating_sub(
        validators
            .saturating_add(tool_specialists)
            .saturating_add(test_workers)
            .saturating_add(reserves),
    );
    // At most three units are lost to flooring, so this chain is exact.
    if leftover > 0 {
        validators += 1;
        leftover -= 1;
    }
    if leftover > 0 {
        tool_specialists += 1;
        leftover -= 1;
    }
    if leftover > 0 {
        test_workers += 1;
        leftover -= 1;
    }
    if leftover > 0 {
        reserves += 1;
    }
    (
        validators as u16,
        tool_specialists as u16,
        test_workers as u16,
        reserves as u16,
    )
}

/// The Max Protect concentration profile: one executor, one observer from
/// four available, one reserve from five, no tool specialists, and the
/// rest of the roster split three-to-one between validators (rounded up)
/// and test workers. Verification dominates; the backup and the deep
/// safety are staffed only once the verification core stands.
pub fn max_protect_profile(available: u16) -> ConcentrationProfile {
    let available = u32::from(available);
    let executors = available.min(1);
    let observers = if available >= 4 { 1 } else { 0 };
    let reserves = if available >= 5 { 1 } else { 0 };
    let remainder =
        available.saturating_sub(executors.saturating_add(observers).saturating_add(reserves));
    let validators = remainder.saturating_sub(remainder / 4);
    let test_workers = remainder / 4;
    ConcentrationProfile {
        executors: executors as u16,
        validators: validators as u16,
        tool_specialists: 0,
        test_workers: test_workers as u16,
        reserves: reserves as u16,
        observers: observers as u16,
    }
}

/// Assurance weight, 0..=1_000 ppm-scale: validators, observers, and
/// reserves over the head-count. An empty profile protects nothing and
/// scores zero.
pub fn protection_score(profile: &ConcentrationProfile) -> u32 {
    let total = headcount(profile);
    if total == 0 {
        return 0;
    }
    let protectors = u32::from(profile.validators)
        .saturating_add(u32::from(profile.observers))
        .saturating_add(u32::from(profile.reserves));
    protectors.saturating_mul(1_000) / total
}

/// Generative breadth, 0..=1_000 ppm-scale: executors over the head-count.
/// The inverse face of [`protection_score`]: one executor among many
/// protectors scores low, an all-executor profile scores the ceiling.
pub fn generative_breadth(profile: &ConcentrationProfile) -> u32 {
    let total = headcount(profile);
    if total == 0 {
        return 0;
    }
    u32::from(profile.executors).saturating_mul(1_000) / total
}

/// Staff up to `wanted` positions from `available` starting at `*cursor`,
/// advancing the cursor. The returned ids are unique across calls because
/// the cursor never revisits a seat.
fn staff(wanted: u16, available: &[PersonnelId], cursor: &mut usize) -> Vec<PersonnelId> {
    let room = usize::from(wanted).min(available.len().saturating_sub(*cursor));
    let staffed = available[*cursor..*cursor + room].to_vec();
    *cursor += room;
    staffed
}

/// Deploy the profile against the roster. The executors Sole-own
/// `primary_domain` (Shared if the caller supplied several), validators,
/// test workers, and tool specialists share the `verification` domain, and
/// observers share the `observation` domain; reserves appear in
/// `PersonnelShape::reserves` and in no assignment. Staffing saturates in
/// doctrine priority — executors, validators, test workers, tool
/// specialists, observers, reserves — so the shape never deploys more than
/// `available.len()`, and the returned shape always passes
/// `PersonnelShape::validate`. An empty roster, an unnamed primary domain,
/// or a profile that would deploy nobody is refused.
pub fn shape(
    profile: &ConcentrationProfile,
    primary_domain: &str,
    available: &[PersonnelId],
) -> Result<PersonnelShape, PlaybookError> {
    if available.is_empty() {
        return Err(tactics_refusal(
            "no personnel available: concentration deploys nobody",
        ));
    }
    if primary_domain.trim().is_empty() {
        return Err(tactics_refusal("concentration names no primary domain"));
    }
    let mut cursor = 0usize;
    let executors = staff(profile.executors, available, &mut cursor);
    let validators = staff(profile.validators, available, &mut cursor);
    let test_workers = staff(profile.test_workers, available, &mut cursor);
    let tool_specialists = staff(profile.tool_specialists, available, &mut cursor);
    let observers = staff(profile.observers, available, &mut cursor);
    let reserves = staff(profile.reserves, available, &mut cursor);
    if executors.is_empty() && observers.is_empty() {
        return Err(tactics_refusal(
            "profile deploys nobody: no executors and no observers",
        ));
    }

    let executor_count = executors.len();
    let support_count = validators.len() + test_workers.len() + tool_specialists.len();
    let observer_count = observers.len();
    let reserve_count = reserves.len();

    let mut assignments = Vec::new();
    if !executors.is_empty() {
        assignments.push(DomainAssignment {
            domain: primary_domain.to_string(),
            personnel: executors,
            mode: if executor_count == 1 {
                OwnershipMode::Sole
            } else {
                OwnershipMode::Shared
            },
        });
    }
    let mut verification = validators;
    verification.extend(test_workers);
    verification.extend(tool_specialists);
    if !verification.is_empty() {
        assignments.push(DomainAssignment {
            domain: "verification".to_string(),
            personnel: verification,
            mode: OwnershipMode::Shared,
        });
    }
    if !observers.is_empty() {
        assignments.push(DomainAssignment {
            domain: "observation".to_string(),
            personnel: observers,
            mode: OwnershipMode::Shared,
        });
    }

    let deployed = PersonnelShape {
        primary_executors: executor_count as u16,
        support: support_count as u16,
        reserves: reserve_count as u16,
        observers: observer_count as u16,
        assignments,
    };
    deployed.validate()?;
    Ok(deployed)
}

/// Which concentration doctrine to call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtectionKind {
    /// Heavy: one executor wrapped in deterministic support — migrations,
    /// security-sensitive patches, release closure, short-YTG completion.
    Heavy,
    /// Max Protect: one executor, a backup, and verification dominance —
    /// production changes, destructive operations, state migrations.
    MaxProtect,
}

/// The Heavy trigger: elevated risk while the drive is still long, or
/// short-YTG completion outright — heavy serves release closure and the
/// final push alike.
fn heavy_trigger() -> TacticalTrigger {
    TacticalTrigger::Any(vec![
        TacticalTrigger::All(vec![
            TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Elevated,
            },
            TacticalTrigger::Not(Box::new(TacticalTrigger::ShortYtg)),
        ]),
        TacticalTrigger::ShortYtg,
    ])
}

/// Build the concentration directive for `kind` against the roster.
/// The shape is drawn from [`heavy_profile`] or [`max_protect_profile`];
/// the primary domain is the context's named critical domain when there is
/// one and the generic primary otherwise. The trigger is measured state,
/// not judgement: Heavy fires at elevated risk with a long field or on
/// short-YTG; Max Protect fires at Critical risk and forbids reassignment
/// so the current assignment is what gets protected. The result passes
/// [`crate::admission::validated`] before it is returned; an empty roster
/// is refused.
pub fn directive(
    kind: ProtectionKind,
    context: &TacticalContext,
    available: &[PersonnelId],
) -> Result<TacticalDirective, PlaybookError> {
    if available.is_empty() {
        return Err(tactics_refusal(
            "no personnel available: protection deploys nobody",
        ));
    }
    let seats = u16::try_from(available.len())
        .map_err(|_| tactics_refusal("roster exceeds the deployable width"))?;
    let profile = match kind {
        ProtectionKind::Heavy => heavy_profile(seats),
        ProtectionKind::MaxProtect => max_protect_profile(seats),
    };
    let primary_domain = context
        .critical_domain
        .clone()
        .unwrap_or_else(|| PRIMARY_DOMAIN_FALLBACK.to_string());
    let personnel_shape = shape(&profile, &primary_domain, available)?;

    let (name, class, trigger) = match kind {
        ProtectionKind::Heavy => ("heavy", TacticalClass::Formation, heavy_trigger()),
        ProtectionKind::MaxProtect => (
            "max_protect",
            TacticalClass::Doctrine,
            TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Critical,
            },
        ),
    };
    let (benefit_ppm, cost_ppm, risk_ppm): (u32, u32, u32) = match kind {
        ProtectionKind::Heavy => (800, 200, 100),
        ProtectionKind::MaxProtect => (850, 150, 100),
    };
    let expected_effect = match kind {
        ProtectionKind::Heavy => ExpectedEffect {
            breadth_delta_ppm: 100,
            protection_delta_ppm: 800,
            tempo_delta_ppm: 0,
            information_delta_ppm: 0,
        },
        ProtectionKind::MaxProtect => ExpectedEffect {
            breadth_delta_ppm: 50,
            protection_delta_ppm: 900,
            tempo_delta_ppm: 0,
            information_delta_ppm: 0,
        },
    };
    let authority = match kind {
        ProtectionKind::Heavy => AuthorityPolicy {
            may_reassign: true,
            may_reallocate_budget: false,
            may_change_information_topology: true,
            observe_only: false,
        },
        ProtectionKind::MaxProtect => AuthorityPolicy {
            may_reassign: false,
            may_reallocate_budget: false,
            may_change_information_topology: true,
            observe_only: false,
        },
    };
    // Heavy's counter is Spread: concentration that overcommits a broad
    // problem is corrected by spreading the field. Max Protect declares no
    // counter (see the module doctrine).
    let counter_tactic = match kind {
        ProtectionKind::Heavy => Some("spread".to_string()),
        ProtectionKind::MaxProtect => None,
    };

    let directive = TacticalDirective {
        id: TacticalId::new(format!("t-{name}"))?,
        name: name.to_string(),
        class,
        trigger,
        benefit_ppm,
        cost_ppm,
        risk_ppm,
        shape: personnel_shape,
        information: InformationPolicy {
            // Concentration works from one shared picture: high overlap,
            // full huddle, and the replacement inherits the residue.
            max_context_overlap_ppm: 600,
            huddle_access: HuddleAccess::Full,
            disclose_failures_to_replacement: true,
        },
        concurrency: ConcurrencyPolicy {
            // One generative lane: that is the doctrine.
            max_concurrent: profile.executors.max(1),
            allow_domain_overlap: false,
        },
        authority,
        budget: TacticalBudget {
            max_tokens: 200_000,
            max_cost_microunits: 20_000_000,
            max_duration_ms: 600_000,
            max_plays: 2,
        },
        expected_effect,
        termination: TerminationCondition {
            max_plays: 2,
            abort_when: vec![
                AbortTrigger::BudgetExhausted,
                AbortTrigger::DivergenceExceedsTolerance,
                AbortTrigger::WinConditionSatisfied,
            ],
        },
        counter_tactic,
    };
    crate::admission::validated(&directive)?;
    Ok(directive)
}

/// The short-YTG completion policy: when completion is near, concentration
/// outranks breadth — but never concentrate into an empty wallet. Holds
/// exactly when the context is short-YTG and at least
/// [`WALLET_FLOOR_PPM`] of the budget remains.
pub fn short_ytg_policy(context: &TacticalContext) -> bool {
    context.short_ytg && context.budget_remaining_ppm >= WALLET_FLOOR_PPM
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::Admission;
    use crate::composition::counter_of;

    fn context() -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 500,
            domain_coupling_ppm: 400,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures: 0,
            risk_band: RiskBand::Elevated,
            divergence_observed: false,
            critical_domain: Some("migration.core".into()),
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 7,
        }
    }

    fn roster(count: usize) -> Vec<PersonnelId> {
        (0..count)
            .map(|index| PersonnelId::new(format!("p{index:02}")).unwrap())
            .collect()
    }

    #[test]
    fn heavy_and_max_protect_split_the_roster_by_the_documented_ratios() {
        // The documented eight-seat examples.
        assert_eq!(
            heavy_profile(8),
            ConcentrationProfile {
                executors: 1,
                validators: 3,
                tool_specialists: 2,
                test_workers: 1,
                reserves: 1,
                observers: 0,
            }
        );
        assert_eq!(
            max_protect_profile(8),
            ConcentrationProfile {
                executors: 1,
                validators: 4,
                tool_specialists: 0,
                test_workers: 1,
                reserves: 1,
                observers: 1,
            }
        );
        for available in 0..=40u16 {
            let heavy = heavy_profile(available);
            let protect = max_protect_profile(available);
            assert_eq!(u32::from(available), headcount(&heavy));
            assert_eq!(u32::from(available), headcount(&protect));
            assert!(
                heavy.validators >= heavy.tool_specialists,
                "heavy validators dominate tool specialists at {available}"
            );
            if available >= 3 {
                assert!(
                    heavy.tool_specialists >= 1,
                    "a tool specialist is staffed whenever the roster allows, at {available}"
                );
            }
            if available >= 1 {
                assert_eq!(heavy.executors, 1);
                assert_eq!(protect.executors, 1);
            }
        }
    }

    #[test]
    fn max_protect_verifies_heavier_and_observes_from_four() {
        // From eight available upward, Max Protect staffs strictly more
        // validators than Heavy at equal availability.
        for available in 8..=24u16 {
            assert!(
                max_protect_profile(available).validators > heavy_profile(available).validators,
                "max protect verifies heavier than heavy at {available}"
            );
        }
        // The deep safety: one observer from four available, none below.
        for available in 4..=16u16 {
            assert!(
                max_protect_profile(available).observers >= 1,
                "max protect observes at {available}"
            );
        }
        for available in 0..=3u16 {
            assert_eq!(max_protect_profile(available).observers, 0);
        }
        // Heavy fields no observers at any size.
        for available in 0..=16u16 {
            assert_eq!(heavy_profile(available).observers, 0);
        }
    }

    #[test]
    fn protection_score_orders_concentration_and_generative_breadth_inverts() {
        let heavy = heavy_profile(8);
        let protect = max_protect_profile(8);
        let wide = ConcentrationProfile {
            executors: 4,
            validators: 0,
            tool_specialists: 0,
            test_workers: 0,
            reserves: 0,
            observers: 0,
        };
        let heavy_score = protection_score(&heavy);
        let protect_score = protection_score(&protect);
        let wide_score = protection_score(&wide);
        assert_eq!(heavy_score, 500);
        assert_eq!(protect_score, 750);
        assert_eq!(wide_score, 0);
        assert!(protect_score >= heavy_score);
        assert!(heavy_score > wide_score);
        // Breadth is the inverse face: the all-executor profile saturates
        // it, the concentrated profiles sit near the floor.
        let heavy_breadth = generative_breadth(&heavy);
        let protect_breadth = generative_breadth(&protect);
        let wide_breadth = generative_breadth(&wide);
        assert_eq!(wide_breadth, 1_000);
        assert_eq!(heavy_breadth, 125);
        assert_eq!(protect_breadth, 125);
        assert!(wide_breadth > heavy_breadth);
        assert!(heavy_breadth >= protect_breadth);
        // The two scores can never overcommit the scale together.
        for available in 0..=40u16 {
            let h = heavy_profile(available);
            let p = max_protect_profile(available);
            assert!(protection_score(&h) + generative_breadth(&h) <= 1_000);
            assert!(protection_score(&p) + generative_breadth(&p) <= 1_000);
        }
    }

    #[test]
    fn shapes_validate_and_never_exceed_the_roster() {
        for available in 1..=16usize {
            let people = roster(available);
            for kind in [ProtectionKind::Heavy, ProtectionKind::MaxProtect] {
                let profile = match kind {
                    ProtectionKind::Heavy => heavy_profile(available as u16),
                    ProtectionKind::MaxProtect => max_protect_profile(available as u16),
                };
                let deployed = shape(&profile, "migration.core", &people).unwrap();
                deployed.validate().unwrap();
                assert!(deployed.deployed() <= available as u16);
                assert!(deployed.primary_executors >= 1);
                // No person is double-booked, and reserves — deployed to
                // no domain — are exactly the unassigned seats.
                let mut owners: Vec<&PersonnelId> = Vec::new();
                for assignment in &deployed.assignments {
                    assert!(!assignment.domain.trim().is_empty());
                    for person in &assignment.personnel {
                        assert!(!owners.contains(&person), "double-booked at {available}");
                        owners.push(person);
                    }
                }
                assert_eq!(
                    owners.len(),
                    (deployed.deployed() - deployed.reserves) as usize
                );
                if kind == ProtectionKind::MaxProtect && available >= 4 {
                    let observation = deployed
                        .assignments
                        .iter()
                        .find(|assignment| assignment.domain == "observation")
                        .unwrap();
                    assert_eq!(observation.personnel.len(), 1);
                }
            }
        }
        // The documented eight-seat heavy topology, seat by seat.
        let people = roster(8);
        let heavy_shape = shape(&heavy_profile(8), "migration.core", &people).unwrap();
        assert_eq!(heavy_shape.primary_executors, 1);
        assert_eq!(heavy_shape.support, 6);
        assert_eq!(heavy_shape.reserves, 1);
        assert_eq!(heavy_shape.observers, 0);
        assert_eq!(heavy_shape.deployed(), 8);
        assert_eq!(heavy_shape.assignments[0].domain, "migration.core");
        assert_eq!(heavy_shape.assignments[0].mode, OwnershipMode::Sole);
        assert_eq!(
            heavy_shape.assignments[0].personnel,
            vec![people[0].clone()]
        );
        assert_eq!(heavy_shape.assignments[1].domain, "verification");
        assert_eq!(heavy_shape.assignments[1].mode, OwnershipMode::Shared);
        assert_eq!(heavy_shape.assignments[1].personnel.len(), 6);
    }

    #[test]
    fn empty_rosters_and_unnamed_domains_are_refused() {
        let people = roster(8);
        assert!(shape(&heavy_profile(8), "migration.core", &[]).is_err());
        assert!(shape(&max_protect_profile(8), "   ", &people).is_err());
        // A profile that deploys nobody cannot ride in on a full roster.
        let empty = ConcentrationProfile {
            executors: 0,
            validators: 0,
            tool_specialists: 0,
            test_workers: 0,
            reserves: 0,
            observers: 0,
        };
        assert!(shape(&empty, "migration.core", &people).is_err());
        assert!(directive(ProtectionKind::Heavy, &context(), &[]).is_err());
        assert!(directive(ProtectionKind::MaxProtect, &context(), &[]).is_err());
    }

    #[test]
    fn directives_validate_admit_and_trigger_on_band_and_ytg() {
        let people = roster(8);
        let context = context();
        let heavy = directive(ProtectionKind::Heavy, &context, &people).unwrap();
        crate::admission::validated(&heavy).unwrap();
        heavy.shape.validate().unwrap();
        assert_eq!(heavy.name, "heavy");
        assert_eq!(heavy.class, TacticalClass::Formation);
        assert_eq!(counter_of(&heavy), Some("spread"));
        assert!(heavy.expected_effect.protection_delta_ppm >= 700);
        assert!(heavy.expected_effect.breadth_delta_ppm <= 200);
        assert!(
            heavy.trigger.evaluate(&context),
            "heavy fires at elevated risk"
        );
        let calm = TacticalContext {
            risk_band: RiskBand::Low,
            ..context.clone()
        };
        assert!(
            !heavy.trigger.evaluate(&calm),
            "heavy does not fire at low risk on a long field"
        );
        let closing = TacticalContext {
            risk_band: RiskBand::Low,
            short_ytg: true,
            ..context.clone()
        };
        assert!(
            heavy.trigger.evaluate(&closing),
            "short ytg alone calls heavy"
        );
        assert_eq!(heavy.evaluate_admission(&context, 100), Admission::Admitted);

        let protect = directive(ProtectionKind::MaxProtect, &context, &people).unwrap();
        crate::admission::validated(&protect).unwrap();
        protect.shape.validate().unwrap();
        assert_eq!(protect.name, "max_protect");
        assert_eq!(protect.class, TacticalClass::Doctrine);
        assert!(
            !protect.authority.may_reassign,
            "max protect guards the current assignment"
        );
        assert_eq!(counter_of(&protect), None);
        assert!(
            !protect.trigger.evaluate(&context),
            "elevated risk alone does not call max protect"
        );
        let critical = TacticalContext {
            risk_band: RiskBand::Critical,
            ..context.clone()
        };
        assert!(protect.trigger.evaluate(&critical));
        assert_eq!(
            protect.evaluate_admission(&critical, 100),
            Admission::Admitted
        );
        assert!(
            protect.shape.observers >= 1,
            "max protect observes the field at eight available"
        );
    }

    #[test]
    fn short_ytg_policy_demands_a_wallet() {
        let mut context = context();
        context.short_ytg = TacticalContext::is_short_ytg(2_500, 10_000);
        context.budget_remaining_ppm = 150_000;
        assert!(short_ytg_policy(&context));
        context.budget_remaining_ppm = 50_000;
        assert!(!short_ytg_policy(&context));
        context.budget_remaining_ppm = WALLET_FLOOR_PPM;
        assert!(short_ytg_policy(&context), "the wallet floor is inclusive");
        context.short_ytg = false;
        context.budget_remaining_ppm = 900_000;
        assert!(!short_ytg_policy(&context));
    }

    #[test]
    fn concentration_is_deterministic() {
        let people = roster(8);
        let context = context();
        assert_eq!(heavy_profile(8), heavy_profile(8));
        assert_eq!(max_protect_profile(12), max_protect_profile(12));
        assert_eq!(
            protection_score(&heavy_profile(8)),
            protection_score(&heavy_profile(8))
        );
        assert_eq!(
            generative_breadth(&max_protect_profile(12)),
            generative_breadth(&max_protect_profile(12))
        );
        assert_eq!(
            shape(&heavy_profile(8), "migration.core", &people).unwrap(),
            shape(&heavy_profile(8), "migration.core", &people).unwrap()
        );
        assert_eq!(
            directive(ProtectionKind::MaxProtect, &context, &people).unwrap(),
            directive(ProtectionKind::MaxProtect, &context, &people).unwrap()
        );
        let heavy = directive(ProtectionKind::Heavy, &context, &people).unwrap();
        assert_eq!(
            heavy.trigger.evaluate(&context),
            heavy.trigger.evaluate(&context)
        );
    }
}
