//! §33 — the adversarial campaign against admission. Every attack an
//! eager liar will try against the evidence gate is constructed here in
//! Rust (no git, no subprocesses) and demonstrated to FAIL, and the two
//! honest envelopes are demonstrated to PASS. `ci_evidence` types are used
//! directly: the judge's semantics are the subject under test.

use ci_evidence::{
    CiEvidence, Finding, GateClassification, GateOutcome, GateRecord, RevisionFreeze, Verdict,
    Waiver,
};

const POLICY: &str = "smoke-policy";
const COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const NOW_MS: u64 = 1_000_000;

fn freeze(commit: &str) -> RevisionFreeze {
    RevisionFreeze {
        commit: commit.to_string(),
        parent: "0000000000000000000000000000000000000000".to_string(),
        branch: "feat/franchise-front-office".to_string(),
        diff_digest: "diff".to_string(),
        lock_digest: "lock".to_string(),
        toolchain: "rustc 1.0".to_string(),
        policy_version: POLICY.to_string(),
        implementation_version: "0.1.0".to_string(),
        config_hashes: Default::default(),
        frozen_at_ms: NOW_MS - 1_000,
    }
}

fn gate(id: &str, classification: GateClassification, outcome: GateOutcome) -> GateRecord {
    GateRecord {
        gate_id: id.to_string(),
        authority: "cargo".to_string(),
        authority_version: "unversioned".to_string(),
        policy_version: POLICY.to_string(),
        classification,
        started_at_ms: NOW_MS - 10,
        completed_at_ms: NOW_MS,
        input_hash: "input".to_string(),
        outcome,
        exit_code: match outcome {
            GateOutcome::Pass => Some(0),
            GateOutcome::Fail => Some(1),
            _ => None,
        },
        findings: match outcome {
            GateOutcome::Pass => Vec::new(),
            _ => vec![Finding {
                code: "test".to_string(),
                severity: "error".to_string(),
                message: "constructed failure".to_string(),
            }],
        },
        artifact_hashes: Vec::new(),
        duration_ms: 10,
    }
}

fn waiver(gate_id: &str, expires_at_ms: Option<u64>) -> Waiver {
    Waiver {
        gate_id: gate_id.to_string(),
        reason: "known-good, tracked upstream".to_string(),
        author: "kaptain".to_string(),
        expires_at_ms,
        recorded_at_revision: COMMIT.to_string(),
    }
}

/// All six mandatory gates passing, for envelopes that vary in one detail.
fn passing_envelope(commit: &str) -> CiEvidence {
    let mut ev = CiEvidence::new(freeze(commit));
    for id in [
        "ci/fmt",
        "ci/check",
        "ci/clippy",
        "ci/test",
        "ci/deliver",
        "ci/replay",
    ] {
        ev.record(gate(id, GateClassification::Blocking, GateOutcome::Pass));
    }
    ev
}

fn verify(ev: &CiEvidence, commit: &str) -> Result<Verdict, String> {
    ci_evidence::verify_admission(
        ev,
        commit,
        POLICY,
        &[
            "ci/fmt",
            "ci/check",
            "ci/clippy",
            "ci/test",
            "ci/deliver",
            "ci/replay",
        ],
        NOW_MS,
    )
}

#[test]
fn blocking_fail_is_rejected() {
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/adversarial",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Rejected));
}

#[test]
fn blocking_error_is_rejected() {
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/adversarial",
        GateClassification::Blocking,
        GateOutcome::Error,
    ));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Rejected));
}

#[test]
fn blocking_inconclusive_is_rejected() {
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/adversarial",
        GateClassification::Blocking,
        GateOutcome::Inconclusive,
    ));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Rejected));
}

#[test]
fn evidence_from_wrong_commit_rejects() {
    // The envelope is honest and complete — but it was minted against a
    // different revision. Evidence for A never authorises B.
    let ev = passing_envelope(OTHER_COMMIT);
    assert!(verify(&ev, COMMIT).is_err());
}

#[test]
fn missing_mandatory_gate_rejects() {
    // ci/deliver simply never ran. Nothing failed; the absence itself is
    // the rejection (§30: fail closed).
    let mut ev = CiEvidence::new(freeze(COMMIT));
    for id in ["ci/fmt", "ci/check", "ci/clippy", "ci/test", "ci/replay"] {
        ev.record(gate(id, GateClassification::Blocking, GateOutcome::Pass));
    }
    let err = verify(&ev, COMMIT).expect_err("missing mandatory gate must reject");
    assert!(
        err.contains("ci/deliver"),
        "reason names the absent gate: {err}"
    );
}

#[test]
fn expired_waiver_does_not_rescue_fail() {
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    ev.waivers.push(waiver("ci/fmt", Some(NOW_MS - 1)));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Rejected));
}

#[test]
fn unexpired_waiver_rescues_fail() {
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    ev.waivers.push(waiver("ci/fmt", Some(NOW_MS + 60_000)));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Admitted));
}

#[test]
fn fully_passing_envelope_is_admitted() {
    let ev = passing_envelope(COMMIT);
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Admitted));
}

#[test]
fn advisory_failure_never_blocks() {
    // The whole point of the federation: traci is one voice among many.
    let mut ev = passing_envelope(COMMIT);
    ev.record(gate(
        "ci/traci",
        GateClassification::Advisory,
        GateOutcome::Fail,
    ));
    assert_eq!(verify(&ev, COMMIT), Ok(Verdict::Admitted));
}

#[test]
fn skips_are_not_execution() {
    // A gate recorded as skipped does not satisfy the mandatory-gate
    // presence check: skipping a mandatory gate is rejection, not a dodge.
    let mut ev = CiEvidence::new(freeze(COMMIT));
    for id in ["ci/fmt", "ci/check", "ci/clippy", "ci/test", "ci/replay"] {
        ev.record(gate(id, GateClassification::Blocking, GateOutcome::Pass));
    }
    ev.record(gate(
        "ci/deliver",
        GateClassification::Blocking,
        GateOutcome::SkippedNotApplicable,
    ));
    assert!(verify(&ev, COMMIT).is_err());
}
