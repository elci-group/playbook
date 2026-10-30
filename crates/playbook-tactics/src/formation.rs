//! The formation engine: who is deployed, and in what arrangement.
//!
//! A formation is the shape of the deployment before any play is called.
//! Each football concept earns its place as a literal computational
//! translation:
//!
//! - Spread — a broad, loosely coupled surface becomes many parallel lanes,
//!   one sole owner per domain, minimal shared context (overlap <= 100 ppm).
//! - Heavy — danger or the endgame becomes one primary executor wrapped in
//!   deterministic support and reserves.
//! - Trips — one dominant domain becomes three personnel concentrated on it,
//!   with the remainder covering globally.
//! - Bunch — observed divergence becomes a tight cooperative micro-team on
//!   one shared context with full huddle access.
//! - Empty — nothing left to hedge becomes total commitment: every deployed
//!   hand executes, zero reserves.
//! - Nickel — rising risk becomes a substitution: one generalist executor
//!   swapped for one specialist reviewer.
//! - Dime — high risk becomes maximum specialist review density.
//! - Bear — a central, tightly coupled shared core becomes deterministic
//!   specialist controls over that core and nothing else.
//! - TwoHigh — unstable conditions become two independent reserves held for
//!   two divergent threat classes.
//!
//! [`profile`] is the doctrine table. [`plan`] walks a fixed, documented
//! ladder from the measured context to a formation and deploys only the
//! personnel it was actually given — it never invents personnel.
//! [`formation_directive`] renders the full [`TacticalDirective`] for a
//! named formation. Nickel, Dime, Bear, and TwoHigh sit outside the frozen
//! composition matrix; they validate and admit individually. Formations
//! recommend; the existing authorities execute.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError, TacticalId};

use crate::domain::{
    tactics_refusal, AbortTrigger, AuthorityPolicy, ConcurrencyPolicy, DomainAssignment,
    ExpectedEffect, HuddleAccess, InformationPolicy, OwnershipMode, PersonnelShape, RiskBand,
    TacticalBudget, TacticalClass, TacticalContext, TacticalDirective, TacticalTrigger,
    TerminationCondition,
};

/// A problem counts as broad at this reading, 0..=1_000 ppm.
pub const SPREAD_BREADTH_MIN_PPM: u32 = 600;
/// A problem counts as loosely coupled at or below this reading, 0..=1_000
/// ppm. Above it, broad parallel formations pay more than they cover.
pub const SPREAD_COUPLING_MAX_PPM: u32 = 400;
/// Provider instability counts as high strictly above this reading,
/// 0..=1_000 ppm, and forces concentration regardless of breadth.
pub const HEAVY_INSTABILITY_PPM: u32 = 600;
/// Trips concentration: the trip is three personnel on one domain, or the
/// whole squad when fewer are available.
pub const TRIP_SIZE: u16 = 3;
/// Deterministic domain names for distributed deployments. Positions past
/// the table fall back to `domain-N`.
pub const DOMAIN_ORDINALS: &[&str] = &[
    "primary",
    "secondary",
    "tertiary",
    "quaternary",
    "quinary",
    "senary",
    "septenary",
    "octonary",
];
/// The directive token ceiling before short-YTG and tight-budget
/// modulation.
pub const DIRECTIVE_MAX_TOKENS: u64 = 50_000;
/// At or below this remaining budget share, 0..=1_000_000 ppm, a formation
/// directive halves its token ceiling.
pub const TIGHT_BUDGET_PPM: u32 = 200_000;

/// A called formation. Each variant is one literal computational
/// translation of its football meaning; `serde` names are snake_case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TacticalFormation {
    /// Broad parallel ownership: many concurrent lanes on distinct domains,
    /// one sole owner each, minimal context overlap (overlap <= 100 ppm).
    Spread,
    /// Concentration: one primary executor plus deterministic support and
    /// reserves — migrations, security patches, release closure, short-YTG.
    Heavy,
    /// Concentrated parallel responsibility: three personnel assigned to one
    /// dominant domain, the remainder on global coverage.
    Trips,
    /// Concentrated cooperative interaction: a tight micro-team sharing one
    /// context (overlap >= 800 ppm) under full huddle access.
    Bunch,
    /// Total commitment: all personnel on execution, zero reserves held
    /// back. Its natural counter is protection (`max_protect`).
    Empty,
    /// Specialist substitution: a generalist executor swapped for one
    /// specialist reviewer.
    Nickel,
    /// Maximum specialist review density: several specialist reviewers
    /// around a reduced execution core.
    Dime,
    /// Deterministic controls over central shared infrastructure: every
    /// deployed hand is a specialist control on one core domain.
    Bear,
    /// Divergent hedging: two independent reserves held against two
    /// different threat classes (reserves >= 400 ppm of the deployment).
    TwoHigh,
}

impl TacticalFormation {
    /// The machine name the conflict matrix and tactical memory key on.
    /// Matches the serde representation exactly.
    pub #[tracing::instrument]
fn machine_name(self) -> &'static str {
        match self {
            Self::Spread => "spread",
            Self::Heavy => "heavy",
            Self::Trips => "trips",
            Self::Bunch => "bunch",
            Self::Empty => "empty",
            Self::Nickel => "nickel",
            Self::Dime => "dime",
            Self::Bear => "bear",
            Self::TwoHigh => "two_high",
        }
    }
}

/// The doctrine table row for one formation: how many may run at once, how
/// much context concurrent personnel may share, who reaches the huddle, and
/// how the deployment divides between reserves and specialists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormationProfile {
    /// Personnel that may operate at once.
    pub max_concurrent: u16,
    /// Maximum context overlap between concurrent personnel, 0..=1_000 ppm.
    pub context_overlap_ppm: u32,
    /// Who may join the huddle: nobody, the sugar huddle (changed facts
    /// only), or full access.
    pub huddle_access: HuddleAccess,
    /// The planned reserve share of the deployment, 0..=1_000 ppm. Empty
    /// holds none; TwoHigh holds the most.
    pub reserves_share_ppm: u32,
    /// The planned specialist share of the deployment, 0..=1_000 ppm. Bear
    /// is entirely specialist controls; Empty is entirely executors.
    pub specialist_share_ppm: u32,
}

/// The doctrine table. Deterministic and total: the same formation always
/// reads the same profile.
pub #[tracing::instrument]
fn profile(formation: TacticalFormation) -> FormationProfile {
    match formation {
        TacticalFormation::Spread => FormationProfile {
            max_concurrent: 6,
            context_overlap_ppm: 50,
            huddle_access: HuddleAccess::None,
            reserves_share_ppm: 0,
            specialist_share_ppm: 200,
        },
        TacticalFormation::Heavy => FormationProfile {
            max_concurrent: 2,
            context_overlap_ppm: 400,
            huddle_access: HuddleAccess::Full,
            reserves_share_ppm: 100,
            specialist_share_ppm: 800,
        },
        TacticalFormation::Trips => FormationProfile {
            max_concurrent: 3,
            context_overlap_ppm: 600,
            huddle_access: HuddleAccess::Sugar,
            reserves_share_ppm: 100,
            specialist_share_ppm: 400,
        },
        TacticalFormation::Bunch => FormationProfile {
            max_concurrent: 3,
            context_overlap_ppm: 900,
            huddle_access: HuddleAccess::Full,
            reserves_share_ppm: 0,
            specialist_share_ppm: 300,
        },
        TacticalFormation::Empty => FormationProfile {
            max_concurrent: 8,
            context_overlap_ppm: 50,
            huddle_access: HuddleAccess::None,
            reserves_share_ppm: 0,
            specialist_share_ppm: 0,
        },
        TacticalFormation::Nickel => FormationProfile {
            max_concurrent: 5,
            context_overlap_ppm: 300,
            huddle_access: HuddleAccess::Sugar,
            reserves_share_ppm: 100,
            specialist_share_ppm: 500,
        },
        TacticalFormation::Dime => FormationProfile {
            max_concurrent: 6,
            context_overlap_ppm: 250,
            huddle_access: HuddleAccess::Sugar,
            reserves_share_ppm: 100,
            specialist_share_ppm: 800,
        },
        TacticalFormation::Bear => FormationProfile {
            max_concurrent: 2,
            context_overlap_ppm: 200,
            huddle_access: HuddleAccess::Full,
            reserves_share_ppm: 0,
            specialist_share_ppm: 1_000,
        },
        TacticalFormation::TwoHigh => FormationProfile {
            max_concurrent: 4,
            context_overlap_ppm: 200,
            huddle_access: HuddleAccess::Sugar,
            reserves_share_ppm: 600,
            specialist_share_ppm: 300,
        },
    }
}

/// The deterministic formation planner. It reads the measured context,
/// walks the ladder below, and deploys the available personnel — at most
/// `available.len()`, never an invented one. With no personnel it refuses
/// with `no personnel available`. The same inputs always produce the same
/// shape.
///
/// The ladder, in order:
///
/// 1. Risk `Critical`, or provider instability strictly above
///    [`HEAVY_INSTABILITY_PPM`], routes to **Heavy** concentration: one
///    primary executor; the remainder splits into deterministic support and
///    reserves (migrations, security patches, release closure, short-YTG).
/// 2. A named critical domain routes to **Trips**: [`TRIP_SIZE`] personnel
///    assigned to that domain (or the whole squad when fewer are available),
///    the remainder on global coverage.
/// 3. Breadth at or above [`SPREAD_BREADTH_MIN_PPM`] with coupling at or
///    below [`SPREAD_COUPLING_MAX_PPM`] routes to **Spread**: the squad
///    splits across distinct domains, one sole owner per domain, no domain
///    with two owners, minimal overlap.
/// 4. Observed divergence routes to **Bunch**: one tight cooperative
///    micro-team on a single shared domain under joint ownership (sole when
///    only one person is available).
/// 5. Otherwise the balanced default: up to two executors on separate
///    domains and up to one reserve; support deploys without a domain.
///
/// A caller `domain_hint` steers the concentration domain of the Heavy and
/// Bunch deployments, and the Trips trip; absent a hint the named critical
/// domain is used, and absent both the fallback `primary`. The returned
/// shape is validated before it is handed back.
pub #[tracing::instrument]
fn plan(
    context: &TacticalContext,
    available: &[PersonnelId],
    domain_hint: Option<&str>,
) -> Result<PersonnelShape, PlaybookError> {
    if available.is_empty() {
        return Err(tactics_refusal("no personnel available"));
    }
    let shape = if context.risk_band >= RiskBand::Critical
        || context.provider_instability_ppm > HEAVY_INSTABILITY_PPM
    {
        heavy_shape(available, resolve_domain(domain_hint, context, "primary"))
    } else if context.critical_domain.is_some() {
        trips_shape(available, resolve_domain(domain_hint, context, "primary"))
    } else if context.problem_breadth_ppm >= SPREAD_BREADTH_MIN_PPM
        && context.domain_coupling_ppm <= SPREAD_COUPLING_MAX_PPM
    {
        spread_shape(available)
    } else if context.divergence_observed {
        bunch_shape(available, resolve_domain(domain_hint, context, "primary"))
    } else {
        balanced_shape(available)
    };
    // Defence in depth: the planner's own output must always validate. A
    // defect here is refused, never deployed.
    shape.validate()?;
    Ok(shape)
}

/// Heavy concentration: one executor, the rest split between deterministic
/// support (the larger half) and reserves. Support shares the executor's
/// domain; reserves hold no domain — they are held out of execution.
#[tracing::instrument]
fn heavy_shape(available: &[PersonnelId], domain: String) -> PersonnelShape {
    let n = u16::try_from(available.len()).unwrap_or(u16::MAX);
    let remaining = n.saturating_sub(1);
    let reserves = remaining / 2;
    let support = remaining.saturating_sub(reserves);
    let committed = usize::from(support).saturating_add(1);
    PersonnelShape {
        primary_executors: 1,
        support,
        reserves,
        observers: 0,
        assignments: vec![DomainAssignment {
            domain,
            personnel: available[..committed.min(available.len())].to_vec(),
            mode: OwnershipMode::Shared,
        }],
    }
}

/// Trips concentration: the first [`TRIP_SIZE`] personnel take the dominant
/// domain under explicit shared ownership; the remainder covers globally —
/// jointly when several, solely when one.
#[tracing::instrument]
fn trips_shape(available: &[PersonnelId], domain: String) -> PersonnelShape {
    let n = u16::try_from(available.len()).unwrap_or(u16::MAX);
    let trip = n.min(TRIP_SIZE);
    let trip_end = usize::from(trip);
    let support = n.saturating_sub(trip);
    let mut assignments = vec![DomainAssignment {
        domain,
        personnel: available[..trip_end].to_vec(),
        mode: OwnershipMode::Shared,
    }];
    let rest = &available[trip_end..];
    if !rest.is_empty() {
        assignments.push(DomainAssignment {
            domain: "global".into(),
            personnel: rest.to_vec(),
            mode: if rest.len() >= 2 {
                OwnershipMode::Shared
            } else {
                OwnershipMode::Sole
            },
        });
    }
    PersonnelShape {
        primary_executors: trip,
        support,
        reserves: 0,
        observers: 0,
        assignments,
    }
}

/// Spread distribution: the squad splits across distinct domains by slicing
/// the available roster positionally — position i takes the i-th ordinal
/// domain, alone, under sole ownership. No domain ever gets two owners and
/// no two lanes share context.
#[tracing::instrument]
fn spread_shape(available: &[PersonnelId]) -> PersonnelShape {
    let n = u16::try_from(available.len()).unwrap_or(u16::MAX);
    let assignments = available
        .iter()
        .enumerate()
        .map(|(i, person)| DomainAssignment {
            domain: domain_ordinal(i),
            personnel: vec![person.clone()],
            mode: OwnershipMode::Sole,
        })
        .collect();
    PersonnelShape {
        primary_executors: n,
        support: 0,
        reserves: 0,
        observers: 0,
        assignments,
    }
}

/// Bunch cooperation: the whole squad concentrates on one shared domain
/// under joint ownership with deterministic switching — sole only when one
/// person is available. Full huddle access is declared in the directive.
#[tracing::instrument]
fn bunch_shape(available: &[PersonnelId], domain: String) -> PersonnelShape {
    let n = u16::try_from(available.len()).unwrap_or(u16::MAX);
    PersonnelShape {
        primary_executors: n,
        support: 0,
        reserves: 0,
        observers: 0,
        assignments: vec![DomainAssignment {
            domain,
            personnel: available.to_vec(),
            mode: if n >= 2 {
                OwnershipMode::Joint
            } else {
                OwnershipMode::Sole
            },
        }],
    }
}

/// The balanced default: up to two executors on separate sole-ownership
/// domains, at most one reserve, remainder as unassigned support. Support
/// and reserves deploy without a domain — they are not on the field until
/// called.
#[tracing::instrument]
fn balanced_shape(available: &[PersonnelId]) -> PersonnelShape {
    let n = u16::try_from(available.len()).unwrap_or(u16::MAX);
    let executors = n.min(2);
    let exec_end = usize::from(executors);
    let remaining = n.saturating_sub(executors);
    let reserves = remaining.min(1);
    let support = remaining.saturating_sub(reserves);
    let assignments = available[..exec_end]
        .iter()
        .enumerate()
        .map(|(i, person)| DomainAssignment {
            domain: domain_ordinal(i),
            personnel: vec![person.clone()],
            mode: OwnershipMode::Sole,
        })
        .collect();
    PersonnelShape {
        primary_executors: executors,
        support,
        reserves,
        observers: 0,
        assignments,
    }
}

/// The concentration domain: the caller's hint steers when given, else the
/// named critical domain, else the fallback.
#[tracing::instrument]
fn resolve_domain(hint: Option<&str>, context: &TacticalContext, fallback: &str) -> String {
    hint.map(str::to_owned)
        .or_else(|| context.critical_domain.clone())
        .unwrap_or_else(|| fallback.to_owned())
}

/// The ordinal domain name for distribution position `i`, falling back to
/// `domain-N` past the named table.
#[tracing::instrument]
fn domain_ordinal(i: usize) -> String {
    match DOMAIN_ORDINALS.get(i) {
        Some(&name) => name.to_owned(),
        None => format!("domain-{}", i.saturating_add(1)),
    }
}

/// The doctrine economics of one formation: its deterministic trigger and
/// its admission claim. Private; rendered into the directive by
/// [`formation_directive`].
struct Doctrine {
    trigger: TacticalTrigger,
    benefit_ppm: u32,
    cost_ppm: u32,
    risk_ppm: u32,
    effect: ExpectedEffect,
    counter: Option<&'static str>,
}

/// The doctrine table's trigger and economics row. Every claim is measured
/// in ppm; every net value is strictly positive so an admitted formation
/// can clear a zero threshold.
#[tracing::instrument]
fn doctrine(formation: TacticalFormation) -> Doctrine {
    match formation {
        TacticalFormation::Spread => Doctrine {
            trigger: TacticalTrigger::All(vec![
                TacticalTrigger::ProblemBreadthAtLeast { ppm: 400 },
                TacticalTrigger::DomainCouplingAtMost {
                    ppm: SPREAD_COUPLING_MAX_PPM,
                },
            ]),
            benefit_ppm: 700,
            cost_ppm: 300,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 800,
                protection_delta_ppm: 0,
                tempo_delta_ppm: 0,
                information_delta_ppm: 200,
            },
            counter: None,
        },
        TacticalFormation::Heavy => Doctrine {
            trigger: TacticalTrigger::Any(vec![
                TacticalTrigger::RiskBandAtLeast {
                    band: RiskBand::Elevated,
                },
                TacticalTrigger::ShortYtg,
            ]),
            benefit_ppm: 700,
            cost_ppm: 350,
            risk_ppm: 150,
            effect: ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 800,
                tempo_delta_ppm: 0,
                information_delta_ppm: 0,
            },
            counter: None,
        },
        TacticalFormation::Trips => Doctrine {
            trigger: TacticalTrigger::CriticalDomainNamed,
            benefit_ppm: 600,
            cost_ppm: 300,
            risk_ppm: 150,
            effect: ExpectedEffect {
                breadth_delta_ppm: 200,
                protection_delta_ppm: 300,
                tempo_delta_ppm: 0,
                information_delta_ppm: 400,
            },
            counter: None,
        },
        TacticalFormation::Bunch => Doctrine {
            trigger: TacticalTrigger::DivergenceObserved,
            benefit_ppm: 550,
            cost_ppm: 300,
            risk_ppm: 150,
            effect: ExpectedEffect {
                breadth_delta_ppm: 100,
                protection_delta_ppm: 100,
                tempo_delta_ppm: 0,
                information_delta_ppm: 700,
            },
            counter: None,
        },
        TacticalFormation::Empty => Doctrine {
            trigger: TacticalTrigger::All(vec![
                TacticalTrigger::ProblemBreadthAtLeast { ppm: 700 },
                TacticalTrigger::RiskBandAtLeast {
                    band: RiskBand::Low,
                },
                TacticalTrigger::Not(Box::new(TacticalTrigger::ShortYtg)),
            ]),
            benefit_ppm: 600,
            cost_ppm: 200,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 500,
                tempo_delta_ppm: 400,
                information_delta_ppm: 0,
            },
            counter: Some("max_protect"),
        },
        TacticalFormation::Nickel => Doctrine {
            trigger: TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Guarded,
            },
            benefit_ppm: 450,
            cost_ppm: 250,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 100,
                protection_delta_ppm: 300,
                tempo_delta_ppm: 0,
                information_delta_ppm: 300,
            },
            counter: None,
        },
        TacticalFormation::Dime => Doctrine {
            trigger: TacticalTrigger::RiskBandAtLeast {
                band: RiskBand::Elevated,
            },
            benefit_ppm: 500,
            cost_ppm: 300,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 400,
                tempo_delta_ppm: 0,
                information_delta_ppm: 400,
            },
            counter: None,
        },
        TacticalFormation::Bear => Doctrine {
            trigger: TacticalTrigger::DomainCouplingAtLeast { ppm: 700 },
            benefit_ppm: 550,
            cost_ppm: 300,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 700,
                tempo_delta_ppm: 0,
                information_delta_ppm: 100,
            },
            counter: None,
        },
        TacticalFormation::TwoHigh => Doctrine {
            trigger: TacticalTrigger::ProviderInstabilityAbove { ppm: 300 },
            benefit_ppm: 500,
            cost_ppm: 300,
            risk_ppm: 100,
            effect: ExpectedEffect {
                breadth_delta_ppm: 0,
                protection_delta_ppm: 600,
                tempo_delta_ppm: 0,
                information_delta_ppm: 200,
            },
            counter: None,
        },
    }
}

/// The base play count of one formation before short-YTG modulation.
#[tracing::instrument]
fn base_plays(formation: TacticalFormation) -> u16 {
    match formation {
        TacticalFormation::Spread => 4,
        TacticalFormation::Heavy | TacticalFormation::Empty | TacticalFormation::Bear => 2,
        TacticalFormation::Trips
        | TacticalFormation::Bunch
        | TacticalFormation::Nickel
        | TacticalFormation::Dime
        | TacticalFormation::TwoHigh => 3,
    }
}

/// Build the full tactical directive for a named formation: trigger and
/// economics from the doctrine table, information and concurrency policies
/// from the doctrine profile, budgets and termination modulated
/// deterministically by the measured context (short-YTG halves the play
/// count; a tight remaining budget halves the token ceiling). The result
/// passes [`crate::admission::validated`].
pub #[tracing::instrument]
fn formation_directive(
    formation: TacticalFormation,
    context: &TacticalContext,
) -> TacticalDirective {
    let row = doctrine(formation);
    let profile = profile(formation);
    // Overlapping context is the overlap policy made concurrency: formations
    // whose personnel share context may also share domains.
    let allow_domain_overlap = profile.context_overlap_ppm >= 400;
    // Spread keeps its lanes independent: a replacement does not inherit the
    // failed lane's residue. Every other formation discloses.
    let disclose_failures_to_replacement = !matches!(formation, TacticalFormation::Spread);
    let max_plays = if context.short_ytg {
        (base_plays(formation) / 2).max(1)
    } else {
        base_plays(formation)
    };
    let max_tokens = if context.budget_remaining_ppm <= TIGHT_BUDGET_PPM {
        DIRECTIVE_MAX_TOKENS / 2
    } else {
        DIRECTIVE_MAX_TOKENS
    };
    TacticalDirective {
        id: TacticalId::new(format!("formation-{}", formation.machine_name()))
            .expect("static formation ids are valid"),
        name: formation.machine_name().to_owned(),
        class: TacticalClass::Formation,
        trigger: row.trigger,
        benefit_ppm: row.benefit_ppm,
        cost_ppm: row.cost_ppm,
        risk_ppm: row.risk_ppm,
        shape: canonical_shape(formation),
        information: InformationPolicy {
            max_context_overlap_ppm: profile.context_overlap_ppm,
            huddle_access: profile.huddle_access,
            disclose_failures_to_replacement,
        },
        concurrency: ConcurrencyPolicy {
            max_concurrent: profile.max_concurrent,
            allow_domain_overlap,
        },
        authority: AuthorityPolicy {
            may_reassign: true,
            may_reallocate_budget: false,
            may_change_information_topology: true,
            observe_only: false,
        },
        budget: TacticalBudget {
            max_tokens,
            max_cost_microunits: 2_000_000,
            max_duration_ms: 60_000,
            max_plays,
        },
        expected_effect: row.effect,
        termination: TerminationCondition {
            max_plays,
            abort_when: vec![
                AbortTrigger::BudgetExhausted,
                AbortTrigger::RepeatedFailureOfThisTactic,
            ],
        },
        counter_tactic: row.counter.map(str::to_owned),
    }
}

/// The canonical deployment a formation directive carries: the doctrine
/// shape rendered with placeholder personnel. `plan` produces the same
/// counts and ownership modes when the real squad matches the canonical
/// size.
#[tracing::instrument]
fn canonical_shape(formation: TacticalFormation) -> PersonnelShape {
    #[tracing::instrument]
fn owner(id: &str) -> PersonnelId {
        PersonnelId::new(id).expect("static canonical ids are valid")
    }
    #[tracing::instrument]
fn sole(domain: &str, id: &str) -> DomainAssignment {
        DomainAssignment {
            domain: domain.to_owned(),
            personnel: vec![owner(id)],
            mode: OwnershipMode::Sole,
        }
    }
    #[tracing::instrument]
fn group(domain: &str, ids: &[&str], mode: OwnershipMode) -> DomainAssignment {
        DomainAssignment {
            domain: domain.to_owned(),
            personnel: ids.iter().map(|id| owner(id)).collect(),
            mode,
        }
    }
    match formation {
        TacticalFormation::Spread => PersonnelShape {
            primary_executors: 3,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![
                sole("primary", "spread-1"),
                sole("secondary", "spread-2"),
                sole("tertiary", "spread-3"),
            ],
        },
        TacticalFormation::Heavy => PersonnelShape {
            primary_executors: 1,
            support: 2,
            reserves: 1,
            observers: 0,
            assignments: vec![sole("primary", "heavy-1")],
        },
        TacticalFormation::Trips => PersonnelShape {
            primary_executors: 3,
            support: 1,
            reserves: 0,
            observers: 0,
            assignments: vec![
                group(
                    "primary",
                    &["trips-1", "trips-2", "trips-3"],
                    OwnershipMode::Shared,
                ),
                sole("global", "trips-4"),
            ],
        },
        TacticalFormation::Bunch => PersonnelShape {
            primary_executors: 3,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![group(
                "primary",
                &["bunch-1", "bunch-2", "bunch-3"],
                OwnershipMode::Joint,
            )],
        },
        TacticalFormation::Empty => PersonnelShape {
            primary_executors: 5,
            support: 0,
            reserves: 0,
            observers: 0,
            assignments: vec![group(
                "primary",
                &["empty-1", "empty-2", "empty-3", "empty-4", "empty-5"],
                OwnershipMode::Shared,
            )],
        },
        TacticalFormation::Nickel => PersonnelShape {
            primary_executors: 4,
            support: 1,
            reserves: 0,
            observers: 0,
            assignments: vec![
                group("primary", &["nickel-1", "nickel-2"], OwnershipMode::Shared),
                group(
                    "secondary",
                    &["nickel-3", "nickel-4"],
                    OwnershipMode::Shared,
                ),
                sole("review", "nickel-review"),
            ],
        },
        TacticalFormation::Dime => PersonnelShape {
            primary_executors: 3,
            support: 3,
            reserves: 0,
            observers: 0,
            assignments: vec![
                group("primary", &["dime-1", "dime-2"], OwnershipMode::Shared),
                group("secondary", &["dime-3"], OwnershipMode::Sole),
                sole("review-a", "dime-review-a"),
                sole("review-b", "dime-review-b"),
                sole("review-c", "dime-review-c"),
            ],
        },
        TacticalFormation::Bear => PersonnelShape {
            primary_executors: 1,
            support: 3,
            reserves: 0,
            observers: 0,
            assignments: vec![group(
                "core",
                &["bear-1", "bear-2", "bear-3", "bear-4"],
                OwnershipMode::Shared,
            )],
        },
        TacticalFormation::TwoHigh => PersonnelShape {
            primary_executors: 2,
            support: 0,
            reserves: 2,
            observers: 0,
            assignments: vec![
                sole("primary", "two-high-1"),
                sole("secondary", "two-high-2"),
            ],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admission::{admit, validated, Admission};

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

    #[tracing::instrument]
fn squad(n: usize) -> Vec<PersonnelId> {
        (0..n)
            .map(|i| PersonnelId::new(format!("p{i}")).expect("squad ids are valid"))
            .collect()
    }

    #[tracing::instrument]
fn firing_context(formation: TacticalFormation) -> TacticalContext {
        let mut context = context();
        match formation {
            TacticalFormation::Spread => context.problem_breadth_ppm = 700,
            TacticalFormation::Heavy => context.risk_band = RiskBand::Elevated,
            TacticalFormation::Trips => context.critical_domain = Some("parser".into()),
            TacticalFormation::Bunch => context.divergence_observed = true,
            TacticalFormation::Empty => context.problem_breadth_ppm = 800,
            TacticalFormation::Nickel => context.risk_band = RiskBand::Guarded,
            TacticalFormation::Dime => context.risk_band = RiskBand::Elevated,
            TacticalFormation::Bear => context.domain_coupling_ppm = 900,
            TacticalFormation::TwoHigh => context.provider_instability_ppm = 500,
        }
        context
    }

    #[test]
    #[tracing::instrument]
fn the_doctrine_table_carries_the_declared_economics() {
        assert_eq!(profile(TacticalFormation::Empty).reserves_share_ppm, 0);
        assert!(
            profile(TacticalFormation::Dime).specialist_share_ppm
                > profile(TacticalFormation::Nickel).specialist_share_ppm,
            "dime must be denser in specialists than nickel"
        );
        assert!(profile(TacticalFormation::TwoHigh).reserves_share_ppm >= 400);
        assert!(profile(TacticalFormation::Bunch).context_overlap_ppm >= 800);
        assert!(profile(TacticalFormation::Spread).context_overlap_ppm <= 100);
        assert_eq!(profile(TacticalFormation::Bear).specialist_share_ppm, 1_000);
        assert!(
            profile(TacticalFormation::Heavy).specialist_share_ppm
                > profile(TacticalFormation::Spread).specialist_share_ppm,
            "heavy is support-heavy"
        );
        // Machine names match the serde representation.
        assert_eq!(TacticalFormation::TwoHigh.machine_name(), "two_high");
        assert_eq!(
            serde_json::to_string(&TacticalFormation::TwoHigh)
                .expect("serialisable")
                .as_str(),
            "\"two_high\""
        );
    }

    #[test]
    #[tracing::instrument]
fn broad_loose_problems_ladder_to_spread() {
        let mut ctx = context();
        ctx.problem_breadth_ppm = 800;
        ctx.domain_coupling_ppm = 200;
        let shape = plan(&ctx, &squad(4), None).expect("planned");
        shape.validate().expect("valid");
        assert_eq!(shape.deployed(), 4);
        assert_eq!(shape.assignments.len(), 4);
        for (i, assignment) in shape.assignments.iter().enumerate() {
            assert_eq!(assignment.mode, OwnershipMode::Sole);
            assert_eq!(assignment.personnel.len(), 1);
            assert_eq!(assignment.domain, domain_ordinal(i));
            for other in &shape.assignments[i + 1..] {
                assert_ne!(assignment.domain, other.domain, "no domain has two owners");
            }
        }
    }

    #[test]
    #[tracing::instrument]
fn a_critical_domain_ladders_to_trips_and_agrees_with_the_directive() {
        let mut ctx = context();
        ctx.critical_domain = Some("parser".into());
        let shape = plan(&ctx, &squad(4), None).expect("planned");
        assert_eq!(shape.primary_executors, 3);
        assert_eq!(shape.support, 1);
        assert_eq!(shape.assignments.len(), 2);
        assert_eq!(shape.assignments[0].domain, "parser");
        assert_eq!(shape.assignments[0].personnel.len(), 3);
        assert_eq!(shape.assignments[0].mode, OwnershipMode::Shared);
        assert_eq!(shape.assignments[1].domain, "global");
        assert_eq!(shape.assignments[1].mode, OwnershipMode::Sole);
        // The planner's shape and the directive's canonical shape agree on
        // counts and ownership modes for the canonical squad.
        let directive = formation_directive(TacticalFormation::Trips, &ctx);
        let canonical = directive.shape;
        assert_eq!(canonical.primary_executors, shape.primary_executors);
        assert_eq!(canonical.support, shape.support);
        assert_eq!(canonical.assignments.len(), shape.assignments.len());
        for (declared, planned) in canonical.assignments.iter().zip(&shape.assignments) {
            assert_eq!(declared.mode, planned.mode);
        }
    }

    #[test]
    #[tracing::instrument]
fn divergence_ladders_to_bunch() {
        let mut ctx = context();
        ctx.divergence_observed = true;
        let shape = plan(&ctx, &squad(3), None).expect("planned");
        shape.validate().expect("valid");
        assert_eq!(shape.deployed(), 3);
        assert_eq!(shape.primary_executors, 3);
        assert_eq!(shape.assignments.len(), 1);
        assert_eq!(shape.assignments[0].mode, OwnershipMode::Joint);
        assert_eq!(shape.assignments[0].personnel.len(), 3);
        // One person cannot jointly own a domain; the planner degrades to
        // sole ownership instead of producing an invalid shape.
        let single = plan(&ctx, &squad(1), None).expect("planned");
        assert_eq!(single.assignments[0].mode, OwnershipMode::Sole);
    }

    #[test]
    #[tracing::instrument]
fn critical_risk_and_instability_ladder_to_heavy() {
        let mut ctx = context();
        ctx.risk_band = RiskBand::Critical;
        let shape = plan(&ctx, &squad(5), Some("migration")).expect("planned");
        assert_eq!(shape.primary_executors, 1);
        assert_eq!(shape.support, 2);
        assert_eq!(shape.reserves, 2);
        assert_eq!(shape.deployed(), 5);
        assert_eq!(shape.assignments.len(), 1);
        assert_eq!(shape.assignments[0].domain, "migration");
        // Reserves hold no domain; executor plus support do.
        assert_eq!(shape.assignments[0].personnel.len(), 3);
        // High provider instability takes the same branch without risk.
        let mut unstable = context();
        unstable.provider_instability_ppm = HEAVY_INSTABILITY_PPM + 1;
        let shape = plan(&unstable, &squad(4), None).expect("planned");
        assert_eq!(shape.primary_executors, 1);
        assert_eq!(shape.reserves + shape.support, 3);
    }

    #[test]
    #[tracing::instrument]
fn the_balanced_default_spreads_two_and_holds_one() {
        let shape = plan(&context(), &squad(3), None).expect("planned");
        assert_eq!(shape.primary_executors, 2);
        assert_eq!(shape.reserves, 1);
        assert_eq!(shape.assignments.len(), 2);
        assert_ne!(
            shape.assignments[0].domain, shape.assignments[1].domain,
            "the two executors run on separate domains"
        );
        let single = plan(&context(), &squad(1), None).expect("planned");
        assert_eq!(single.primary_executors, 1);
        assert_eq!(single.reserves, 0);
        single.validate().expect("valid");
    }

    #[test]
    #[tracing::instrument]
fn the_planner_never_invents_personnel_and_refuses_an_empty_squad() {
        let mut heavy = context();
        heavy.risk_band = RiskBand::Critical;
        let mut trips = context();
        trips.critical_domain = Some("parser".into());
        let mut spread = context();
        spread.problem_breadth_ppm = 800;
        let mut bunch = context();
        bunch.divergence_observed = true;
        let contexts = [heavy, trips, spread, bunch, context()];
        for context in &contexts {
            for n in 0..=6usize {
                if n == 0 {
                    assert_eq!(
                        plan(context, &squad(n), None),
                        Err(tactics_refusal("no personnel available"))
                    );
                    continue;
                }
                let shape = plan(context, &squad(n), None).expect("planned");
                assert!(
                    shape.deployed() <= u16::try_from(n).expect("small n"),
                    "planner invented personnel"
                );
                shape.validate().expect("planner output validates");
            }
        }
    }

    #[test]
    #[tracing::instrument]
fn every_formation_directive_validates_and_admits_when_its_trigger_fires() {
        let formations = [
            TacticalFormation::Spread,
            TacticalFormation::Heavy,
            TacticalFormation::Trips,
            TacticalFormation::Bunch,
            TacticalFormation::Empty,
            TacticalFormation::Nickel,
            TacticalFormation::Dime,
            TacticalFormation::Bear,
            TacticalFormation::TwoHigh,
        ];
        for formation in formations {
            let directive = formation_directive(formation, &firing_context(formation));
            validated(&directive).expect("directive validates");
            assert_eq!(
                admit(&directive, &firing_context(formation), 0),
                Admission::Admitted,
                "{} admits on its own trigger",
                formation.machine_name()
            );
        }
        // Empty's trigger carries Not(ShortYtg): the all-in formation never
        // fires once the two-minute drill starts.
        let mut hurry_up = context();
        hurry_up.problem_breadth_ppm = 800;
        hurry_up.short_ytg = true;
        assert!(!formation_directive(TacticalFormation::Empty, &hurry_up)
            .trigger
            .evaluate(&hurry_up));
        // And the declared counter of Empty is protection.
        assert_eq!(
            formation_directive(TacticalFormation::Empty, &context()).counter_tactic,
            Some("max_protect".into())
        );
    }

    #[test]
    #[tracing::instrument]
fn planning_is_deterministic() {
        let mut spread = context();
        spread.problem_breadth_ppm = 800;
        spread.domain_coupling_ppm = 200;
        let mut heavy = context();
        heavy.risk_band = RiskBand::Critical;
        let squad = squad(5);
        for context in [&spread, &heavy] {
            assert_eq!(
                plan(context, &squad, None).expect("planned"),
                plan(context, &squad, None).expect("planned"),
                "the same inputs always deploy the same shape"
            );
            assert_eq!(
                formation_directive(TacticalFormation::Trips, context),
                formation_directive(TacticalFormation::Trips, context),
            );
        }
        assert_eq!(
            profile(TacticalFormation::Bear),
            profile(TacticalFormation::Bear)
        );
    }
}
