//! The consumption boundary.
//!
//! A recovery artefact carries its failed context by design; a packet
//! carries the residue without it. Later personnel receive bounded lists —
//! the first entries, always in harvest order — plus the failure class, a
//! reuse hint, and the digests needed to trace the artefact back. The
//! packet refuses to carry competitive metadata: the serialized packet is
//! scanned before it leaves, and a leaking packet is refused outright.
//! Scoping never decides what is true; it only decides what leaves.

use serde_json::Value;

use playbook_core::{leaks_competitive_metadata, PlaybookError};

use crate::artefact::{RecoveryArtefact, RecoverySource};

/// Build a scoped consumable packet from an artefact: what later personnel
/// may receive without the failed context. Every list is truncated
/// deterministically to the first `max_items_per_list` entries, in harvest
/// order; the id, failure class, reuse hint, context digest, and timestamp
/// are never truncated. The serialized packet is scanned for competitive
/// metadata — and for credential-shaped material — and refused with
/// [`PlaybookError::AmnesiaLeak`] if it leaks. Credentials never travel in
/// a packet: facts reference secrets by digest, never by value.
pub fn packet(
    artefact: &RecoveryArtefact,
    max_items_per_list: usize,
) -> Result<Value, PlaybookError> {
    let bounded = |items: &[String]| -> Vec<String> {
        items.iter().take(max_items_per_list).cloned().collect()
    };
    let value = serde_json::json!({
        "id": artefact.id.as_str(),
        "failure_class": artefact.failure_class,
        "reuse_hint": reuse_hint(&artefact.source),
        "confirmed_facts": bounded(&artefact.confirmed_facts),
        "failed_assumptions": bounded(&artefact.failed_assumptions),
        "partial_patches": bounded(&artefact.partial_patches),
        "reproduction": bounded(&artefact.reproduction),
        "rejected_hypotheses": bounded(&artefact.rejected_hypotheses),
        "useful_commands": bounded(&artefact.useful_commands),
        "context_digest": artefact.context_digest,
        "at_ms": artefact.at_ms,
    });
    let bytes = serde_json::to_vec(&value).map_err(PlaybookError::json)?;
    if leaks_competitive_metadata(&bytes) || leaks_credentials(&bytes) {
        return Err(PlaybookError::AmnesiaLeak);
    }
    Ok(value)
}

/// Credential-shaped needles a leaked fact must never carry. The scan is
/// deliberately textual: a fact that names a credential value — however
/// innocently phrased — does not travel.
const CREDENTIAL_NEEDLES: &[&str] = &[
    "api_key=",
    "apikey=",
    "bearer ",
    "authorization:",
    "password=",
    "private_key=",
    "sk-",
];

/// True when the serialized packet carries credential-shaped material.
fn leaks_credentials(bytes: &[u8]) -> bool {
    let lowered = String::from_utf8_lossy(bytes).to_lowercase();
    CREDENTIAL_NEEDLES
        .iter()
        .any(|needle| lowered.contains(needle))
}

/// The hunt key a consumer needs to find this artefact again, without the
/// failed context that produced it.
fn reuse_hint(source: &RecoverySource) -> Value {
    match source {
        RecoverySource::FailedPlay { operation, .. } => {
            serde_json::json!({ "kind": "play", "operation": operation })
        }
        RecoverySource::FailedBlitz { failure, .. } => {
            serde_json::json!({ "kind": "blitz", "failure": failure })
        }
        RecoverySource::ProviderOutage { provider } => {
            serde_json::json!({ "kind": "outage", "provider": provider })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harvest::harvest_play;
    use playbook_core::{
        CapabilityId, DriveBudget, PersonnelId, PlayId, PlayKind, PlayResult, PlayStatus,
        ToolFailure, ToolFailureKind,
    };

    fn play_artefact(diagnostics: Vec<String>, output_refs: Vec<String>) -> RecoveryArtefact {
        let failure = ToolFailure {
            play_id: PlayId::new("play-7").unwrap(),
            play_kind: PlayKind::TargetedInference,
            operation: "inference.complete".into(),
            capability_id: CapabilityId::new("cap-inference").unwrap(),
            personnel_id: PersonnelId::new("p-model-a").unwrap(),
            kind: ToolFailureKind::Timeout,
            diagnostics,
            cost: DriveBudget::zero(),
            at_ms: 1_000,
            idempotency_key: None,
            side_effects: Vec::new(),
        };
        let result = PlayResult {
            status: PlayStatus::Failed,
            play_id: PlayId::new("play-7").unwrap(),
            operation: "inference.complete".into(),
            output_refs,
            evidence: Vec::new(),
            artefact_refs: Vec::new(),
            cost: DriveBudget::zero(),
            duration_ms: 50,
            diagnostics: Vec::new(),
            side_effects: Vec::new(),
            at_ms: 1_050,
        };
        harvest_play(&failure, Some(&result))
    }

    #[test]
    fn packet_truncates_deterministically() {
        let artefact = play_artefact(
            vec!["fact-1".into(), "fact-2".into(), "fact-3".into()],
            vec!["patch-1".into(), "patch-2".into()],
        );
        let scoped = packet(&artefact, 2).unwrap();
        // The first entries are kept, in harvest order.
        assert_eq!(
            scoped["confirmed_facts"],
            serde_json::json!(["fact-1", "fact-2"])
        );
        assert_eq!(
            scoped["partial_patches"],
            serde_json::json!(["patch-1", "patch-2"])
        );
        assert_eq!(scoped["context_digest"], artefact.context_digest.as_str());
        assert_eq!(scoped["at_ms"], serde_json::json!(1_000));
        let tight = packet(&artefact, 1).unwrap();
        assert_eq!(tight["confirmed_facts"], serde_json::json!(["fact-1"]));
        assert_eq!(tight["partial_patches"], serde_json::json!(["patch-1"]));
        let empty = packet(&artefact, 0).unwrap();
        assert_eq!(empty["confirmed_facts"], serde_json::json!([]));
        assert_eq!(empty["reproduction"], serde_json::json!([]));
        let full = packet(&artefact, 10).unwrap();
        assert_eq!(
            full["confirmed_facts"].as_array().map(|items| items.len()),
            Some(3)
        );
    }

    #[test]
    fn packet_refuses_credential_shaped_facts() {
        let credentialed = play_artefact(
            vec!["the endpoint requires api_key=SK-live-9f2c7 for auth".into()],
            Vec::new(),
        );
        assert!(matches!(
            packet(&credentialed, 4),
            Err(PlaybookError::AmnesiaLeak)
        ));
        // A clean fact of the same shape still travels.
        let clean = play_artefact(vec!["the endpoint returned 503 twice".into()], Vec::new());
        assert!(packet(&clean, 4).is_ok());
    }

    #[test]
    fn packet_refuses_competitive_metadata() {
        let clean = play_artefact(vec!["provider returned 503".into()], Vec::new());
        assert!(packet(&clean, 4).is_ok());
        let leaking = play_artefact(
            vec!["previous_model_identity was silenced mid-drive".into()],
            Vec::new(),
        );
        assert!(matches!(
            packet(&leaking, 4),
            Err(PlaybookError::AmnesiaLeak)
        ));
    }
}
