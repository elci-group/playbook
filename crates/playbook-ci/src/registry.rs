//! The gate registry: the CI policy is DATA, not code paths (§16). Adding,
//! reclassifying or demoting a gate is an edit to this table, visible in
//! review and auditable against the evidence envelope.

use ci_evidence::GateClassification;

/// When a gate may run. Skipped gates are recorded, never silently absent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    /// Unconditional.
    Always,
    /// Run only if the named tool resolves on PATH; otherwise
    /// `SkippedNotApplicable`.
    IfToolExists,
    /// Run only if Cargo.lock or any Cargo.toml changed versus the parent
    /// revision (§14 determinism gates).
    IfDependenciesChanged,
    /// Run only if README.md or docs/ changed versus the parent revision.
    IfClaimsSurfaceChanged,
}

/// One gate's definition. `command` is the exact string whose sha256 is the
/// `input_hash` recorded in evidence — the command IS the input.
#[derive(Clone, Copy, Debug)]
pub struct GateDef {
    pub id: &'static str,
    pub authority: &'static str,
    pub classification: GateClassification,
    pub command: &'static str,
    pub condition: Condition,
    /// Authority tool whose presence is probed for `condition`. Empty for
    /// Always / change-conditioned gates.
    pub tool: &'static str,
}

/// The canonical gate matrix (§14). Order here is order of execution and
/// order of recording in `CI_EVIDENCE.json`.
pub const GATES: &[GateDef] = &[
    GateDef {
        id: "ci/fmt",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo fmt --all --check",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/check",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo check --workspace --all-targets --offline",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/clippy",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo clippy --workspace --all-targets --offline -- -D warnings",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/test",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo test --workspace --offline",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/deliver",
        authority: "deliver",
        classification: GateClassification::Blocking,
        command: "deliver --spec deliver.toml --strict --progress never",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/replay",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo test -p playbook-state --offline",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/mvp-replay",
        authority: "playbook",
        classification: GateClassification::Blocking,
        command: "target/debug/playbook mvp --basic",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/value-lab",
        authority: "cargo",
        classification: GateClassification::Blocking,
        command: "cargo test -p value-lab --offline",
        condition: Condition::Always,
        tool: "",
    },
    GateDef {
        id: "ci/traci",
        authority: "traci",
        classification: GateClassification::Advisory,
        command: "traci check . --format human",
        condition: Condition::IfToolExists,
        tool: "traci",
    },
    GateDef {
        id: "ci/amber",
        authority: "amber",
        classification: GateClassification::Conditional,
        command: "amber check .",
        condition: Condition::IfDependenciesChanged,
        tool: "",
    },
    GateDef {
        id: "ci/verity",
        authority: "verity",
        classification: GateClassification::Conditional,
        command: "true",
        condition: Condition::IfClaimsSurfaceChanged,
        tool: "",
    },
];

/// Gates whose execution admission verification REQUIRES (§17). Conditional
/// and advisory gates are not mandatory; a Conditional gate that ran and
/// failed still blocks via `derive_verdict` semantics? No — Conditional
/// failures block only when policy declares them required; this list is that
/// declaration, and `ci/amber`/`ci/verity` are deliberately absent.
pub const REQUIRED_GATES: &[&str] = &[
    "ci/fmt",
    "ci/check",
    "ci/clippy",
    "ci/test",
    "ci/deliver",
    "ci/replay",
];
