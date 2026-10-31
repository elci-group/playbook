//! The enterprise integration gauntlet (§32, Sprint F).
//!
//! One canonical campaign exercising all six external surfaces through the
//! real journal fold: REECE reads the ecosystem before the snap, Mesut
//! routes and scrambles execution, consolidation fires when two viable
//! implementations diverge, Theosis preflights capability freshness, and
//! Dreamseq reviews the closed drive. The constitutional assertion runs
//! through every phase: the advisory plane journals what it observed and
//! never mutates game truth — and the whole campaign replays identically.

use playbook_events::DriveEvent;
use playbook_integration::mesut::{
    route, scramble, CancellationContract, MesutExecutionRequest, PreservedContract, Priority,
    ResourceConstraints, SchedulingPreferences, WorkKind,
};
use playbook_integration::reece::{consider, field_read, ProposedAction, QbReeceQuery};
use playbook_policy::Policy;
use playbook_state::{fold, ApplyMode};
use std::collections::BTreeMap;

fn started() -> DriveEvent {
    DriveEvent::DriveStarted {
        drive_id: playbook_core::DriveId::new("drive-integration").unwrap(),
        task: playbook_core::TaskContract {
            statement: "execute across the integrated ecosystem".into(),
            workload_class: "engineering".into(),
            constraints: Vec::new(),
            integrity: playbook_core::TaskIntegrity::Sound,
        },
        budget: playbook_core::DriveBudget {
            tokens: 1_000_000,
            monetary_cost_micros: 100_000_000,
            wall_clock_ms: 600_000,
            tool_calls: 1_000,
            inference_calls: 1_000,
            retry_count: 100,
            context_capacity: 1_000_000,
            provider_quota: 10_000,
        },
        at_ms: 0,
    }
}

fn request(execution: &str) -> MesutExecutionRequest {
    MesutExecutionRequest {
        execution_id: execution.into(),
        work_kind: WorkKind::BlockingTool,
        priority: Priority::High,
        hard_constraints: ResourceConstraints {
            max_workers: 4,
            max_memory_hint_mb: Some(512),
            requires_dedicated_core: false,
        },
        preferred_constraints: SchedulingPreferences {
            prefer_substrate: None,
            defer_until_ms: None,
        },
        cancellation: CancellationContract {
            cancellable: true,
            graceful_window_ms: Some(100),
        },
        deadline_ms: Some(5_000),
        expected_evidence: vec![playbook_core::EvidenceClass::ToolInvocation],
    }
}

fn routed(execution: &str, arrival: u16) -> DriveEvent {
    let decision = route(&request(execution), arrival);
    DriveEvent::MesutExecutionRouted {
        execution_id: decision.execution_id,
        substrate: match decision.substrate {
            playbook_integration::mesut::Substrate::Tokio => "tokio".into(),
            playbook_integration::mesut::Substrate::Rayon => "rayon".into(),
            playbook_integration::mesut::Substrate::Blocking => "blocking".into(),
        },
        queue_position: decision.queue_position,
        reason_code: decision.reason_code,
    }
}

/// PHASE 1 — the QB consults REECE before the snap: bounded consideration
/// over the ecosystem snapshot, compressed into a field read.
#[test]
fn phase_one_the_qb_reads_the_ecosystem_before_calling_the_play() {
    let query = QbReeceQuery {
        objective_id: "objective-integration".into(),
        current_drive: "drive-integration".into(),
        proposed_action: ProposedAction::ApiMutation,
        affected_capabilities: vec!["mesut.scheduling".into()],
        recursion_budget: 3,
        evidence_cutoff_ms: 1_000,
    };
    // Facts use REECE's documented verb grammar so they project into the
    // record's categories.
    let snapshot = vec![
        playbook_integration::reece::EcosystemFact {
            project: "frampton".into(),
            fact: "depends_on integration current-schema".into(),
        },
        playbook_integration::reece::EcosystemFact {
            project: "dreamseq".into(),
            fact: "depends_on integration review-result".into(),
        },
        playbook_integration::reece::EcosystemFact {
            project: "hybreed".into(),
            fact: "conflict two reconciliation paths claimed".into(),
        },
        playbook_integration::reece::EcosystemFact {
            project: "theosis".into(),
            fact: "owned_by host capability-freshness".into(),
        },
    ];
    // ApiMutation is admission-required.
    assert!(matches!(
        playbook_integration::reece::admit(&query),
        playbook_integration::ReeceAdmission::Required { .. }
    ));
    let record = consider(&query, &snapshot);
    assert_eq!(record.relevant_projects.len(), 3, "budget 3, three hops");
    let read = field_read(&record);
    assert!(!read.threats.is_empty() || !read.coverage.is_empty());
}

/// PHASE 2 — Mesut routes mixed work: blocking tools to the blocking pool,
/// CPU-bound analysis to Rayon, priority-ordered queues. Congestion
/// scrambles the substrate while the contract is preserved.
#[test]
fn phase_two_mesut_routes_and_scrambles_under_pressure() {
    let decision = route(&request("exec-block"), 0);
    assert_eq!(
        decision.substrate,
        playbook_integration::mesut::Substrate::Blocking
    );
    let congested = playbook_integration::mesut::ExecutionPressure {
        queue_latency_ms: 9_999,
        ..Default::default()
    };
    let scrambled = scramble(
        &request("exec-block"),
        playbook_integration::mesut::Substrate::Tokio,
        &congested,
        PreservedContract {
            same_capability: true,
            same_evidence: true,
            same_authority: true,
            same_objective: true,
        },
    )
    .unwrap()
    .expect("congestion scrambles toward the blocking pool");
    assert!(scrambled.preserved.intact());
    // Journalled as execution metadata — not a new tactical play.
    let events = vec![
        started(),
        DriveEvent::MesutScrambleStarted {
            execution_id: scrambled.execution_id.clone(),
            reason: format!("{:?}", scrambled.reason).to_lowercase(),
            from_substrate: "tokio".into(),
            to_substrate: "blocking".into(),
        },
        DriveEvent::MesutScrambleCompleted {
            execution_id: scrambled.execution_id,
        },
    ];
    let state = fold(&events, &Policy::builtin(), ApplyMode::Audit).unwrap();
    assert_eq!(state.integrations().total("mesut_scramble_started"), 1);
    assert_eq!(
        fold(&events, &Policy::builtin(), ApplyMode::Resimulate).unwrap(),
        state
    );
}

/// PHASE 3 — two viable implementations diverge: consolidation is
/// requested, Hybreed reconciles, Emulsify plans, Kaptaind accepts — and a
/// UNI regression is rolled back.
#[test]
fn phase_three_divergent_candidates_consolidate_transactionally() {
    let candidate = playbook_integration::ConsolidationCandidate {
        objective_id: "objective-integration".into(),
        drive_id: "drive-integration".into(),
        baseline_ref: "digest-prime".into(),
        candidate_refs: vec!["digest-a".into(), "digest-b".into()],
        acceptance_gates: vec!["implementation".into()],
        evidence_refs: vec!["ev-1".into()],
        provenance: vec!["blitz-worker-w1".into(), "blitz-worker-w2".into()],
        constraints: playbook_integration::ConsolidationConstraints {
            max_merge_units: 4,
            require_evidence_refs: true,
        },
    };
    candidate.validate().unwrap();
    // A single candidate is not divergence: consolidation is refused.
    let mut solo = candidate.clone();
    solo.candidate_refs = vec!["digest-a".into()];
    assert!(solo.validate().is_err());

    let plan = playbook_integration::EmulsifyPlan {
        compatibility_class: playbook_integration::CompatibilityClass::Emulsify,
        merge_units: vec![playbook_integration::MergeUnit {
            unit: "parser-core".into(),
            sources: vec!["digest-a".into(), "digest-b".into()],
        }],
        adapters: vec!["schema-bridge".into()],
        suspended_components: Vec::new(),
        rejected_components: Vec::new(),
        expected_tree_digest: "merged-tree".into(),
    };
    plan.validate().unwrap();
    let digest = playbook_integration::plan_digest(&plan).unwrap();
    // The transaction: accepted when the gates reran and UNI held...
    assert!(matches!(
        playbook_integration::kaptaind_gate(&candidate, &plan, 97, 97, true, true),
        playbook_integration::GateVerdict::Accepted
    ));
    // ...rolled back on UNI regression.
    assert!(matches!(
        playbook_integration::kaptaind_gate(&candidate, &plan, 97, 80, true, true),
        playbook_integration::GateVerdict::Rejected { .. }
    ));
    let _ = digest;
}

/// PHASE 4 — Theosis preflight classifies capability freshness; a required
/// update outside a maintenance window refuses the capability, and a
/// downgrade is never applied.
#[test]
fn phase_four_theosis_preflights_and_never_downgrades() {
    use playbook_integration::theosis::{classify, ToolVersion, VersionClassification};
    // Downgrade forbidden: an older remote stays Current.
    assert_eq!(
        classify(&ToolVersion {
            tool: "mesut".into(),
            installed: "1.4.0".into(),
            remote: "1.2.0".into(),
            commit: None,
        }),
        VersionClassification::Current
    );
    let policy = playbook_integration::theosis::CompatibilityPolicy {
        minimum_required: BTreeMap::from([("dreamseq".to_string(), (2, 0))]),
    };
    // Installed below the required minimum: update REQUIRED.
    assert_eq!(
        playbook_integration::theosis::classify_with_policy(
            &ToolVersion {
                tool: "dreamseq".into(),
                installed: "1.9.0".into(),
                remote: "2.1.0".into(),
                commit: None,
            },
            &policy
        ),
        VersionClassification::UpdateRequired
    );
}

/// PHASE 5–6 — the drive closes on evidence (the real MVP acceptance drive),
/// Dreamseq reviews the closed drive from the journal, and its
/// recommendations are derived without reopening anything. Same journal →
/// same review input (§31).
#[test]
fn phase_five_six_dreamseq_reviews_the_closed_drive() {
    let events = playbook_sim::run_mvp().unwrap().events;
    let package = playbook_integration::dreamseq::review_package(
        "franchise-integration",
        "objective-integration",
        "drive-integration",
        &events,
    )
    .unwrap();
    let again = playbook_integration::dreamseq::review_package(
        "franchise-integration",
        "objective-integration",
        "drive-integration",
        &events,
    )
    .unwrap();
    assert_eq!(package, again, "same journal, same review input");
    let result = playbook_integration::dreamseq::evaluate(&package);
    // The MVP drive punts with real spend: the economics questions have
    // material to answer even when the specific findings vary.
    assert!(
        !result.salary_findings.is_empty() || package.inference_spend_tokens == 0,
        "a punt with spend owes a salary finding"
    );
}

/// The whole campaign through the journal fold: every advisory observation
/// lands in the integration ledger, the authoritative game state is
/// untouched by it, and the campaign replays identically.
#[test]
fn the_full_integration_campaign_journals_replays_and_never_yields_authority() {
    let mut events = vec![started()];
    events.push(DriveEvent::ReeceQbQueryRequested {
        objective_id: "objective-integration".into(),
        proposed_action: "api_mutation".into(),
    });
    events.push(DriveEvent::ReeceQbConsiderationReturned {
        objective_id: "objective-integration".into(),
        confidence_ppm: 910_000,
        conflicts: 1,
    });
    events.push(routed("exec-block", 0));
    events.push(DriveEvent::MesutScrambleStarted {
        execution_id: "exec-block".into(),
        reason: "executor_congestion".into(),
        from_substrate: "tokio".into(),
        to_substrate: "blocking".into(),
    });
    events.push(DriveEvent::MesutScrambleCompleted {
        execution_id: "exec-block".into(),
    });
    events.push(DriveEvent::ConsolidationRequested {
        objective_id: "objective-integration".into(),
        candidate_count: 2,
    });
    events.push(DriveEvent::HybreedPlanGenerated {
        confidence_ppm: 910_000,
        retained: 3,
        rejected: 1,
    });
    events.push(DriveEvent::EmulsifyPlanGenerated {
        compatibility_class: "emulsify".into(),
        merge_units: 1,
    });
    events.push(DriveEvent::KaptaindConsolidationAccepted {
        plan_digest: "digest-plan".into(),
    });
    events.push(DriveEvent::TheosisPreflightCompleted {
        current: 5,
        update_available: 1,
        update_required: 0,
        unavailable: 0,
    });
    events.push(DriveEvent::DreamseqDriveReviewRequested {
        drive_id: "drive-integration".into(),
    });
    events.push(DriveEvent::DreamseqDriveReviewCompleted {
        drive_id: "drive-integration".into(),
        personnel_adjustments: 1,
        infrastructure_opportunities: 2,
    });
    // The campaign journal ends on the advisory plane's own records: the
    // drive itself is still mid-possession, which is exactly the point — the
    // integration plane observes without driving the game to completion.

    let policy = Policy::builtin();
    let state = fold(&events, &policy, ApplyMode::Audit).unwrap();
    assert_eq!(
        fold(&events, &policy, ApplyMode::Resimulate).unwrap(),
        state,
        "the integration campaign replays identically"
    );
    // Every advisory observation is on the record.
    let ledger = state.integrations();
    assert_eq!(ledger.total("mesut_execution_routed"), 1);
    assert_eq!(ledger.total("mesut_scramble_started"), 1);
    assert_eq!(ledger.total("kaptaind_consolidation_accepted"), 1);
    assert_eq!(ledger.total("theosis_preflight_completed"), 1);
    assert_eq!(ledger.total("dreamseq_drive_review_completed"), 1);
    assert_eq!(ledger.total("reece_qb_consideration_returned"), 1);
    // And the completion authority is unchanged: no advisory event moved
    // the drive toward completion — status is exactly what the authoritative
    // events alone made it.
    assert_eq!(state.status(), playbook_core::DriveStatus::Active);
}
