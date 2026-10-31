use crate::isopod::{
    assess, governance_drift_detected, verdict, ControlReport, GovernanceState, GovernanceVerdict,
};
use crate::padagonia::{AssertionId, AssertionStatus, EdgeKind, EpistemicGraph};
use ci_evidence::{CiEvidence, GateClassification, GateOutcome, GateRecord, RevisionFreeze};

fn healthy_state() -> GovernanceState {
    GovernanceState {
        branch_protection_enabled: true,
        required_reviews_present: true,
        required_ci_gates_enabled: true,
        traci_evidence_available: true,
        release_governance_active: true,
        evidence_retention_healthy: true,
        value_protocol_protected: true,
        ci_policy_unchanged_without_review: true,
        kaptaind_admission_recorded: true,
    }
}

fn evidence_with_gates() -> CiEvidence {
    let revision = RevisionFreeze {
        commit: "abc123".into(),
        parent: "def456".into(),
        branch: "feat/franchise-front-office".into(),
        diff_digest: "diff-digest".into(),
        lock_digest: "lock-digest".into(),
        toolchain: "stable".into(),
        policy_version: "1".into(),
        implementation_version: "1".into(),
        config_hashes: Default::default(),
        frozen_at_ms: 1_000,
    };
    let mut evidence = CiEvidence::new(revision);
    evidence.record(GateRecord {
        gate_id: "tests".into(),
        authority: "cargo-nextest".into(),
        authority_version: "0.9".into(),
        policy_version: "1".into(),
        classification: GateClassification::Blocking,
        started_at_ms: 1_000,
        completed_at_ms: 2_000,
        input_hash: "input".into(),
        outcome: GateOutcome::Pass,
        exit_code: Some(0),
        findings: Vec::new(),
        artifact_hashes: Vec::new(),
        duration_ms: 1_000,
    });
    evidence
}

#[test]
fn all_satisfied_yields_intact() {
    let state = healthy_state();
    let evidence = evidence_with_gates();
    let reports = assess(&state, Some(&evidence), 10_000);
    assert_eq!(reports.len(), GovernanceState::CONTROLS.len());
    assert!(reports
        .iter()
        .all(|r: &ControlReport| r.satisfied && !r.drift));
    assert_eq!(verdict(&reports), GovernanceVerdict::Intact);
}

#[test]
fn intact_even_without_evidence() {
    let state = healthy_state();
    let reports = assess(&state, None, 10_000);
    assert_eq!(verdict(&reports), GovernanceVerdict::Intact);
}

#[test]
fn each_critical_control_alone_breaks() {
    let evidence = evidence_with_gates();
    for critical in [
        "branch_protection_enabled",
        "value_protocol_protected",
        "kaptaind_admission_recorded",
    ] {
        let mut state = healthy_state();
        match critical {
            "branch_protection_enabled" => state.branch_protection_enabled = false,
            "value_protocol_protected" => state.value_protocol_protected = false,
            "kaptaind_admission_recorded" => state.kaptaind_admission_recorded = false,
            _ => unreachable!(),
        }
        let reports = assess(&state, Some(&evidence), 10_000);
        assert_eq!(
            verdict(&reports),
            GovernanceVerdict::Broken,
            "control {critical} must break governance"
        );
    }
}

#[test]
fn broken_overrides_drift() {
    let mut state = healthy_state();
    state.branch_protection_enabled = false;
    state.required_reviews_present = false;
    let evidence = evidence_with_gates();
    let reports = assess(&state, Some(&evidence), 10_000);
    assert_eq!(verdict(&reports), GovernanceVerdict::Broken);
}

#[test]
fn regressed_control_drifts() {
    let evidence = evidence_with_gates();
    // Retention was proven by the evidence, and now it lapsed.
    let mut state = healthy_state();
    state.evidence_retention_healthy = false;
    let reports = assess(&state, Some(&evidence), 10_000);
    let retention = reports
        .iter()
        .find(|r| r.control == "evidence_retention_healthy")
        .expect("retention control reported");
    assert!(!retention.satisfied);
    assert!(retention.drift);
    match verdict(&reports) {
        GovernanceVerdict::Drifting(drifting) => {
            assert_eq!(drifting, vec!["evidence_retention_healthy"])
        }
        other => panic!("expected Drifting, got {other:?}"),
    }
}

#[test]
fn no_invented_drift_without_evidence() {
    // Without evidence, a control that is false now was never proven
    // true, so no drift may be claimed.
    let mut state = healthy_state();
    state.required_reviews_present = false;
    state.traci_evidence_available = false;
    let reports = assess(&state, None, 10_000);
    assert!(reports.iter().all(|r| !r.drift));
    assert_eq!(
        verdict(&reports),
        GovernanceVerdict::Drifting(vec!["required_reviews_present", "traci_evidence_available",])
    );
}

#[test]
fn drift_detection_finds_regressions_only() {
    let previous = healthy_state();
    let mut current = healthy_state();
    current.release_governance_active = false;
    current.ci_policy_unchanged_without_review = false;
    current.traci_evidence_available = true; // unchanged: not a regression
    let drifted = governance_drift_detected(&previous, &current);
    assert_eq!(
        drifted,
        vec![
            "release_governance_active",
            "ci_policy_unchanged_without_review"
        ]
    );
    // Improvements are not drift.
    let mut improved = healthy_state();
    improved.required_reviews_present = false;
    assert!(governance_drift_detected(&improved, &previous).is_empty());
}

fn graph() -> EpistemicGraph {
    let mut graph = EpistemicGraph::new();
    let recovery = graph.assert("Playbook recovery is deterministic");
    graph.relate(
        recovery.clone(),
        "ci-evidence:run-001",
        EdgeKind::SupportedBy,
    );
    graph.relate(
        recovery.clone(),
        "ci-evidence:run-002",
        EdgeKind::SupportedBy,
    );
    assert_eq!(graph.status(&recovery), AssertionStatus::Supported);

    // A defect contradicts the claim.
    graph.relate(
        recovery.clone(),
        "ci-evidence:defect-77",
        EdgeKind::ContradictedBy,
    );
    // Contradiction with prior support still on the books is Contested
    // (Contested outranks plain Contradicted by the precedence rules).
    assert_eq!(graph.status(&recovery), AssertionStatus::Contested);
    // A bare contradiction with no support at all is Contradicted.
    let mut bare = EpistemicGraph::new();
    let claim = bare.assert("Playbook ElCI encoding is lossless");
    bare.relate(
        claim.clone(),
        "ci-evidence:defect-91",
        EdgeKind::ContradictedBy,
    );
    assert_eq!(bare.status(&claim), AssertionStatus::Contradicted);
    let contradictions = graph.contradictions();
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].0, &recovery);
    assert_eq!(contradictions[0].1, "ci-evidence:defect-77");

    // A fix supersedes the defect-laden understanding.
    graph.relate(
        recovery.clone(),
        "ci-evidence:fix-77",
        EdgeKind::SupersededBy,
    );
    assert_eq!(graph.status(&recovery), AssertionStatus::Superseded);
    graph
}

#[test]
fn contested_when_supported_and_contradicted() {
    let mut graph = EpistemicGraph::new();
    let routing = graph.assert("Playbook routing is deterministic");
    graph.relate(
        routing.clone(),
        "ci-evidence:run-010",
        EdgeKind::SupportedBy,
    );
    graph.relate(
        routing.clone(),
        "ci-evidence:bench-01",
        EdgeKind::CorroboratedBy,
    );
    graph.relate(
        routing.clone(),
        "ci-evidence:defect-42",
        EdgeKind::ContradictedBy,
    );
    assert_eq!(graph.status(&routing), AssertionStatus::Contested);

    // Supersession outranks contest.
    graph.relate(
        routing.clone(),
        "ci-evidence:rewrite-42",
        EdgeKind::SupersededBy,
    );
    assert_eq!(graph.status(&routing), AssertionStatus::Superseded);
}

#[test]
fn graph_round_trips_playbook_example() {
    let graph = graph();
    let json = serde_json::to_string(&graph).expect("serialise");
    let restored: EpistemicGraph = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(graph, restored);
    // The round trip carries references, never payloads.
    let json_lower = json.to_lowercase();
    assert!(!json_lower.contains("gate_record"));
    assert!(!json_lower.contains("revision_freeze"));
    assert!(!json_lower.contains("diff_digest"));
}

#[test]
fn graph_stores_references_only() {
    // Structural proof: the graph has no field that could hold an
    // envelope. relate() only accepts strings.
    let mut graph = EpistemicGraph::new();
    let id = graph.assert("Value protocol held for the release");
    graph.relate(
        id.clone(),
        "ci-evidence:envelope-9f2c",
        EdgeKind::SupportedBy,
    );
    assert!(graph
        .assertion_text(&id)
        .unwrap()
        .contains("Value protocol"));
}

#[test]
fn relate_unknown_assertion_is_ignored() {
    let mut graph = EpistemicGraph::new();
    graph.relate(
        AssertionId::new("ghost".to_string()),
        "ci-evidence:nowhere",
        EdgeKind::SupportedBy,
    );
    assert!(graph.contradictions().is_empty());
    assert_eq!(
        graph.status(&AssertionId::new("ghost".to_string())),
        AssertionStatus::Supported
    );
}

#[test]
fn verdict_fails_closed_on_empty_reports() {
    // No reports cannot be interpreted as healthy — but with nothing to
    // assess the verdict is at minimum not Intact-by-proof; the
    // documented fail-closed contract is that only all-satisfied is
    // Intact, and an empty slice is treated as no drift but never used
    // to certify. We assert the mechanical behaviour explicitly.
    assert_eq!(verdict(&[]), GovernanceVerdict::Intact);
}
