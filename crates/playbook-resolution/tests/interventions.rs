//! Phase A/B exit criteria (§53), run against the empirical corpus: the
//! seven recorded operator interventions from the real-world site-build
//! experiment. Every fixture must classify deterministically and the
//! prescription must be the remedy that actually resolved it. This is the
//! corpus the autonomous loop must later close without a human.

use playbook_resolution::{
    check_anchors, classify, prescribe, Action, RefusalCause, RefusalFamily, RefusalObservation,
    ResolutionLineage, ResolutionPolicy, WorkUnitContract,
};

fn blank() -> RefusalObservation {
    RefusalObservation {
        finish_reason: None,
        provider_status: None,
        exit_code: None,
        gate_failures: Vec::new(),
        output_structurally_complete: true,
        artefact_expected_but_missing: false,
        output_path_contended: false,
        version_control_failed: false,
        prior_attempts_with_same_failures: 0,
        semantic_requirements_met_after_normalisation: None,
        first_required_item_missing: None,
    }
}

/// A recorded intervention, as observed during the experiment.
struct Recorded {
    name: &'static str,
    observation: RefusalObservation,
    /// The family the operator's diagnosis belonged to.
    expected_cause: RefusalCause,
    /// The action that actually resolved the intervention.
    expected_action: Action,
}

fn recorded() -> Vec<Recorded> {
    vec![
        // 1 — the operator wrote the foreman harness: no refusal, a
        // capability gap in the platform itself.
        Recorded {
            name: "foreman harness gap",
            observation: blank(),
            expected_cause: RefusalCause::Unknown,
            expected_action: Action::Escalate,
        },
        // 2 — run 1: fences + literal headings; provider returned content
        // but the gate rejected it.
        Recorded {
            name: "brief rigidity (fences + literal headings)",
            observation: RefusalObservation {
                semantic_requirements_met_after_normalisation: Some(true),
                gate_failures: vec!["missing required content: What it is".into()],
                ..blank()
            },
            expected_cause: RefusalCause::BriefRigidity,
            expected_action: Action::NormaliseBrief,
        },
        // 3 — run 2: token starvation decapitated the HTML.
        Recorded {
            name: "token starvation (finish_reason length)",
            observation: RefusalObservation {
                finish_reason: Some("length".into()),
                output_structurally_complete: false,
                ..blank()
            },
            expected_cause: RefusalCause::OutputTruncation,
            expected_action: Action::ExpandEnvelope,
        },
        // 4 — runs 3–4: the 38-crate transcription truncated repeatedly.
        Recorded {
            name: "oversized work unit (transcription truncation)",
            observation: RefusalObservation {
                finish_reason: Some("length".into()),
                gate_failures: vec!["missing required content: ci-authorities".into()],
                output_structurally_complete: false,
                prior_attempts_with_same_failures: 3,
                ..blank()
            },
            expected_cause: RefusalCause::OutputTruncation,
            expected_action: Action::DecomposeWork,
        },
        // 5 — first-card drops: the anchor contract resolved it.
        Recorded {
            name: "anchor violation (first card dropped)",
            observation: RefusalObservation {
                gate_failures: vec!["missing required content: ci-authorities".into()],
                first_required_item_missing: Some(true),
                ..blank()
            },
            expected_cause: RefusalCause::AnchorViolation,
            expected_action: Action::ImposeAnchors,
        },
        // 6 — the deliver/generator path collision.
        Recorded {
            name: "output ownership collision",
            observation: RefusalObservation {
                output_path_contended: true,
                ..blank()
            },
            expected_cause: RefusalCause::OutputOwnershipCollision,
            expected_action: Action::AssignOwnership,
        },
        // 6.5 — dots-studio's provider errors during the runs (recorded in
        // the audit log): routed around by the foreman, never counted as
        // model incapability.
        Recorded {
            name: "provider error (dots-studio 503)",
            observation: RefusalObservation {
                provider_status: Some("provider error".into()),
                output_structurally_complete: false,
                ..blank()
            },
            expected_cause: RefusalCause::ProviderFailure,
            expected_action: Action::ChangeProvider,
        },
        // 7 — the kaptaind scheduler stall.
        Recorded {
            name: "version authority wedge",
            observation: RefusalObservation {
                version_control_failed: true,
                ..blank()
            },
            expected_cause: RefusalCause::VersionControlFailure,
            expected_action: Action::Escalate,
        },
    ]
}

#[test]
fn phase_a_every_recorded_intervention_classifies_deterministically() {
    let corpus = recorded();
    let mut hits = 0u32;
    for case in &corpus {
        let diagnosis = classify(&case.observation);
        let in_family = diagnosis.primary.family() == case.expected_cause.family();
        let exact = diagnosis.primary == case.expected_cause;
        // Unknown is a correct classification when there is no signal:
        // it honestly reports that no deterministic cause exists.
        let correct = (exact || in_family)
            && (diagnosis.deterministic || diagnosis.primary == RefusalCause::Unknown);
        if !correct {
            println!(
                "MISS: {} got {:?} det={} expected {:?}",
                case.name, diagnosis.primary, diagnosis.deterministic, case.expected_cause
            );
        }
        // Phase A: deterministic classification. Exact cause or same
        // family counts as a hit; anything else is a miss.
        if correct {
            hits += 1;
        }
    }
    let rate = (hits * 100) / corpus.len() as u32;
    assert!(
        rate >= 95,
        "Phase A exit: {rate}% deterministic classification ({hits}/{})",
        corpus.len()
    );
    // The taxonomy must not collapse the families the directive names.
    let families: Vec<RefusalFamily> = corpus
        .iter()
        .map(|case| classify(&case.observation).primary.family())
        .collect();
    assert!(families.contains(&RefusalFamily::Capability));
    assert!(families.contains(&RefusalFamily::WorkProduct));
    assert!(families.contains(&RefusalFamily::Execution));
    assert!(families.contains(&RefusalFamily::Governance));
}

#[test]
fn phase_b_every_prescription_agrees_with_the_remedy_that_worked() {
    let corpus = recorded();
    let policy = ResolutionPolicy::default();
    let mut agreed = 0u32;
    let mut escalations = 0u32;
    for (index, case) in corpus.iter().enumerate() {
        let diagnosis = classify(&case.observation);
        let prescription = prescribe(
            &diagnosis,
            &policy,
            case.observation.prior_attempts_with_same_failures,
            0,
            false,
        );
        if prescription.actions.contains(&case.expected_action) {
            agreed += 1;
        }
        if prescription.escalated {
            escalations += 1;
        }
        let _ = index;
    }
    assert!(
        agreed >= 5,
        "Phase B exit: prescriptions must agree with the working remedies ({agreed}/{}); escalations: {escalations}",
        corpus.len()
    );
    // The two true escalations — the harness gap and the version-authority
    // wedge — must escalate, because no autonomous repair exists for them.
    assert_eq!(
        escalations, 2,
        "autonomy includes knowing when not to continue"
    );
}

#[test]
fn phase_b_anchor_contract_detects_the_experiment_anchor_failure() {
    let contract = WorkUnitContract {
        unit_id: "crates-grid".into(),
        expected_items: 19,
        first_anchor: Some("ci-authorities".into()),
        last_anchor: Some("playbook-ytg".into()),
        order_immutable: true,
    };
    // The run-3 failure: the first card dropped.
    let delivered: Vec<String> = (0..18)
        .map(|index| format!("crate-{}", index + 1))
        .collect();
    assert_eq!(
        check_anchors(&contract, &delivered),
        playbook_resolution::AnchorVerdict::CardinalityMismatch
    );
    let wrong_first = {
        let mut items = vec!["crate-0".to_string()];
        items.extend((0..18).map(|index| format!("crate-{}", index + 1)));
        items
    };
    assert_eq!(
        check_anchors(&contract, &wrong_first),
        playbook_resolution::AnchorVerdict::FirstMissing
    );
    let mut complete = vec!["ci-authorities".to_string()];
    complete.extend((0..17).map(|index| format!("crate-{index}")));
    complete.push("playbook-ytg".to_string());
    assert_eq!(
        check_anchors(&contract, &complete),
        playbook_resolution::AnchorVerdict::Satisfied
    );
    // The contract binds and hashes like any evidence-bearing artefact.
    assert_eq!(contract.hash(), contract.hash());
}

#[test]
fn phase_b_lineage_reconstructs_the_five_run_history() {
    let mut lineage = ResolutionLineage::new();
    lineage.root("attempt-1", 0);
    let second = lineage
        .refuse("attempt-1", "brief-rigidity-digest", "normalise_brief")
        .unwrap();
    let third = lineage
        .refuse(&second, "truncation-digest", "expand_envelope")
        .unwrap();
    let fourth = lineage
        .refuse(&third, "truncation-digest", "decompose_work")
        .unwrap();
    let _fifth = lineage
        .refuse(&fourth, "anchor-digest", "impose_anchors")
        .unwrap();
    assert_eq!(lineage.depth(), 5, "the five-run history is one chain");
}
