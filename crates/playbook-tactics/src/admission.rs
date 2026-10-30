//! The deterministic admission engine.
//!
//! Every tactic exposes preconditions, expected benefit, expected cost, and
//! risk. The generic rule: admit a tactic when its trigger holds and its net
//! expected value clears the policy threshold. The same directive and
//! context always decide the same way; a rejection names its reason.

use serde::{Deserialize, Serialize};

use crate::domain::{tactics_refusal, TacticalContext, TacticalDirective};
use playbook_core::PlaybookError;

/// The admission decision for one directive against one context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    /// The trigger fired and the economics cleared the threshold.
    Admitted,
    Rejected {
        reason_code: String,
    },
}

/// The deterministic admission calculation.
pub #[tracing::instrument]
fn admit(
    directive: &TacticalDirective,
    context: &TacticalContext,
    threshold_ppm: u32,
) -> Admission {
    if !directive.trigger.evaluate(context) {
        return Admission::Rejected {
            reason_code: "trigger_not_fired".into(),
        };
    }
    let net = directive.net_value_ppm();
    if net <= i64::from(threshold_ppm) {
        return Admission::Rejected {
            reason_code: format!("net_value_{net}_below_threshold_{threshold_ppm}"),
        };
    }
    // Exhausted tactics do not re-admit themselves.
    if directive.budget.max_plays == 0 {
        return Admission::Rejected {
            reason_code: "budget_declares_no_plays".into(),
        };
    }
    // An observe-only tactic must not claim executors: observers watch.
    if directive.authority.observe_only
        && directive.shape.primary_executors > directive.shape.observers
    {
        return Admission::Rejected {
            reason_code: "observe_only_deploys_executors".into(),
        };
    }
    Admission::Admitted
}

/// Refuse a composition that violates the authority boundary or the frozen
/// contract's validation rules. Shared by every module before it hands a
/// directive upward.
pub #[tracing::instrument]
fn validated(directive: &TacticalDirective) -> Result<(), PlaybookError> {
    directive.shape.validate()?;
    if directive.name.trim().is_empty() {
        return Err(tactics_refusal("directive names no tactic"));
    }
    if directive.termination.max_plays == 0 {
        return Err(tactics_refusal("termination declares no plays"));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::domain::{
        AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment, ExpectedEffect,
        HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, RiskBand, TacticalBudget,
        TacticalClass, TacticalContext, TacticalTrigger, TerminationCondition,
    };
    use playbook_core::{PersonnelId, TacticalId};

    pub #[tracing::instrument]
fn shape() -> PersonnelShape {
        PersonnelShape {
            primary_executors: 3,
            support: 1,
            reserves: 0,
            observers: 0,
            assignments: vec![DomainAssignment {
                domain: "parser".into(),
                personnel: vec![PersonnelId::new("a").unwrap()],
                mode: OwnershipMode::Sole,
            }],
        }
    }

    pub #[tracing::instrument]
fn directive(
        name: &str,
        trigger: TacticalTrigger,
        benefit: u32,
        cost: u32,
        risk: u32,
    ) -> TacticalDirective {
        TacticalDirective {
            id: TacticalId::new(format!("t-{name}")).unwrap(),
            name: name.into(),
            class: TacticalClass::Formation,
            trigger,
            benefit_ppm: benefit,
            cost_ppm: cost,
            risk_ppm: risk,
            shape: shape(),
            information: InformationPolicy {
                max_context_overlap_ppm: 200,
                huddle_access: HuddleAccess::None,
                disclose_failures_to_replacement: true,
            },
            concurrency: ConcurrencyPolicy {
                max_concurrent: 3,
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
                breadth_delta_ppm: 400,
                protection_delta_ppm: 0,
                tempo_delta_ppm: 0,
                information_delta_ppm: 200,
            },
            termination: TerminationCondition {
                max_plays: 2,
                abort_when: vec![AbortTrigger::BudgetExhausted],
            },
            counter_tactic: None,
        }
    }

    pub #[tracing::instrument]
fn context() -> TacticalContext {
        TacticalContext {
            problem_breadth_ppm: 700,
            domain_coupling_ppm: 200,
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
fn admission_requires_trigger_and_economics() {
        let spread = directive(
            "spread",
            TacticalTrigger::ProblemBreadthAtLeast { ppm: 500 },
            700,
            200,
            100,
        );
        assert_eq!(admit(&spread, &context(), 100), Admission::Admitted);
        // The same inputs decide the same way.
        assert_eq!(
            admit(&spread, &context(), 100),
            admit(&spread, &context(), 100)
        );
        // Trigger not fired.
        let narrow = TacticalContext {
            problem_breadth_ppm: 100,
            ..context()
        };
        assert_eq!(
            admit(&spread, &narrow, 100),
            Admission::Rejected {
                reason_code: "trigger_not_fired".into()
            }
        );
        // Economics below the threshold.
        assert_eq!(
            admit(&spread, &context(), 500),
            Admission::Rejected {
                reason_code: "net_value_400_below_threshold_500".into()
            }
        );
    }

    #[test]
    #[tracing::instrument]
fn observe_only_tactics_may_not_claim_executors() {
        let mut spy = directive("spy", TacticalTrigger::Always, 300, 100, 50);
        spy.authority.observe_only = true;
        assert_eq!(
            admit(&spy, &context(), 0),
            Admission::Rejected {
                reason_code: "observe_only_deploys_executors".into()
            }
        );
        let mut watchers = spy;
        watchers.shape.primary_executors = 0;
        watchers.shape.observers = 2;
        watchers.shape.validate().unwrap();
        assert_eq!(admit(&watchers, &context(), 0), Admission::Admitted);
    }

    #[test]
    #[tracing::instrument]
fn validated_refuses_unnamed_or_undefined_directives() {
        let mut good = directive("spread", TacticalTrigger::Always, 400, 100, 50);
        assert!(validated(&good).is_ok());
        good.name = "  ".into();
        assert!(validated(&good).is_err());
        good.name = "spread".into();
        good.termination.max_plays = 0;
        assert!(validated(&good).is_err());
    }
}
