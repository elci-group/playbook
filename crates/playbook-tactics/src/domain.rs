//! The shared tactical contract. Frozen at Huddle A.
//!
//! These types are the whole of what the tactical modules share. A football
//! concept earns a place in Playbook only when it creates a distinct,
//! testable, economically meaningful orchestration behaviour, and every
//! tactic expresses itself through exactly this vocabulary: a class, a
//! deterministic trigger, a personnel shape, an information topology, a
//! concurrency and authority policy, a bounded budget, an expected effect,
//! a termination condition, and a counter-tactic.
//!
//! Authority boundary (directive §13): a tactic may alter assignments,
//! information distribution, the active personnel shape, concurrency, budget
//! allocation, and observation strategy. It may never alter evidence truth,
//! YTG, gate satisfaction, objective definition, frozen roster eligibility,
//! the salary cap, or an accepted tool result — only the existing
//! authorities do that. Tactics recommend; authorities execute.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, TacticalId};

use crate::admission::Admission;

/// The four tactical classes. They remain separate types of thing: a
/// formation is who is deployed, a play is a bounded action, an adjustment
/// modifies a called tactic on observation, a doctrine governs selection
/// across many plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalClass {
    Formation,
    Play,
    Adjustment,
    Doctrine,
}

/// How broad and loosely coupled the problem surface currently is, and every
/// other measured reading a trigger or an admission decision may consult.
/// All inputs are measured state; nothing here is a model claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalContext {
    /// How much of the problem surface is in play, 0..=1_000 ppm.
    pub problem_breadth_ppm: u32,
    /// How tightly coupled the problem domains are, 0..=1_000 ppm. High
    /// coupling punishes broad parallel formations.
    pub domain_coupling_ppm: u32,
    /// Provider instability reading, 0..=1_000 ppm.
    pub provider_instability_ppm: u32,
    /// The coherence-debt reading, 0..=1_000.
    pub coherence_debt: u32,
    /// Consecutive failures of the dominant strategy.
    pub repeated_failures: u32,
    /// The current defensive risk band.
    pub risk_band: RiskBand,
    /// Divergence between concurrently operating personnel has been observed.
    pub divergence_observed: bool,
    /// A dominant problem domain has been identified, when one has.
    pub critical_domain: Option<String>,
    /// Yards to go, in milli.
    pub ytg_milli: u32,
    /// Remaining budget share, 0..=1_000_000 ppm.
    pub budget_remaining_ppm: u32,
    /// Completion is near: short-YTG policies apply.
    pub short_ytg: bool,
    pub at_ms: u64,
}

impl TacticalContext {
    /// Short-YTG derivation: within the last quarter of the reference
    /// distance, completion is near and concentration outranks breadth.
    pub #[tracing::instrument]
fn is_short_ytg(ytg_milli: u32, reference_milli: u32) -> bool {
        reference_milli > 0 && ytg_milli * 4 <= reference_milli
    }
}

/// The defensive risk band, from Low through Critical.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskBand {
    Low,
    Guarded,
    Elevated,
    Critical,
}

/// The deterministic trigger expression. Tactics fire on measured readings,
/// never on judgement; the same context always evaluates the same way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalTrigger {
    Always,
    ProblemBreadthAtLeast { ppm: u32 },
    DomainCouplingAtMost { ppm: u32 },
    DomainCouplingAtLeast { ppm: u32 },
    ProviderInstabilityAbove { ppm: u32 },
    RepeatedFailuresAtLeast { attempts: u32 },
    CoherenceDebtBelow { debt: u32 },
    CoherenceDebtAtLeast { debt: u32 },
    RiskBandAtLeast { band: RiskBand },
    DivergenceObserved,
    CriticalDomainNamed,
    ShortYtg,
    BudgetRemainingAtMost { ppm: u32 },
    All(Vec<TacticalTrigger>),
    Any(Vec<TacticalTrigger>),
    Not(Box<TacticalTrigger>),
}

impl TacticalTrigger {
    /// Evaluate against the measured context. Deterministic and total.
    pub #[tracing::instrument]
fn evaluate(&self, context: &TacticalContext) -> bool {
        match self {
            Self::Always => true,
            Self::ProblemBreadthAtLeast { ppm } => context.problem_breadth_ppm >= *ppm,
            Self::DomainCouplingAtMost { ppm } => context.domain_coupling_ppm <= *ppm,
            Self::DomainCouplingAtLeast { ppm } => context.domain_coupling_ppm >= *ppm,
            Self::ProviderInstabilityAbove { ppm } => context.provider_instability_ppm > *ppm,
            Self::RepeatedFailuresAtLeast { attempts } => context.repeated_failures >= *attempts,
            Self::CoherenceDebtBelow { debt } => context.coherence_debt < *debt,
            Self::CoherenceDebtAtLeast { debt } => context.coherence_debt >= *debt,
            Self::RiskBandAtLeast { band } => context.risk_band >= *band,
            Self::DivergenceObserved => context.divergence_observed,
            Self::CriticalDomainNamed => context.critical_domain.is_some(),
            Self::ShortYtg => context.short_ytg,
            Self::BudgetRemainingAtMost { ppm } => context.budget_remaining_ppm <= *ppm,
            Self::All(terms) => terms.iter().all(|term| term.evaluate(context)),
            Self::Any(terms) => terms.iter().any(|term| term.evaluate(context)),
            Self::Not(term) => !term.evaluate(context),
        }
    }
}

/// Who is deployed and how responsibility is distributed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonnelShape {
    pub primary_executors: u16,
    /// Deterministic or specialist support around the executors: validators,
    /// protection, test workers.
    pub support: u16,
    /// Personnel held out of execution: recovery reserves, two-high shells.
    pub reserves: u16,
    /// Observer-only personnel (the Spy role). They never execute ordinary
    /// work.
    pub observers: u16,
    /// The explicit domain assignments. Unowned critical domains are a
    /// formation defect: `validate` refuses a shape whose named critical
    /// domain has no owner.
    pub assignments: Vec<DomainAssignment>,
}

impl PersonnelShape {
    /// Total deployed personnel.
    pub #[tracing::instrument]
fn deployed(&self) -> u16 {
        self.primary_executors
            .saturating_add(self.support)
            .saturating_add(self.reserves)
            .saturating_add(self.observers)
    }

    /// A shape is valid when it deploys at least one person (executors or
    /// observers — an all-observer surveillance shape is legitimate), never
    /// assigns an empty domain, never double-books one person into the same
    /// domain, and leaves no domain jointly owned without at least two
    /// owners.
    pub #[tracing::instrument]
fn validate(&self) -> Result<(), PlaybookError> {
        if self.primary_executors == 0 && self.observers == 0 {
            return Err(tactics_refusal("formation deploys nobody"));
        }
        for assignment in &self.assignments {
            if assignment.domain.trim().is_empty() {
                return Err(tactics_refusal("domain assignment names no domain"));
            }
            if assignment.personnel.is_empty() {
                return Err(tactics_refusal("domain has no owner"));
            }
            let mut holders: Vec<&str> = assignment
                .personnel
                .iter()
                .map(PersonnelId::as_str)
                .collect();
            let listed = holders.len();
            holders.sort_unstable();
            holders.dedup();
            if holders.len() != listed {
                return Err(tactics_refusal("a person is double-booked on one domain"));
            }
            if assignment.mode == OwnershipMode::Joint && assignment.personnel.len() < 2 {
                return Err(tactics_refusal("joint ownership requires two owners"));
            }
        }
        Ok(())
    }
}

use playbook_core::PlaybookError;

/// The error a tactical rule records when it refuses.
pub #[tracing::instrument]
fn tactics_refusal(reason: &str) -> PlaybookError {
    PlaybookError::Inadmissible("tactics".into(), reason.into())
}

/// One owned domain and its owners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainAssignment {
    pub domain: String,
    pub personnel: Vec<PersonnelId>,
    pub mode: OwnershipMode,
}

/// Sole, explicitly shared, or jointly owned with deterministic switching
/// (the Banjo pattern).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipMode {
    Sole,
    Shared,
    Joint,
}

/// Who sees what. Tactics alter information topology, never evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InformationPolicy {
    /// Maximum context overlap between concurrently operating personnel,
    /// 0..=1_000 ppm. Spread minimises it; Bunch maximises it.
    pub max_context_overlap_ppm: u32,
    pub huddle_access: HuddleAccess,
    /// Whether a replacement receives the failed worker's structured
    /// residue (the CERS seam).
    pub disclose_failures_to_replacement: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HuddleAccess {
    None,
    /// The sugar huddle: only changed facts, assignment, constraint,
    /// termination.
    Sugar,
    Full,
}

/// How many may run at once and whether domains may overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyPolicy {
    pub max_concurrent: u16,
    pub allow_domain_overlap: bool,
}

/// What the tactic is allowed to alter. Everything outside this list is
/// constitutionally out of reach for tactics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityPolicy {
    pub may_reassign: bool,
    pub may_reallocate_budget: bool,
    pub may_change_information_topology: bool,
    /// The observer-only restriction: the tactic deploys personnel that
    /// watch but never execute ordinary work.
    pub observe_only: bool,
}

/// The bounded budget of one tactic. A tactic without a budget does not
/// graduate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalBudget {
    pub max_tokens: u64,
    pub max_cost_microunits: u64,
    pub max_duration_ms: u64,
    pub max_plays: u16,
}

impl TacticalBudget {
    pub #[tracing::instrument]
fn exhausted(&self, spent_tokens: u64, spent_micros: u64, plays: u16) -> bool {
        spent_tokens >= self.max_tokens
            || spent_micros >= self.max_cost_microunits
            || plays >= self.max_plays
    }
}

/// The effect a tactic claims to produce, as directional deltas. Admission
/// economics read these claims; post-tactic evidence grades them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedEffect {
    /// Coverage breadth delta, 0..=1_000 ppm (Spread raises it, Heavy
    /// lowers it).
    pub breadth_delta_ppm: u32,
    /// Assurance redundancy delta (Heavy and Max Protect raise it).
    pub protection_delta_ppm: u32,
    /// Tempo delta (Hurry-up and Quick Game raise it; Prevent lowers it).
    pub tempo_delta_ppm: u32,
    /// Diagnostic information expected per unit spend (Motion, Mesh, Flood).
    pub information_delta_ppm: u32,
}

/// When the tactic ends, win or lose. A tactic without termination does not
/// graduate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminationCondition {
    pub max_plays: u16,
    pub abort_when: Vec<AbortTrigger>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbortTrigger {
    BudgetExhausted,
    DivergenceExceedsTolerance,
    CoherenceDebtExceedsPolicy,
    WinConditionSatisfied,
    RepeatedFailureOfThisTactic,
}

/// The complete tactical directive: everything the admission engine, the
/// composition rules, and the executing authorities need.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalDirective {
    pub id: TacticalId,
    /// The machine name, e.g. "spread", "max_protect". The conflict matrix
    /// keys on this.
    pub name: String,
    pub class: TacticalClass,
    pub trigger: TacticalTrigger,
    /// Admission economics, each 0..=1_000 ppm: expected benefit, expected
    /// cost, expected risk of running this tactic now.
    pub benefit_ppm: u32,
    pub cost_ppm: u32,
    pub risk_ppm: u32,
    pub shape: PersonnelShape,
    pub information: InformationPolicy,
    pub concurrency: ConcurrencyPolicy,
    pub authority: AuthorityPolicy,
    pub budget: TacticalBudget,
    pub expected_effect: ExpectedEffect,
    pub termination: TerminationCondition,
    /// The natural counter to this tactic, for the observed-overreaction
    /// path: when this tactic's failure signature appears, the constraint
    /// mechanism proposes the named counter.
    pub counter_tactic: Option<String>,
}

impl TacticalDirective {
    /// Trigger holds AND the economics clear the threshold.
    pub #[tracing::instrument]
fn evaluate_admission(&self, context: &TacticalContext, threshold_ppm: u32) -> Admission {
        crate::admission::admit(self, context, threshold_ppm)
    }

    /// The net expected value in ppm: benefit minus cost minus risk.
    pub #[tracing::instrument]
fn net_value_ppm(&self) -> i64 {
        i64::from(self.benefit_ppm.min(1_000))
            - i64::from(self.cost_ppm.min(1_000))
            - i64::from(self.risk_ppm.min(1_000))
    }
}

/// What a tactic produced, for tactical memory. Evidence grades the claim;
/// the record never rewrites it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalOutcome {
    pub id: TacticalId,
    pub name: String,
    pub admitted: bool,
    pub success: Option<bool>,
    pub tokens_spent: u64,
    pub cost_microunits: u64,
    pub ytg_before_milli: u32,
    pub ytg_after_milli: u32,
    pub failure_mode: Option<String>,
    pub counter_used: Option<String>,
    pub at_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracing::instrument]
fn context() -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 400,
            domain_coupling_ppm: 300,
            provider_instability_ppm: 100,
            coherence_debt: 100,
            repeated_failures: 0,
            risk_band: RiskBand::Low,
            divergence_observed: false,
            critical_domain: None,
            ytg_milli: 10_000,
            budget_remaining_ppm: 900_000,
            short_ytg: false,
            at_ms: 0,
        }
    }

    #[test]
    #[tracing::instrument]
fn triggers_evaluate_deterministically_and_compositionally() {
        let broad = TacticalTrigger::ProblemBreadthAtLeast { ppm: 300 };
        assert!(broad.evaluate(&context()));
        let loose_and_broad = TacticalTrigger::All(vec![
            TacticalTrigger::DomainCouplingAtMost { ppm: 500 },
            broad.clone(),
        ]);
        assert!(loose_and_broad.evaluate(&context()));
        let risky = TacticalTrigger::RiskBandAtLeast {
            band: RiskBand::Elevated,
        };
        assert!(!risky.evaluate(&context()));
        assert!(TacticalTrigger::Not(Box::new(risky.clone())).evaluate(&context()));
        assert!(TacticalTrigger::Any(vec![risky, broad]).evaluate(&context()));
        // The same context always evaluates the same way.
        assert_eq!(
            loose_and_broad.evaluate(&context()),
            loose_and_broad.evaluate(&context())
        );
    }

    #[test]
    #[tracing::instrument]
fn shapes_validate_ownership_rules() {
        let shape = PersonnelShape {
            primary_executors: 2,
            support: 1,
            reserves: 1,
            observers: 0,
            assignments: vec![DomainAssignment {
                domain: "parser".into(),
                personnel: vec![PersonnelId::new("a").unwrap()],
                mode: OwnershipMode::Sole,
            }],
        };
        shape.validate().unwrap();
        // No executor: nothing plays.
        let mut executorless = shape.clone();
        executorless.primary_executors = 0;
        assert!(executorless.validate().is_err());
        // Joint ownership with one owner is a contradiction.
        let mut joint = shape.clone();
        joint.assignments[0].mode = OwnershipMode::Joint;
        assert!(joint.validate().is_err());
        // An empty domain is a gap, and gaps are refused.
        let mut gap = shape.clone();
        gap.assignments[0].personnel.clear();
        assert!(gap.validate().is_err());
        // One person listed twice on one domain is a double booking.
        let mut double_booked = shape.clone();
        double_booked.assignments[0]
            .personnel
            .push(PersonnelId::new("a").unwrap());
        assert!(double_booked.validate().is_err());
    }

    #[test]
    #[tracing::instrument]
fn budgets_exhaust_at_their_ceilings() {
        let budget = TacticalBudget {
            max_tokens: 1_000,
            max_cost_microunits: 100_000,
            max_duration_ms: 5_000,
            max_plays: 3,
        };
        assert!(!budget.exhausted(999, 99_999, 2));
        assert!(budget.exhausted(1_000, 0, 0));
        assert!(budget.exhausted(0, 0, 3));
    }

    #[test]
    #[tracing::instrument]
fn short_ytg_is_the_last_quarter_of_the_reference() {
        assert!(!TacticalContext::is_short_ytg(3_000, 10_000));
        assert!(TacticalContext::is_short_ytg(2_500, 10_000));
        assert!(!TacticalContext::is_short_ytg(2_500, 0));
    }
}
