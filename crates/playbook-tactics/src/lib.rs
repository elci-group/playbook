//! The tactical layer.
//!
//! Playbook's authority crates decide *which personnel act*. This crate
//! decides the organisational shape those personnel take: what information
//! they receive, what interaction constraints they operate under, and what
//! tactical objective governs the play. It sits between the franchise and
//! the execution authorities — it recommends and structures; the existing
//! authorities execute and validate.
//!
//! The reasoning hierarchy:
//!
//! ```text
//! Front Office   who belongs to the organisation
//! Franchise      who owns the objective
//! Formation      how available personnel are arranged      (this crate)
//! Play           what bounded action they take            (this crate)
//! Adjustment     how that action changes as reality does  (this crate)
//! Execution      the authorities run it
//! Evidence       the field decides what was true
//! Counter        tactical memory shapes the next call     (this crate)
//! ```
//!
//! The frozen contract lives in [`domain`]: every tactic is a
//! [`TacticalDirective`] with a deterministic trigger, a personnel shape, an
//! information topology, a concurrency and authority policy, a bounded
//! budget, an expected effect, a termination condition, and a declared
//! counter. Admission is economics; composition is the conflict matrix.
//! A football name without a distinct, testable, economically meaningful
//! orchestration behaviour does not graduate.

#![forbid(unsafe_code)]

pub mod adjustment;
pub mod admission;
pub mod composition;
pub mod conservative;
pub mod diagnostic;
pub mod domain;
pub mod exchange;
pub mod formation;
pub mod protect;
pub mod sequencing;
pub mod surveillance;

pub use adjustment::{
    apply_shift, call_audible, check_with_me, motion_reveals, plan_motion, AudibleDecision,
    MotionPlan, MotionProbe, ProbeKind, Shift, ShiftMove,
};
pub use admission::{admit, validated, Admission};
pub use composition::{
    class_of, compatibility, compose, counter_of, Compatibility, TacticalPlan, KNOWN_TACTICS,
};
pub use conservative::{
    adjudicate, classify, directive as conservative_directive, drive_stages,
    envelope as change_envelope, ChangeEnvelope, ChangeRequest, ChangeVerdict, RiskState,
};
pub use diagnostic::{
    directive as diagnostic_directive, merge_flood, overlapping_personnel, plan_flood, plan_levels,
    plan_mesh, AbstractionLevel, AttackPattern, CrossingAssignment, DepthAssignment, FloodPlan,
    LevelsPlan, MeshPlan,
};
pub use domain::{
    tactics_refusal, AbortTrigger, DomainAssignment, ExpectedEffect, HuddleAccess,
    InformationPolicy, OwnershipMode, PersonnelShape, RiskBand, TacticalBudget, TacticalClass,
    TacticalContext, TacticalDirective, TacticalOutcome, TacticalTrigger, TerminationCondition,
};
pub use exchange::{
    banjo, directive as exchange_directive, initial as banjo_initial, post_exchange_roles,
    primary_of, reject_stale, scrape_exchange, switch_on, BanjoState, Exchange, ExchangeKind,
    Holder, OwnershipPair, SwitchTrigger,
};
pub use formation::{
    formation_directive, plan as plan_formation, profile as formation_profile, FormationProfile,
    TacticalFormation,
};
pub use protect::{
    directive as protection_directive, generative_breadth, heavy_profile, max_protect_profile,
    protection_score, shape as protection_shape, short_ytg_policy, ConcentrationProfile,
    ProtectionKind,
};
pub use sequencing::{
    constraint_for, detect_overreaction, recommend_next, HistoryRow, Overreaction, TacticalHistory,
};
pub use surveillance::{
    assign_spy, bracket, directive as surveillance_directive, drift_band, is_bracked, Bracket,
    DriftBand, DriftPolicy, DriftReading, Intervention, SpyAssignment, SurveillanceKind,
};
