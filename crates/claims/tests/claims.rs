//! §33-style adversarial tests for the CLAIMS/1 admission registry.

use ci_evidence::{
    CiEvidence, Finding, GateClassification, GateOutcome, GateRecord, RevisionFreeze, Verdict,
    Waiver,
};
use claims::{seal, verify, verify_for, AdmissionClaim, ClaimRegistry};

fn revision(commit: &str, policy: &str) -> RevisionFreeze {
    RevisionFreeze {
        commit: commit.to_string(),
        parent: "0".repeat(40),
        branch: "main".to_string(),
        diff_digest: "d".repeat(64),
        lock_digest: "l".repeat(64),
        toolchain: "stable".to_string(),
        policy_version: policy.to_string(),
        implementation_version: "impl/1".to_string(),
        config_hashes: Default::default(),
        frozen_at_ms: 1_000,
    }
}

fn gate(id: &str, outcome: GateOutcome) -> GateRecord {
    GateRecord {
        gate_id: id.to_string(),
        authority: format!("authority::{id}"),
        authority_version: "1".to_string(),
        policy_version: "policy/1".to_string(),
        classification: GateClassification::Blocking,
        started_at_ms: 1_000,
        completed_at_ms: 2_000,
        input_hash: "i".repeat(64),
        outcome,
        exit_code: Some(0),
        findings: Vec::new(),
        artifact_hashes: Vec::new(),
        duration_ms: 1_000,
    }
}

fn envelope(commit: &str, policy: &str) -> CiEvidence {
    let mut evidence = CiEvidence::new(revision(commit, policy));
    evidence.record(gate("build", GateOutcome::Pass));
    evidence.record(gate("test", GateOutcome::Pass));
    evidence
}

const NOW: u64 = 10_000;

fn sealed(evidence: &CiEvidence) -> AdmissionClaim {
    seal(evidence, "playbook", "scribe/1", Some(60_000), NOW).expect("seal")
}

#[test]
fn correct_claim_verifies() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let claim = sealed(&evidence);
    assert_eq!(verify(&claim, &evidence, NOW + 1), Ok(Verdict::Admitted));
    assert_eq!(
        verify_for(&claim, &evidence, "scribe/1", NOW + 1),
        Ok(Verdict::Admitted)
    );
}

#[test]
fn wrong_revision_rejected() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let mut claim = sealed(&evidence);
    claim.revision_sha = "b".repeat(40);
    assert_eq!(
        verify(&claim, &evidence, NOW).unwrap_err(),
        "wrong revision"
    );
}

#[test]
fn expired_claim_is_stale() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let claim = seal(&evidence, "playbook", "scribe/1", Some(1_000), NOW).expect("seal");
    assert_eq!(
        verify(&claim, &evidence, NOW + 1_000).unwrap_err(),
        "stale claim"
    );
}

#[test]
fn no_ttl_never_expires() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let claim = seal(&evidence, "playbook", "scribe/1", None, NOW).expect("seal");
    assert_eq!(
        verify(&claim, &evidence, NOW + u64::MAX / 2),
        Ok(Verdict::Admitted)
    );
}

#[test]
fn policy_mismatch_rejected() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let mut claim = sealed(&evidence);
    claim.policy_version = "policy/2".to_string();
    assert_eq!(verify(&claim, &evidence, NOW).unwrap_err(), "wrong policy");
}

#[test]
fn tampered_evidence_rejected() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let claim = sealed(&evidence);
    // Flip a gate outcome after sealing.
    let mut tampered = evidence.clone();
    let mut gate = gate("build", GateOutcome::Fail);
    gate.outcome = GateOutcome::Fail;
    gate.findings = vec![Finding {
        code: "F".to_string(),
        severity: "high".to_string(),
        message: "tampered".to_string(),
    }];
    tampered.gates[0] = gate;
    assert_eq!(
        verify(&claim, &tampered, NOW).unwrap_err(),
        "altered evidence root"
    );
}

#[test]
fn unrecognised_authority_rejected() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let claim = sealed(&evidence);
    assert_eq!(
        verify_for(&claim, &evidence, "scribe/2", NOW).unwrap_err(),
        "unrecognised authority"
    );
    // Empty authority is never recognised.
    assert_eq!(
        verify_for(&claim, &evidence, "", NOW).unwrap_err(),
        "unrecognised authority"
    );
}

#[test]
fn claim_with_empty_authority_rejected() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let mut claim = sealed(&evidence);
    claim.authority = "  ".to_string();
    assert_eq!(
        verify(&claim, &evidence, NOW).unwrap_err(),
        "unrecognised authority"
    );
}

#[test]
fn missing_mandatory_gate_rejected() {
    let mut evidence = envelope("a".repeat(40).as_str(), "policy/1");
    evidence.gates.retain(|gate| gate.gate_id != "test");
    let mut claim = sealed(&evidence);
    claim.mandatory_gates = vec!["build".to_string(), "test".to_string()];
    assert_eq!(
        verify(&claim, &evidence, NOW).unwrap_err(),
        "missing mandatory gate: test"
    );
}

#[test]
fn mandatory_gate_that_did_not_execute_rejected() {
    let mut evidence = envelope("a".repeat(40).as_str(), "policy/1");
    let mut skipped = gate("test", GateOutcome::SkippedAuthorised);
    skipped.classification = GateClassification::Conditional;
    skipped.outcome = GateOutcome::SkippedNotApplicable;
    evidence.gates[1] = skipped;
    let mut claim = sealed(&evidence);
    claim.mandatory_gates = vec!["test".to_string()];
    assert_eq!(
        verify(&claim, &evidence, NOW).unwrap_err(),
        "missing mandatory gate: test"
    );
}

#[test]
fn verdict_no_longer_derivable() {
    let evidence = envelope("a".repeat(40).as_str(), "policy/1");
    // Add a failing blocking gate plus a waiver that expires: at NOW the
    // waiver covers it, later it does not — the claim's verdict no longer
    // re-derives.
    let mut later = evidence.clone();
    later.record(gate("lint", GateOutcome::Fail));
    later.waivers.push(Waiver {
        gate_id: "lint".to_string(),
        reason: "temporary".to_string(),
        author: "scribe".to_string(),
        expires_at_ms: Some(NOW + 5_000),
        recorded_at_revision: "a".repeat(40),
    });
    let claim = seal(&later, "playbook", "scribe/1", Some(60_000), NOW).expect("seal");
    assert_eq!(verify(&claim, &later, NOW + 1), Ok(Verdict::Admitted));
    assert_eq!(
        verify(&claim, &later, NOW + 6_000).unwrap_err(),
        "verdict no longer derivable"
    );
}

#[test]
fn foreign_schema_refused_at_seal() {
    let mut evidence = envelope("a".repeat(40).as_str(), "policy/1");
    evidence.schema = "CI_EVIDENCE/0".to_string();
    assert!(seal(&evidence, "playbook", "scribe/1", None, NOW)
        .unwrap_err()
        .contains("CI_EVIDENCE/1"));
}

#[test]
fn rejected_verdict_cannot_be_sealed() {
    let mut evidence = envelope("a".repeat(40).as_str(), "policy/1");
    evidence.record(gate("build", GateOutcome::Fail));
    let err = seal(&evidence, "playbook", "scribe/1", None, NOW).unwrap_err();
    assert!(err.contains("Rejected"), "unexpected: {err}");
}

#[test]
fn registry_round_trip() {
    let commit = "a".repeat(40);
    let evidence = envelope(&commit, "policy/1");
    let claim = sealed(&evidence);
    let mut registry = ClaimRegistry::new();
    registry
        .record(claim.clone(), evidence.clone())
        .expect("record");
    assert_eq!(
        registry.admission_for(&commit, "policy/1", NOW + 1),
        Ok(Verdict::Admitted)
    );
    assert_eq!(
        registry
            .latest_for_revision(&commit)
            .map(|c| c.authority.as_str()),
        Some("scribe/1")
    );
}

#[test]
fn registry_refuses_duplicate() {
    let commit = "a".repeat(40);
    let evidence = envelope(&commit, "policy/1");
    let claim = sealed(&evidence);
    let mut registry = ClaimRegistry::new();
    registry
        .record(claim.clone(), evidence.clone())
        .expect("record");
    let err = registry.record(claim, evidence).unwrap_err();
    assert!(err.contains("already recorded"), "unexpected: {err}");
}

#[test]
fn registry_admission_names_missing_claim() {
    let registry = ClaimRegistry::new();
    let err = registry
        .admission_for("a".repeat(40).as_str(), "policy/1", NOW)
        .unwrap_err();
    assert!(err.contains("no claim recorded"), "unexpected: {err}");
}

#[test]
fn registry_admission_of_expired_claim_is_stale() {
    let commit = "a".repeat(40);
    let evidence = envelope(&commit, "policy/1");
    let claim = seal(&evidence, "playbook", "scribe/1", Some(1_000), NOW).expect("seal");
    let mut registry = ClaimRegistry::new();
    registry.record(claim, evidence).expect("record");
    assert_eq!(
        registry
            .admission_for(&commit, "policy/1", NOW + 2_000)
            .unwrap_err(),
        "stale claim"
    );
}

#[test]
fn registry_admission_of_tampered_stored_envelope_rejected() {
    let commit = "a".repeat(40);
    let evidence = envelope(&commit, "policy/1");
    let claim = sealed(&evidence);
    // The registry stores the envelope; mutate the stored envelope after
    // recording so it no longer matches the claim's evidence root.
    let mut tampered = evidence.clone();
    tampered.gates[0].findings.push(Finding {
        code: "F".to_string(),
        severity: "high".to_string(),
        message: "tampered".to_string(),
    });
    let mut registry = ClaimRegistry::new();
    registry.record(claim, tampered).expect("record");
    assert_eq!(
        registry
            .admission_for(&commit, "policy/1", NOW)
            .unwrap_err(),
        "altered evidence root"
    );
}

#[test]
fn policy_evolution_gets_a_second_claim() {
    let commit = "a".repeat(40);
    let evidence = envelope(&commit, "policy/1");
    let claim = sealed(&evidence);
    let mut registry = ClaimRegistry::new();
    registry.record(claim, evidence).expect("record");
    // Same revision, new policy: not a duplicate.
    let evidence2 = envelope(&commit, "policy/2");
    let claim2 = seal(&evidence2, "playbook", "scribe/1", None, NOW + 1).expect("seal");
    registry.record(claim2, evidence2).expect("record");
    assert_eq!(
        registry
            .latest_for_revision(&commit)
            .map(|c| c.policy_version.as_str()),
        Some("policy/2")
    );
}
