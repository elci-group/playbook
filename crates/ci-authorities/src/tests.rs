//! Runner-level unit tests: exit-code mapping, spawn-failure handling,
//! timeout fail-closed behaviour, and input hashing.

use std::time::Duration;

use ci_evidence::GateOutcome;

use crate::runner::{input_hash, run_gate};
use crate::GateSpec;

fn spec() -> GateSpec {
    GateSpec::new(
        "ci/test",
        "test-authority",
        ci_evidence::GateClassification::Advisory,
    )
}

#[test]
fn exit_zero_maps_to_pass() {
    let record = run_gate(&spec(), "p", "digest", "true", &[], Duration::from_secs(10));
    assert_eq!(record.outcome, GateOutcome::Pass);
    assert_eq!(record.exit_code, Some(0));
    assert!(record.outcome.satisfies());
    assert!(record.outcome.executed());
    assert!(record.duration_ms <= 10_000);
}

#[test]
fn exit_nonzero_maps_to_fail() {
    let record = run_gate(
        &spec(),
        "p",
        "digest",
        "false",
        &[],
        Duration::from_secs(10),
    );
    assert_eq!(record.outcome, GateOutcome::Fail);
    assert_eq!(record.exit_code, Some(1));
    assert!(!record.outcome.satisfies());
    assert!(record.outcome.executed());
}

#[test]
fn spawn_failure_maps_to_error_fail_closed() {
    let record = run_gate(
        &spec(),
        "p",
        "digest",
        "definitely-not-a-binary-xyz",
        &[],
        Duration::from_secs(10),
    );
    assert_eq!(record.outcome, GateOutcome::Error);
    assert!(record.exit_code.is_none());
    assert!(!record.outcome.satisfies());
    assert!(record.findings.iter().any(|f| f.code == "spawn_failure"));
}

#[test]
fn timeout_maps_to_error_fail_closed() {
    let record = run_gate(
        &spec(),
        "p",
        "digest",
        "sleep",
        &["5"],
        Duration::from_millis(300),
    );
    assert_eq!(record.outcome, GateOutcome::Error);
    assert!(record.exit_code.is_none());
    assert!(record.findings.iter().any(|f| f.code == "timeout"));
}

#[test]
fn record_carries_spec_identity() {
    let record = run_gate(
        &spec(),
        "policy/9",
        "digest",
        "true",
        &[],
        Duration::from_secs(10),
    );
    assert_eq!(record.gate_id, "ci/test");
    assert_eq!(record.authority, "test-authority");
    assert_eq!(record.policy_version, "policy/9");
    assert_eq!(
        record.classification,
        ci_evidence::GateClassification::Advisory
    );
    assert!(record.artifact_hashes.is_empty());
}

#[test]
fn input_hash_is_deterministic_and_distinguishes_inputs() {
    let a = input_hash("d", "prog", &["--x", "y"]);
    let a_again = input_hash("d", "prog", &["--x", "y"]);
    let b = input_hash("d", "prog", &["--xy"]);
    let c = input_hash("d2", "prog", &["--x", "y"]);
    assert_eq!(a, a_again);
    assert_ne!(a, b, "joined args must not collide with concatenations");
    assert_ne!(a, c);
}
