//! The CI_EVIDENCE/1 campaign: the verdict is mechanical, failure
//! semantics are closed, waivers are explicit and expiring, and evidence
//! from one revision never authorises another (§16, §29, §30).

use ci_evidence::{
    verify_admission, CiEvidence, GateClassification, GateOutcome, GateRecord, RevisionFreeze,
    Verdict,
};

const NOW: u64 = 1_000_000;

fn revision() -> RevisionFreeze {
    RevisionFreeze {
        commit: "abc123".into(),
        parent: "parent0".into(),
        branch: "feat/ci".into(),
        diff_digest: "diff".into(),
        lock_digest: "lock".into(),
        toolchain: "rustc 1.0".into(),
        policy_version: "v1".into(),
        implementation_version: "0.1.0".into(),
        config_hashes: Default::default(),
        frozen_at_ms: 0,
    }
}

fn gate(id: &str, classification: GateClassification, outcome: GateOutcome) -> GateRecord {
    GateRecord {
        gate_id: id.into(),
        authority: "cargo".into(),
        authority_version: "1".into(),
        policy_version: "v1".into(),
        classification,
        started_at_ms: 0,
        completed_at_ms: 1,
        input_hash: "input".into(),
        outcome,
        exit_code: Some(0),
        findings: Vec::new(),
        artifact_hashes: Vec::new(),
        duration_ms: 1,
    }
}

#[test]
fn a_fully_passing_envelope_is_admitted() {
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    evidence.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    evidence.record(gate(
        "ci/amber",
        GateClassification::Advisory,
        GateOutcome::Fail,
    ));
    // An advisory failure never prevents admission (§4).
    assert_eq!(evidence.derive_verdict(NOW), Verdict::Admitted);
}

#[test]
fn blocking_failure_is_rejected_and_error_never_equals_pass() {
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    evidence.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    assert_eq!(evidence.derive_verdict(NOW), Verdict::Rejected);
    let mut error = CiEvidence::new(revision());
    error.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Error,
    ));
    assert_eq!(
        error.derive_verdict(NOW),
        Verdict::Rejected,
        "ERROR is not PASS"
    );
    let mut inconclusive = CiEvidence::new(revision());
    inconclusive.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Inconclusive,
    ));
    assert_eq!(
        inconclusive.derive_verdict(NOW),
        Verdict::Rejected,
        "INCONCLUSIVE is not PASS"
    );
    // Even a CONDITIONAL gate that errored fails the study closed (§30).
    let mut conditional_error = CiEvidence::new(revision());
    conditional_error.record(gate(
        "ci/amber",
        GateClassification::Conditional,
        GateOutcome::Error,
    ));
    assert_eq!(conditional_error.derive_verdict(NOW), Verdict::Rejected);
}

#[test]
fn a_valid_waiver_rescues_but_an_expired_one_does_not() {
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    evidence.waivers.push(ci_evidence::Waiver {
        gate_id: "ci/test".into(),
        reason: "known flake, tracked as CI-17".into(),
        author: "sal".into(),
        expires_at_ms: Some(NOW + 1_000),
        recorded_at_revision: "abc123".into(),
    });
    assert_eq!(evidence.derive_verdict(NOW), Verdict::Admitted);
    // After expiry the same envelope rejects.
    assert_eq!(evidence.derive_verdict(NOW + 2_000), Verdict::Rejected);
    // A waiver with no reason is not a waiver.
    let mut hollow = CiEvidence::new(revision());
    hollow.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Fail,
    ));
    hollow.waivers.push(ci_evidence::Waiver {
        gate_id: "ci/test".into(),
        reason: "  ".into(),
        author: "sal".into(),
        expires_at_ms: None,
        recorded_at_revision: "abc123".into(),
    });
    assert_eq!(hollow.derive_verdict(NOW), Verdict::Rejected);
}

#[test]
fn evidence_from_another_revision_authorises_nothing() {
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    evidence.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    // The same envelope cannot authorise a different commit (§5).
    let rejected = verify_admission(&evidence, "different-sha", "v1", &["ci/test"], NOW);
    assert!(rejected.is_err());
    // Or a different policy version (§9: threshold games are visible).
    assert!(verify_admission(&evidence, "abc123", "v2", &["ci/test"], NOW).is_err());
    // Or a missing mandatory gate (§17).
    assert!(verify_admission(&evidence, "abc123", "v1", &["ci/test", "ci/clippy"], NOW).is_err());
    // The honest case admits.
    assert_eq!(
        verify_admission(&evidence, "abc123", "v1", &["ci/test"], NOW).unwrap(),
        Verdict::Admitted
    );
}

#[test]
fn skipped_gates_do_not_satisfy_requirements() {
    // A conditional gate skipped as not-applicable is recorded truthfully
    // and does not block…
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    evidence.record(gate(
        "ci/amber",
        GateClassification::Conditional,
        GateOutcome::SkippedNotApplicable,
    ));
    assert_eq!(evidence.derive_verdict(NOW), Verdict::Admitted);
    // …but a BLOCKING gate recorded as skipped cannot satisfy admission:
    // it never executed, and pass-closed means it is not a pass.
    let mut hollow = CiEvidence::new(revision());
    hollow.record(gate(
        "ci/test",
        GateClassification::Blocking,
        GateOutcome::SkippedNotApplicable,
    ));
    assert_eq!(
        verify_admission(&hollow, "abc123", "v1", &["ci/test"], NOW).unwrap_err(),
        "mandatory gate ci/test did not execute"
    );
}

#[test]
fn the_envelope_round_trips_through_json() {
    let mut evidence = CiEvidence::new(revision());
    evidence.record(gate(
        "ci/fmt",
        GateClassification::Blocking,
        GateOutcome::Pass,
    ));
    let text = serde_json::to_string(&evidence).unwrap();
    let parsed: CiEvidence = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed.schema, "CI_EVIDENCE/1");
    assert_eq!(parsed, evidence);
    // The schema identity is load-bearing: an unknown schema is refused on
    // admission.
    let mut wrong = parsed.clone();
    wrong.schema = "CI_EVIDENCE/0".into();
    assert!(verify_admission(&wrong, "abc123", "v1", &["ci/fmt"], NOW).is_err());
}
