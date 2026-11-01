//! The film-room refusal panel: strict parity with the resolution
//! record, and the mandatory §36 acceptance line.

use playbook_presentation::ResolutionSnapshot;
use playbook_resolution::classify::{Confidence, Diagnosis, RefusalCause};
use playbook_resolution::lineage::ResolutionLineage;
use playbook_resolution::policy::{Action, Prescription};
use playbook_tui::{render_lineage, render_refusal};

const WIDTH: usize = 80;

fn fixture() -> (Diagnosis, Prescription, ResolutionLineage) {
    let diagnosis = Diagnosis {
        primary: RefusalCause::OutputTruncation,
        confidence: Confidence::Certain,
        deterministic: true,
        rationale: "finish_reason 'length' with structurally incomplete output".into(),
    };
    let prescription = Prescription {
        actions: vec![Action::ExpandEnvelope, Action::DecomposeWork],
        suggested_envelope: Some(8_000),
        suggested_chunks: Some(3),
        rationale: "token starvation: widen, then split".into(),
        escalated: false,
    };
    let mut lineage = ResolutionLineage::new();
    lineage.root("attempt-1", 1_000);
    lineage.refuse("attempt-1", "deadbeefcafe", "decompose_work");
    (diagnosis, prescription, lineage)
}

#[test]
fn panel_contains_the_mandatory_acceptance_line() {
    let (diagnosis, prescription, lineage) = fixture();
    let snapshot = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
    let panel = render_refusal(&snapshot, WIDTH);
    assert!(panel.contains("FLAG ON PLAY"));
    assert!(panel.contains("Acceptance standard changed: NO"));
    assert!(!panel.contains("changed: YES"));
}

#[test]
fn policy_revision_panel_carries_the_recorded_hash() {
    let (diagnosis, prescription, lineage) = fixture();
    let snapshot = ResolutionSnapshot::for_policy_revision(
        &diagnosis,
        &prescription,
        &lineage,
        "feed01".into(),
    );
    let panel = render_refusal(&snapshot, WIDTH);
    assert!(panel.contains("Acceptance standard changed: YES (policy feed01)"));
    assert!(!panel.contains("changed: NO\n"));
}

#[test]
fn panel_renders_cause_adjustments_and_escalation() {
    let (diagnosis, mut prescription, lineage) = fixture();
    let snapshot = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
    let panel = render_refusal(&snapshot, WIDTH.max(96));
    assert!(panel.contains(
        "CAUSE: output_truncation · family capability · confidence certain · deterministic"
    ));
    assert!(panel.contains("expand_envelope"));
    assert!(panel.contains("decompose_work"));
    assert!(!panel.contains("ESCALATED"));
    prescription.escalated = true;
    let escalated = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
    assert!(render_refusal(&escalated, WIDTH).contains("ESCALATED"));
}

#[test]
fn lineage_renders_each_hop() {
    let (_, _, lineage) = fixture();
    let rendered = render_lineage(&lineage, WIDTH);
    assert!(rendered.contains("RECOVERY CHAIN"));
    assert!(rendered.contains("ATTEMPT attempt-1"));
    assert!(rendered.contains("refusal deadbeef"));
    assert!(rendered.contains("decompose_work"));
    // The child attempt the refusal produced (§37: attempt → refusal →
    // resolution → child).
    let child = lineage.attempts()[1].attempt_id.clone();
    assert!(rendered.contains(&format!("ATTEMPT {child}")));
    assert!(rendered.contains("DEPTH: 2"));
}
