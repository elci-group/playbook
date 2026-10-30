//! Dynamic responsibility transfer: the Banjo and the Scrape Exchange.
//!
//! Two football mechanisms, one doctrine: responsibility may move between
//! agents, but only by declared rule and with a journalled record — never by
//! silent renegotiation mid-play.
//!
//! Football → computational:
//!
//! - Banjo coverage (directive §7.18): two defenders face two crossing
//!   receivers and trade assignments by pre-agreed rule as the routes
//!   develop — the defender waiting on a route takes it the moment it
//!   becomes active. → Two agents jointly own two coupled domains; at any
//!   moment one holds primary on one domain and secondary on the other;
//!   the active-domain trigger swaps the primaries deterministically.
//!   Unlike Manmark's fixed assignment, ownership here is joint and the
//!   assignment moves with the problem.
//! - Scrape exchange (directive §7.17): a defender who meets a form he
//!   cannot beat hands his gap to the teammate scraping past and takes the
//!   teammate's path. → When the problem changes form mid-execution (the
//!   runtime failure reveals an architectural cause), the failing owner
//!   hands the failing role to the partner and takes the partner's former
//!   role. The exchange is explicit and journalled: an [`Exchange`] record,
//!   not a drift.
//! - No unblocked gap: after an exchange both agents' new roles are
//!   declared at once ([`post_exchange_roles`]), so no responsibility gap
//!   opens between the handoff and the next play.
//! - A defender never chases yesterday's route: an exchange computed
//!   against state older than the current observation is stale-owner
//!   rejected ([`reject_stale`]).
//!
//! Authority boundary (directive §13): these functions journal and
//! recommend transfers of responsibility. They never touch evidence truth,
//! YTG, gate satisfaction, or an accepted tool result — tactics recommend;
//! the existing authorities execute.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError, TacticalId};

use crate::domain::{
    tactics_refusal, AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment,
    ExpectedEffect, HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, TacticalBudget,
    TacticalClass, TacticalContext, TacticalDirective, TacticalTrigger, TerminationCondition,
};

/// The two agents and the two coupled domains they share responsibility
/// for. `domain_x` and `domain_y` name the domains; which agent currently
/// holds primary on each lives in [`BanjoState`], not here — a pair is a
/// roster, a state is a moment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipPair {
    pub agent_a: PersonnelId,
    pub agent_b: PersonnelId,
    pub domain_x: String,
    pub domain_y: String,
}

/// The Banjo pairing: the pair jointly owns X and Y. At any moment one
/// agent holds primary on one domain and secondary on the other; which is
/// which is tracked by [`BanjoState`].
///
/// Refusals: a pair needs two distinct agents ("banjo_needs_two_agents")
/// and two distinct, non-empty domains ("banjo_needs_two_domains").
pub #[tracing::instrument]
fn banjo(
    agent_a: PersonnelId,
    agent_b: PersonnelId,
    domain_x: &str,
    domain_y: &str,
) -> Result<OwnershipPair, PlaybookError> {
    if agent_a == agent_b {
        return Err(tactics_refusal("banjo_needs_two_agents"));
    }
    if domain_x.trim().is_empty() || domain_y.trim().is_empty() || domain_x == domain_y {
        return Err(tactics_refusal("banjo_needs_two_domains"));
    }
    Ok(OwnershipPair {
        agent_a,
        agent_b,
        domain_x: domain_x.into(),
        domain_y: domain_y.into(),
    })
}

/// Which member of an [`OwnershipPair`] currently holds primary on a
/// domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Holder {
    AgentA,
    AgentB,
}

impl Holder {
    /// The other member of the pair.
    #[tracing::instrument]
fn other(self) -> Self {
        match self {
            Self::AgentA => Self::AgentB,
            Self::AgentB => Self::AgentA,
        }
    }
}

/// The Banjo at one moment: who currently holds primary on `domain_x` (the
/// other agent holds primary on `domain_y`), how many deterministic
/// switches have fired, and the monotonic observation time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BanjoState {
    pub pair: OwnershipPair,
    pub x_primary: Holder,
    pub switches: u32,
    pub at_ms: u64,
}

/// The opening Banjo look: agent A takes primary on `domain_x`, agent B on
/// `domain_y`, and nothing has switched yet.
pub #[tracing::instrument]
fn initial(pair: OwnershipPair, at_ms: u64) -> BanjoState {
    BanjoState {
        pair,
        x_primary: Holder::AgentA,
        switches: 0,
        at_ms,
    }
}

/// The measured trigger of a Banjo switch: one of the two domains just
/// became active.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchTrigger {
    DomainXActive,
    DomainYActive,
}

/// The deterministic Banjo switch: the waiting agent takes the active
/// domain, its partner takes the other. There are exactly two agents and
/// two domains, so whichever domain fired, the effect is one full exchange
/// of both primaries: `x_primary` flips and `domain_y` moves with it. The
/// switch counter increments (saturating) and the state is stamped with
/// the observation time. Time is monotonic: a switch at an `at_ms` earlier
/// than the current state is refused ("switch_time_regressed"). An equal
/// timestamp is permitted — the trigger and the observation may share a
/// tick.
pub #[tracing::instrument]
fn switch_on(
    state: BanjoState,
    trigger: SwitchTrigger,
    at_ms: u64,
) -> Result<BanjoState, PlaybookError> {
    if at_ms < state.at_ms {
        return Err(tactics_refusal("switch_time_regressed"));
    }
    // Either trigger names the domain that just became active, and the
    // rule is deliberately symmetric: the waiting agent takes the active
    // domain and its partner takes the other. With exactly two agents and
    // two domains that is one full exchange of both primaries — which is
    // why the trigger does not change the arithmetic below.
    let _ = trigger;
    Ok(BanjoState {
        pair: state.pair,
        x_primary: state.x_primary.other(),
        switches: state.switches.saturating_add(1),
        at_ms: at_ms.max(state.at_ms),
    })
}

/// The agent currently holding primary on `domain`, or `None` when the
/// domain is not one of the pair's two domains.
pub #[tracing::instrument]
fn primary_of(state: &BanjoState, domain: &str) -> Option<PersonnelId> {
    if domain == state.pair.domain_x {
        Some(match state.x_primary {
            Holder::AgentA => state.pair.agent_a.clone(),
            Holder::AgentB => state.pair.agent_b.clone(),
        })
    } else if domain == state.pair.domain_y {
        Some(match state.x_primary {
            Holder::AgentA => state.pair.agent_b.clone(),
            Holder::AgentB => state.pair.agent_a.clone(),
        })
    } else {
        None
    }
}

/// The Scrape Exchange journal record: explicit and journalled, so a
/// responsibility transfer is evidence of a decision, not a silent drift.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    /// The agent who held the failing role.
    pub out_agent: PersonnelId,
    /// The partner taking it over.
    pub in_agent: PersonnelId,
    /// The role `out_agent` held before the exchange (and `in_agent` holds
    /// after it).
    pub from_role: String,
    /// The role `out_agent` takes over (formerly `in_agent`'s).
    pub to_role: String,
    /// Why the problem changed form, as a stable machine code.
    pub reason_code: String,
    pub at_ms: u64,
}

/// Scrape Exchange (§7.17): the problem changed form mid-execution. The
/// current owner hands the failing role to the partner and takes the
/// partner's former role.
///
/// Deterministic refusals: an agent cannot exchange with himself
/// ("exchange_needs_two_agents"); both roles must be named
/// ("exchange_needs_roles"); and the journal clock cannot read zero — time
/// zero precedes every observed state, so a record stamped there could
/// only be a regression ("exchange_time_regressed"). Staleness against the
/// last observed state is a separate, explicit check: [`reject_stale`].
pub #[tracing::instrument]
fn scrape_exchange(
    out_agent: PersonnelId,
    in_agent: PersonnelId,
    from_role: &str,
    to_role: &str,
    reason_code: &str,
    at_ms: u64,
) -> Result<Exchange, PlaybookError> {
    if out_agent == in_agent {
        return Err(tactics_refusal("exchange_needs_two_agents"));
    }
    if from_role.trim().is_empty() || to_role.trim().is_empty() {
        return Err(tactics_refusal("exchange_needs_roles"));
    }
    if at_ms == 0 {
        return Err(tactics_refusal("exchange_time_regressed"));
    }
    Ok(Exchange {
        out_agent,
        in_agent,
        from_role: from_role.into(),
        to_role: to_role.into(),
        reason_code: reason_code.into(),
        at_ms,
    })
}

/// Stale-owner rejection: an exchange decision computed against state
/// older than the current observation is refused. The exchange is valid
/// exactly when `computed_at >= last_state_at_ms`
/// ("exchange_stale_owner").
pub #[tracing::instrument]
fn reject_stale(last_state_at_ms: u64, computed_at: u64) -> Result<(), PlaybookError> {
    if computed_at < last_state_at_ms {
        return Err(tactics_refusal("exchange_stale_owner"));
    }
    Ok(())
}

/// Recovery semantics: after an exchange, both agents' new roles are
/// declared, so no responsibility gap opens. Returns
/// `((out_agent, to_role), (in_agent, from_role))`.
pub #[tracing::instrument]
fn post_exchange_roles(exchange: &Exchange) -> ((PersonnelId, String), (PersonnelId, String)) {
    (
        (exchange.out_agent.clone(), exchange.to_role.clone()),
        (exchange.in_agent.clone(), exchange.from_role.clone()),
    )
}

/// Which exchange doctrine a directive expresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExchangeKind {
    Banjo,
    ScrapeExchange,
}

#[tracing::instrument]
fn template_agent_a() -> PersonnelId {
    PersonnelId::new("agent_a").unwrap()
}

#[tracing::instrument]
fn template_agent_b() -> PersonnelId {
    PersonnelId::new("agent_b").unwrap()
}

/// Assemble one exchange directive. The economics, budget, and effect are
/// fixed template values; the caller's kind decides the shape, the
/// trigger, the information topology, and the termination.
#[allow(clippy::too_many_arguments)]
#[tracing::instrument]
fn directive_template(
    name: &str,
    class: TacticalClass,
    trigger: TacticalTrigger,
    shape: PersonnelShape,
    information: InformationPolicy,
    concurrency: ConcurrencyPolicy,
    termination: TerminationCondition,
    counter: Option<&str>,
) -> TacticalDirective {
    TacticalDirective {
        id: TacticalId::new(format!("t-{name}")).unwrap(),
        name: name.into(),
        class,
        trigger,
        benefit_ppm: 500,
        cost_ppm: 150,
        risk_ppm: 100,
        shape,
        information,
        concurrency,
        authority: AuthorityPolicy {
            may_reassign: true,
            may_reallocate_budget: false,
            may_change_information_topology: false,
            observe_only: false,
        },
        budget: TacticalBudget {
            max_tokens: 8_000,
            max_cost_microunits: 800_000,
            max_duration_ms: 15_000,
            max_plays: 2,
        },
        expected_effect: ExpectedEffect {
            breadth_delta_ppm: 0,
            protection_delta_ppm: 200,
            tempo_delta_ppm: 0,
            information_delta_ppm: 300,
        },
        termination,
        counter_tactic: counter.map(str::to_owned),
    }
}

/// The exchange directives, fully specified for admission:
///
/// - [`ExchangeKind::Banjo`]: class Adjustment; fires when divergence is
///   observed or domain coupling reaches 600 ppm; the two template agents
///   Joint-own the two domains (a shape that validates); authority
///   includes reassignment; the natural counter is bracket coverage.
/// - [`ExchangeKind::ScrapeExchange`]: class Play; fires at two repeated
///   failures; two executors Sole-own two domains; authority includes
///   reassignment; and its termination aborts on
///   [`AbortTrigger::RepeatedFailureOfThisTactic`] — an exchange loop is
///   itself the failure mode: if the partners keep handing the same
///   problem back and forth, the tactic has stopped transferring
///   responsibility and started laundering it.
///
/// Both pass [`crate::admission::validated`]. Construction is
/// measurement-independent: the triggers read the measured context at
/// admission time, not here.
pub #[tracing::instrument]
fn directive(kind: ExchangeKind, context: &TacticalContext) -> TacticalDirective {
    // The template is deliberately measurement-independent; the context
    // belongs to admission, not construction.
    let _ = context;
    match kind {
        ExchangeKind::Banjo => directive_template(
            "banjo",
            TacticalClass::Adjustment,
            TacticalTrigger::Any(vec![
                TacticalTrigger::DivergenceObserved,
                TacticalTrigger::DomainCouplingAtLeast { ppm: 600 },
            ]),
            PersonnelShape {
                primary_executors: 2,
                support: 0,
                reserves: 0,
                observers: 0,
                assignments: vec![
                    DomainAssignment {
                        domain: "domain_x".into(),
                        personnel: vec![template_agent_a(), template_agent_b()],
                        mode: OwnershipMode::Joint,
                    },
                    DomainAssignment {
                        domain: "domain_y".into(),
                        personnel: vec![template_agent_a(), template_agent_b()],
                        mode: OwnershipMode::Joint,
                    },
                ],
            },
            InformationPolicy {
                max_context_overlap_ppm: 500,
                huddle_access: HuddleAccess::Full,
                disclose_failures_to_replacement: true,
            },
            ConcurrencyPolicy {
                max_concurrent: 2,
                allow_domain_overlap: true,
            },
            TerminationCondition {
                max_plays: 2,
                abort_when: vec![
                    AbortTrigger::BudgetExhausted,
                    AbortTrigger::DivergenceExceedsTolerance,
                ],
            },
            Some("bracket"),
        ),
        ExchangeKind::ScrapeExchange => directive_template(
            "scrape_exchange",
            TacticalClass::Play,
            TacticalTrigger::RepeatedFailuresAtLeast { attempts: 2 },
            PersonnelShape {
                primary_executors: 2,
                support: 0,
                reserves: 0,
                observers: 0,
                assignments: vec![
                    DomainAssignment {
                        domain: "failing_role".into(),
                        personnel: vec![template_agent_a()],
                        mode: OwnershipMode::Sole,
                    },
                    DomainAssignment {
                        domain: "partner_role".into(),
                        personnel: vec![template_agent_b()],
                        mode: OwnershipMode::Sole,
                    },
                ],
            },
            InformationPolicy {
                max_context_overlap_ppm: 200,
                huddle_access: HuddleAccess::Sugar,
                disclose_failures_to_replacement: true,
            },
            ConcurrencyPolicy {
                max_concurrent: 2,
                allow_domain_overlap: false,
            },
            TerminationCondition {
                max_plays: 2,
                abort_when: vec![
                    AbortTrigger::BudgetExhausted,
                    AbortTrigger::RepeatedFailureOfThisTactic,
                ],
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::RiskBand;

    /// The canonical pair: runtime containment and root cause.
    #[tracing::instrument]
fn pair() -> OwnershipPair {
        banjo(
            PersonnelId::new("runtime").unwrap(),
            PersonnelId::new("arch").unwrap(),
            "runtime_containment",
            "root_cause",
        )
        .unwrap()
    }

    #[tracing::instrument]
fn error_code(err: PlaybookError) -> String {
        match err {
            PlaybookError::Inadmissible(authority, code) => {
                assert_eq!(authority, "tactics");
                code
            }
            other => panic!("expected a tactics refusal, got {other:?}"),
        }
    }

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
fn banjo_refuses_degenerate_pairs_and_builds_clean_ones() {
        let a = PersonnelId::new("runtime").unwrap();
        let b = PersonnelId::new("arch").unwrap();
        // One agent cannot banjo with himself.
        assert_eq!(
            error_code(banjo(a.clone(), a.clone(), "x", "y").unwrap_err()),
            "banjo_needs_two_agents"
        );
        // Two domains must differ.
        assert_eq!(
            error_code(banjo(a.clone(), b.clone(), "runtime", "runtime").unwrap_err()),
            "banjo_needs_two_domains"
        );
        // Two domains must be named.
        assert_eq!(
            error_code(banjo(a.clone(), b.clone(), "runtime", "   ").unwrap_err()),
            "banjo_needs_two_domains"
        );
        let pair = banjo(a, b, "runtime_containment", "root_cause").unwrap();
        assert_eq!(pair.domain_x, "runtime_containment");
        assert_eq!(pair.domain_y, "root_cause");
    }

    #[test]
    #[tracing::instrument]
fn initial_hands_the_first_domain_to_agent_a() {
        let state = initial(pair(), 1_000);
        assert_eq!(state.x_primary, Holder::AgentA);
        assert_eq!(state.switches, 0);
        assert_eq!(state.at_ms, 1_000);
        assert_eq!(
            primary_of(&state, "runtime_containment").unwrap(),
            PersonnelId::new("runtime").unwrap()
        );
        assert_eq!(
            primary_of(&state, "root_cause").unwrap(),
            PersonnelId::new("arch").unwrap()
        );
    }

    #[test]
    #[tracing::instrument]
fn any_active_domain_trigger_swaps_both_primaries() {
        let start = initial(pair(), 1_000);
        // The waiting agent takes the active domain: either trigger is a
        // full exchange of both primaries.
        let after_x = switch_on(start, SwitchTrigger::DomainXActive, 2_000).unwrap();
        assert_eq!(after_x.x_primary, Holder::AgentB);
        assert_eq!(after_x.switches, 1);
        assert_eq!(after_x.at_ms, 2_000);
        let after_y = switch_on(after_x, SwitchTrigger::DomainYActive, 3_000).unwrap();
        assert_eq!(after_y.x_primary, Holder::AgentA);
        assert_eq!(after_y.switches, 2);
        // A shared tick is not a regression.
        let same_tick = switch_on(after_y, SwitchTrigger::DomainXActive, 3_000).unwrap();
        assert_eq!(same_tick.at_ms, 3_000);
        assert_eq!(same_tick.switches, 3);
        // Time only moves forward.
        assert_eq!(
            error_code(switch_on(same_tick, SwitchTrigger::DomainYActive, 2_999).unwrap_err()),
            "switch_time_regressed"
        );
    }

    #[test]
    #[tracing::instrument]
fn primary_of_tracks_the_swapped_holders() {
        let start = initial(pair(), 1_000);
        let swapped = switch_on(start, SwitchTrigger::DomainXActive, 2_000).unwrap();
        assert_eq!(
            primary_of(&swapped, "runtime_containment").unwrap(),
            PersonnelId::new("arch").unwrap()
        );
        assert_eq!(
            primary_of(&swapped, "root_cause").unwrap(),
            PersonnelId::new("runtime").unwrap()
        );
        // A domain outside the pair has no holder.
        assert_eq!(primary_of(&swapped, "special_teams"), None);
    }

    #[test]
    #[tracing::instrument]
fn scrape_exchange_refuses_degenerate_records() {
        let a = PersonnelId::new("runtime").unwrap();
        let b = PersonnelId::new("arch").unwrap();
        assert_eq!(
            error_code(
                scrape_exchange(
                    a.clone(),
                    a.clone(),
                    "containment",
                    "root_cause",
                    "form_changed",
                    5_000
                )
                .unwrap_err()
            ),
            "exchange_needs_two_agents"
        );
        assert_eq!(
            error_code(
                scrape_exchange(
                    a.clone(),
                    b.clone(),
                    "  ",
                    "root_cause",
                    "form_changed",
                    5_000
                )
                .unwrap_err()
            ),
            "exchange_needs_roles"
        );
        assert_eq!(
            error_code(
                scrape_exchange(
                    a.clone(),
                    b.clone(),
                    "containment",
                    "",
                    "form_changed",
                    5_000
                )
                .unwrap_err()
            ),
            "exchange_needs_roles"
        );
        assert_eq!(
            error_code(
                scrape_exchange(a, b, "containment", "root_cause", "form_changed", 0).unwrap_err()
            ),
            "exchange_time_regressed"
        );
    }

    #[test]
    #[tracing::instrument]
fn scrape_exchange_journals_the_full_handoff() {
        let record = scrape_exchange(
            PersonnelId::new("runtime").unwrap(),
            PersonnelId::new("arch").unwrap(),
            "runtime_containment",
            "root_cause",
            "runtime_failure_reveals_architectural_cause",
            5_000,
        )
        .unwrap();
        assert_eq!(record.out_agent, PersonnelId::new("runtime").unwrap());
        assert_eq!(record.in_agent, PersonnelId::new("arch").unwrap());
        assert_eq!(record.from_role, "runtime_containment");
        assert_eq!(record.to_role, "root_cause");
        assert_eq!(
            record.reason_code,
            "runtime_failure_reveals_architectural_cause"
        );
        assert_eq!(record.at_ms, 5_000);
    }

    #[test]
    #[tracing::instrument]
fn post_exchange_roles_declares_both_sides_exactly() {
        let record = scrape_exchange(
            PersonnelId::new("runtime").unwrap(),
            PersonnelId::new("arch").unwrap(),
            "containment",
            "root_cause",
            "form_changed",
            5_000,
        )
        .unwrap();
        let ((out_agent, out_role), (in_agent, in_role)) = post_exchange_roles(&record);
        assert_eq!(out_agent, record.out_agent);
        assert_eq!(out_role, record.to_role);
        assert_eq!(in_agent, record.in_agent);
        assert_eq!(in_role, record.from_role);
    }

    #[test]
    #[tracing::instrument]
fn reject_stale_draws_the_boundary_at_equality() {
        // Computed against exactly the observed state: valid.
        assert!(reject_stale(5_000, 5_000).is_ok());
        // Computed against fresher state: valid.
        assert!(reject_stale(5_000, 6_000).is_ok());
        // Computed against older state: stale owner, refused.
        assert_eq!(
            error_code(reject_stale(5_000, 4_999).unwrap_err()),
            "exchange_stale_owner"
        );
    }

    #[test]
    #[tracing::instrument]
fn exchange_directives_validate() {
        let context = context();
        let banjo_directive = directive(ExchangeKind::Banjo, &context);
        crate::admission::validated(&banjo_directive).unwrap();
        assert_eq!(banjo_directive.name, "banjo");
        assert_eq!(banjo_directive.class, TacticalClass::Adjustment);
        assert!(banjo_directive.authority.may_reassign);
        // Fires on divergence, or on tight coupling.
        let mut divergence = context.clone();
        divergence.divergence_observed = true;
        assert!(banjo_directive.trigger.evaluate(&divergence));
        let mut coupled = context.clone();
        coupled.domain_coupling_ppm = 600;
        assert!(banjo_directive.trigger.evaluate(&coupled));
        assert!(!banjo_directive.trigger.evaluate(&context));

        let scrape = directive(ExchangeKind::ScrapeExchange, &context);
        crate::admission::validated(&scrape).unwrap();
        assert_eq!(scrape.name, "scrape_exchange");
        assert_eq!(scrape.class, TacticalClass::Play);
        assert!(scrape.authority.may_reassign);
        // An exchange loop is the failure mode: the tactic must abort on
        // its own repeated failure.
        assert!(scrape
            .termination
            .abort_when
            .contains(&AbortTrigger::RepeatedFailureOfThisTactic));
        let mut failing = context.clone();
        failing.repeated_failures = 2;
        assert!(scrape.trigger.evaluate(&failing));
        assert!(!scrape.trigger.evaluate(&context));
    }

    #[test]
    #[tracing::instrument]
fn exchange_is_deterministic() {
        let context = context();
        // The same inputs always produce the same pairs, states, records,
        // and directives.
        assert_eq!(pair(), pair());
        assert_eq!(initial(pair(), 1), initial(pair(), 1));
        let state = initial(pair(), 1_000);
        assert_eq!(
            switch_on(state.clone(), SwitchTrigger::DomainXActive, 2_000).unwrap(),
            switch_on(state, SwitchTrigger::DomainXActive, 2_000).unwrap()
        );
        assert_eq!(
            directive(ExchangeKind::Banjo, &context),
            directive(ExchangeKind::Banjo, &context)
        );
        assert_eq!(
            directive(ExchangeKind::ScrapeExchange, &context),
            directive(ExchangeKind::ScrapeExchange, &context)
        );
        assert_eq!(
            scrape_exchange(
                PersonnelId::new("runtime").unwrap(),
                PersonnelId::new("arch").unwrap(),
                "containment",
                "root_cause",
                "form_changed",
                9,
            )
            .unwrap(),
            scrape_exchange(
                PersonnelId::new("runtime").unwrap(),
                PersonnelId::new("arch").unwrap(),
                "containment",
                "root_cause",
                "form_changed",
                9,
            )
            .unwrap()
        );
    }
}
