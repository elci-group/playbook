//! The enterprise integration layer.
//!
//! Six external systems surround the Playbook control plane — Mesut beneath
//! execution, REECE before the snap, Dreamseq after the whistle, Hybreed and
//! Emulsify where divergent work must consolidate, Theosis keeping the
//! tooling fresh — and every one of them is an advisory or delegated
//! surface. Playbook owns the game. None of these systems may silently
//! acquire authority, and the dependency direction is one-way:
//!
//! ```text
//! Playbook authority → state transition → journal
//!                                     └→ integration representation
//! ```
//!
//! never the reverse. Each subsystem here is a contract plus a
//! deterministic decision procedure with an explicit unavailable-fallback:
//! Mesut routes eligible work but cannot change what the work means; REECE
//! reads the ecosystem but cannot call a play; Dreamseq reviews closed
//! drives but cannot reopen them; Hybreed and Emulsify plan consolidations
//! but Kaptaind remains the transaction authority; Theosis classifies
//! capability freshness but never downgrades and never updates mid-play.
//!
//! Journalling is via `playbook-events`; these surfaces journal what they
//! observed without the journal becoming their authority.

#![forbid(unsafe_code)]

pub mod consolidation;
pub mod dreamseq;
pub mod mesut;
pub mod provenance;
pub mod reece;
pub mod theosis;

pub use consolidation::{
    kaptaind_gate, plan_digest, CompatibilityClass, ConsolidationCandidate,
    ConsolidationConstraints, EmulsifyPlan, GateVerdict, HybreedPlan, KaptaindGate, MergeUnit,
    Trait, Transformation,
};
pub use dreamseq::{
    evaluate as dreamseq_evaluate, redact as dreamseq_redact,
    review_package as dreamseq_review_package, DriveReviewResult, InfrastructureOpportunity,
    PersonnelRecommendation, PostDriveReview, ReviewError, SalaryFinding, TacticalFinding,
};
pub use mesut::{
    route, scramble, CancellationContract, ExecutionPressure, MesutExecutionRequest,
    MesutUnavailable, PreservedContract, Priority, ResourceConstraints, RouteDecision,
    SchedulingPreferences, ScrambleDecision, ScrambleReason, Substrate, WorkKind,
};
pub use provenance::{SystemVersion, ToolchainProvenance, KNOWN_SYSTEMS};
pub use reece::{
    admit as reece_admit, fallback_read, field_read, CapabilityEdge, CompressedFieldRead, Conflict,
    EcosystemFact, OwnershipConstraint, ProjectRef, ProjectedImpact, ProposedAction,
    QbConsiderationRecord, QbReeceQuery, ReeceAdmission, ReeceUnavailability,
};
pub use theosis::{
    classify, classify_with_policy, decide as theosis_decide, preflight, update_permitted,
    CompatibilityPolicy as TheosisCompatibilityPolicy, PreflightDecision, PreflightReport,
    ToolVersion, VersionClassification,
};
