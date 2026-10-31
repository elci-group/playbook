//! The v0.2.4 CERS handoff campaign. The handoff is the product: failed
//! cognition becomes structured, scoped, leak-checked residue the
//! replacement can act on. These tests walk the full path the doctrine
//! names — harvest from failed work, seal, record, pull a packet — and
//! pin each boundary the replacement depends on: the failed identity
//! stays behind, blitz scouting survives the formation that bought it,
//! reuse is measured instead of assumed, one packet schema serves every
//! source, the leak refusal holds at the boundary, and the store
//! reconstructs deterministically from the same artefacts.

use playbook_blitz::{
    BlitzClass, BlitzClosure, BlitzCost, BlitzFailure, BlitzOutcome, BlitzRecord,
    BlitzResourceMode, Severity, WorkerScouting,
};
use playbook_cers::{harvest_blitz, harvest_outage, harvest_play, packet, CersStore, ReuseHint};
use playbook_core::{
    BlitzId, CapabilityId, DriveBudget, EvidenceId, FailureClass, PersonnelId, PlayId, PlayKind,
    PlayResult, PlayStatus, PlaybookError, ToolFailure, ToolFailureKind, PPM_SCALE,
};

/// A materially failed play: diagnostics carry the measured cause, the
/// idempotency key survives so the replacement can reconcile before
/// retrying.
fn failed_play(diagnostics: Vec<String>) -> ToolFailure {
    ToolFailure {
        play_id: PlayId::new("play-11").unwrap(),
        play_kind: PlayKind::TargetedInference,
        operation: "inference.complete".to_string(),
        capability_id: CapabilityId::new("cap-inference").unwrap(),
        personnel_id: PersonnelId::new("p-model-a").unwrap(),
        kind: ToolFailureKind::Timeout,
        diagnostics,
        cost: DriveBudget::zero(),
        at_ms: 1_000,
        idempotency_key: Some("idem-handoff-1".to_string()),
        side_effects: Vec::new(),
    }
}

/// The result that arrived with the failure: partial output survived.
fn play_result() -> PlayResult {
    PlayResult {
        status: PlayStatus::Failed,
        play_id: PlayId::new("play-11").unwrap(),
        operation: "inference.complete".to_string(),
        output_refs: vec!["draft/patch-1.diff".to_string()],
        evidence: Vec::new(),
        artefact_refs: Vec::new(),
        cost: DriveBudget::zero(),
        duration_ms: 50,
        diagnostics: Vec::new(),
        side_effects: Vec::new(),
        at_ms: 1_050,
    }
}

/// One worker's scouting row, as the fold records it.
fn worker(
    personnel: &str,
    provider: &str,
    family: &str,
    produced: bool,
    verified: bool,
) -> WorkerScouting {
    WorkerScouting {
        personnel_id: PersonnelId::new(personnel).unwrap(),
        provider: provider.to_string(),
        model_family: family.to_string(),
        wave: 1,
        produced_candidate: produced,
        candidate_verified: verified,
        tokens: 400,
        cost_microunits: 40,
        latency_ms: 900,
    }
}

/// A closed blitz carrying the given failure and scouting rows.
fn blitz_record(failure: Option<BlitzFailure>, workers: Vec<WorkerScouting>) -> BlitzRecord {
    BlitzRecord {
        id: BlitzId::new("blitz-9").unwrap(),
        class: BlitzClass::Instinctive,
        closure: if failure.is_none() {
            BlitzClosure::Recovered
        } else {
            BlitzClosure::Exhausted
        },
        reason_code: "scouting".to_string(),
        target_hash: "target".to_string(),
        resource_mode: BlitzResourceMode::Normal,
        severity: Severity::High,
        outcome: Some(BlitzOutcome {
            target_hash: "target".to_string(),
            winner: None,
            first_candidate_ms: Some(400),
            time_to_verified_ms: None,
            time_to_recovery_ms: None,
            raw_parallelism: 3,
            effective_parallelism_milli: 2_400,
            correlated_failures: 0,
            tokens_to_recovery: 0,
            cost_to_recovery_microunits: 0,
            cancellation: None,
            worker_results: workers,
            failure,
        }),
        spent: BlitzCost::zero(),
        at_ms: 100,
        closed_at_ms: 1_000,
    }
}

/// The v0.2.4 gate — harvest from failed work: a failed play's measured
/// diagnostics become confirmed facts, the store keeps the residue, and
/// the packet the replacement pulls carries the facts and the surviving
/// patches without the failed play's identity, byte-identical after a
/// serde_json round trip.
#[test]
fn a_failed_play_harvests_and_the_replacement_consumes_the_packet() {
    let diagnostics = vec![
        "provider returned 503 after 3 attempts".to_string(),
        "tokens consumed: 412".to_string(),
    ];
    let failure = failed_play(diagnostics.clone());
    let result = play_result();
    let artefact = harvest_play(&failure, Some(&result));
    // The harvest measured the real cause, not a narrative, and kept the
    // work that survived.
    assert_eq!(artefact.confirmed_facts, diagnostics);
    assert_eq!(artefact.partial_patches, result.output_refs);
    assert_eq!(
        artefact.reproduction,
        vec!["reconcile idempotency key idem-handoff-1 before retry".to_string()]
    );
    artefact.verify().unwrap();

    let mut store = CersStore::empty();
    store.record(artefact.clone()).unwrap();

    let scoped = packet(&artefact, 8).unwrap();
    // (a) The replacement receives the confirmed facts, the surviving
    // work, and the reproduction path.
    assert_eq!(scoped["confirmed_facts"], serde_json::json!(diagnostics));
    assert_eq!(
        scoped["partial_patches"],
        serde_json::json!(["draft/patch-1.diff"])
    );
    assert_eq!(
        scoped["reproduction"],
        serde_json::json!(["reconcile idempotency key idem-handoff-1 before retry"])
    );
    // (b) The failed identity stays behind: no play or personnel field
    // leaves, neither in the packet's keys nor anywhere in its bytes; the
    // hunt hint carries the operation, not who ran it.
    let text = serde_json::to_string(&scoped).unwrap();
    assert!(!text.contains("play_id"));
    assert!(!text.contains("personnel_id"));
    assert_eq!(
        scoped["reuse_hint"],
        serde_json::json!({ "kind": "play", "operation": "inference.complete" })
    );
    // (c) The packet round-trips: what the replacement parses is exactly
    // what crossed the boundary.
    let round: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(round, scoped);
}

/// The v0.2.4 gate — scouting outlives the formation that bought it: a
/// failed blitz's unverified producers name their provider and family as
/// failed assumptions, the verified winner names its own provider and
/// family as a confirmed fact, and the next formation finds the residue
/// by the recorded failure hint alone.
#[test]
fn a_failed_blitz_hands_scouting_to_the_next_formation() {
    let record = blitz_record(
        Some(BlitzFailure::VerificationFailure),
        vec![
            worker("p-model-a", "provider-alpha", "family-alpha", true, false),
            worker("p-adapter-a", "provider-alpha", "family-alpha", true, false),
            worker("p-model-b", "provider-beta", "family-beta", true, true),
        ],
    );
    let artefact = harvest_blitz(&record);
    // Who hunted this ground in vain is the reusable scouting: both
    // alpha producers are named, provider and family included.
    assert_eq!(
        artefact.failed_assumptions,
        vec![
            "personnel p-model-a on provider-alpha/family-alpha hunted this ground and produced no verified fix".to_string(),
            "personnel p-adapter-a on provider-alpha/family-alpha hunted this ground and produced no verified fix".to_string(),
        ]
    );
    // The winner is named by what was measured: a verified fix on beta.
    assert_eq!(
        artefact.confirmed_facts,
        vec![
            "personnel p-model-b on provider-beta/family-beta produced a verified fix".to_string()
        ]
    );
    assert_eq!(
        artefact.failure_class,
        Some(FailureClass::ValidationFailure)
    );
    artefact.verify().unwrap();

    let mut store = CersStore::empty();
    store.record(artefact.clone()).unwrap();
    // The next formation hunts by hint, never by narrative similarity.
    let found = store.matching(&ReuseHint::Blitz {
        failure: BlitzFailure::VerificationFailure,
    });
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, artefact.id);
}

/// The v0.2.4 gate — reuse is measured, not assumed: the rate starts at
/// zero, reaches the full million parts when the only artefact is
/// consumed, and lands on exactly half when one of two is.
#[test]
fn reuse_is_measured_not_assumed() {
    let mut single = CersStore::empty();
    let only = harvest_outage("provider-a", 1);
    single.record(only.clone()).unwrap();
    assert_eq!(single.reuse_rate_ppm(), 0);
    assert!(single.mark_reused(&only.id));
    assert_eq!(single.reuse_rate_ppm(), PPM_SCALE);

    let mut pair = CersStore::empty();
    pair.record(harvest_outage("provider-a", 2)).unwrap();
    pair.record(harvest_outage("provider-b", 3)).unwrap();
    assert_eq!(pair.reuse_rate_ppm(), 0);
    let first = pair.all()[0].id.clone();
    assert!(pair.mark_reused(&first));
    assert_eq!(pair.reuse_rate_ppm(), 500_000);
}

/// The v0.2.4 gate — one handoff schema for every source: play, blitz,
/// and outage residue each seals, each verifies, and each produces a
/// packet with the same top-level key set, so a consumer parses one
/// shape no matter where the residue came from.
#[test]
fn the_handoff_schema_is_uniform_across_sources() {
    let play = harvest_play(
        &failed_play(vec!["provider returned 503 after 3 attempts".to_string()]),
        Some(&play_result()),
    );
    let blitz = harvest_blitz(&blitz_record(
        Some(BlitzFailure::ProviderFailure),
        vec![worker(
            "p-model-b",
            "provider-beta",
            "family-beta",
            true,
            true,
        )],
    ));
    let outage = harvest_outage("provider-a", 5);
    // Every source seals into an intact artefact.
    for artefact in [&play, &blitz, &outage] {
        artefact.verify().unwrap();
    }
    // And every packet speaks the same schema.
    let expected = [
        "at_ms",
        "confirmed_facts",
        "context_digest",
        "failed_assumptions",
        "failure_class",
        "id",
        "partial_patches",
        "rejected_hypotheses",
        "reproduction",
        "reuse_hint",
        "useful_commands",
    ];
    for artefact in [&play, &blitz, &outage] {
        let scoped = packet(artefact, 4).unwrap();
        let mut keys: Vec<&str> = scoped
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, expected);
    }
}

/// The v0.2.4 gate — the leak boundary holds under handoff: a packet
/// carrying leaking residue is refused outright with `AmnesiaLeak`, a
/// refused handoff writes nothing to any store, and a clean handoff's
/// bytes carry no credential-shaped strings across the boundary.
#[test]
fn the_leak_boundary_holds_under_handoff() {
    // The refusal: the packet is scanned before it leaves, and a leaking
    // packet is refused, never redacted.
    let leaking = harvest_play(
        &failed_play(vec![
            "competitive_ranking ranked this provider first of three".to_string(),
        ]),
        None,
    );
    assert!(matches!(
        packet(&leaking, 4),
        Err(PlaybookError::AmnesiaLeak)
    ));
    // The handoff discipline: pull the packet first; record only what
    // cleared the boundary. A refused packet reaches no store.
    let mut store = CersStore::empty();
    if packet(&leaking, 4).is_ok() {
        store.record(leaking.clone()).unwrap();
    }
    assert_eq!(store.total(), 0);
    assert_eq!(store.reuse_rate_ppm(), 0);

    // The clean side: ordinary harvested residue produces a packet whose
    // serialized bytes carry no credential-shaped strings.
    let clean = harvest_play(
        &failed_play(vec!["provider returned 503 after 3 attempts".to_string()]),
        Some(&play_result()),
    );
    let scoped = packet(&clean, 4).unwrap();
    let text = serde_json::to_string(&scoped).unwrap().to_lowercase();
    for needle in ["api_key", "bearer ", "password", "secret"] {
        assert!(!text.contains(needle), "the handoff packet leaked {needle}");
    }
}

/// The artefacts of one reconstructed institution, recorded in a fixed
/// order: two outages and a failed blitz.
fn populate(store: &mut CersStore) -> Vec<EvidenceId> {
    let artefacts = [
        harvest_outage("provider-c", 7),
        harvest_blitz(&blitz_record(
            Some(BlitzFailure::CorrelatedFailure),
            vec![worker("p-model-a", "provider-a", "family-a", true, false)],
        )),
        harvest_outage("provider-a", 9),
    ];
    let mut ids = Vec::new();
    for artefact in artefacts {
        ids.push(artefact.id.clone());
        store.record(artefact).unwrap();
    }
    ids
}

/// The v0.2.4 gate — the store reconstructs deterministically: two
/// stores fed the same artefacts in the same order hold identical
/// content in identical order, answer every hunt hint identically, and
/// measure reuse identically.
#[test]
fn stores_reconstruct_deterministically() {
    let mut left = CersStore::empty();
    let left_ids = populate(&mut left);
    let mut right = CersStore::empty();
    let right_ids = populate(&mut right);
    assert_eq!(left_ids, right_ids);
    assert_eq!(left.all(), right.all());
    let hints = [
        ReuseHint::Outage {
            provider: "provider-c".to_string(),
        },
        ReuseHint::Blitz {
            failure: BlitzFailure::CorrelatedFailure,
        },
        ReuseHint::Play {
            operation: "inference.complete".to_string(),
        },
    ];
    for hint in &hints {
        assert_eq!(left.matching(hint), right.matching(hint));
    }
    assert_eq!(left.reuse_rate_ppm(), right.reuse_rate_ppm());
    // The measurement moves identically in both institutions.
    assert!(left.mark_reused(&left_ids[0]));
    assert!(right.mark_reused(&right_ids[0]));
    assert_ne!(left.reuse_rate_ppm(), 0);
    assert_eq!(left.reuse_rate_ppm(), right.reuse_rate_ppm());
    assert_eq!(left, right);
}
