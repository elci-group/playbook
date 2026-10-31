//! The recovery artefact.
//!
//! One materially failed inference yields one structured residue. The
//! artefact is content-addressed twice over: `context_digest` covers the
//! harvested content, and `id` covers everything. Both are digests of
//! canonical bytes, so the same inputs always name the same artefact and a
//! tampered artefact names itself.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use playbook_blitz::BlitzFailure;
use playbook_core::{
    canonical_bytes, sha256_hex, EvidenceId, FailureClass, PlaybookError, SCHEMA_VERSION,
};

/// Where the harvested cognition came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum RecoverySource {
    /// A play invocation failed materially.
    FailedPlay {
        /// The play that failed.
        play_id: String,
        /// The operation the play attempted.
        operation: String,
        /// The personnel that executed it.
        personnel_id: String,
    },
    /// A closed blitz. A blitz that recovered is harvested too — what
    /// worked is residue — so `failure` is `None` exactly when the record
    /// closed recovered with no failure to name.
    FailedBlitz {
        /// The blitz that closed.
        blitz_id: String,
        /// The recorded blitz failure, if any.
        failure: Option<BlitzFailure>,
    },
    /// A provider stopped serving inference.
    ProviderOutage {
        /// The provider that went dark.
        provider: String,
    },
}

/// One structured recovery artefact. Deterministic fields; nothing here is
/// generated prose — every entry comes from measured diagnostics, evidence,
/// or rejected candidates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryArtefact {
    /// The content address, computed by [`RecoveryArtefact::seal`].
    pub id: EvidenceId,
    /// Where the failed cognition came from.
    pub source: RecoverySource,
    /// The play-level failure class, when the source names a failure.
    pub failure_class: Option<FailureClass>,
    /// Measured observations: what the record proves happened.
    pub confirmed_facts: Vec<String>,
    /// Assumptions the failure disproved.
    pub failed_assumptions: Vec<String>,
    /// Work that survived the failure and may be resumed.
    pub partial_patches: Vec<String>,
    /// Deterministic steps that reproduce or reconcile the failure.
    pub reproduction: Vec<String>,
    /// Hypotheses the evidence ruled out.
    pub rejected_hypotheses: Vec<String>,
    /// Commands that produced measured, useful output.
    pub useful_commands: Vec<String>,
    /// Digest over the harvested content, independent of the id.
    pub context_digest: String,
    /// When the failure was recorded, in milliseconds.
    pub at_ms: u64,
}

impl RecoveryArtefact {
    /// The placeholder id of a not-yet-sealed artefact. [`Self::seal`]
    /// replaces it with the content address.
    pub(crate) fn unsealed_id() -> EvidenceId {
        EvidenceId::new("unsealed").expect("the unsealed marker is a valid id")
    }

    /// Seal: compute the content-addressed id from the canonical bytes of
    /// the whole artefact. The context digest is recomputed first, so a
    /// sealed artefact never carries a stale digest. Deterministic.
    pub fn seal(mut self) -> Result<Self, PlaybookError> {
        self.context_digest = sha256_hex(&canonical_bytes(&self.content_fingerprint())?);
        let mut whole = serde_json::to_value(&self).map_err(PlaybookError::json)?;
        if let Value::Object(ref mut fields) = whole {
            let _ = fields.remove("id");
        }
        self.id = EvidenceId::new(sha256_hex(&canonical_bytes(&whole)?))?;
        Ok(self)
    }

    /// The artefact is intact: the context digest matches the harvested
    /// content and the id matches the whole.
    pub fn verify(&self) -> Result<(), PlaybookError> {
        let sealed = self.clone().seal()?;
        if sealed.id != self.id || sealed.context_digest != self.context_digest {
            return Err(PlaybookError::DigestMismatch);
        }
        Ok(())
    }

    /// The canonical fingerprint over the harvested content: everything
    /// except the id and the context digest itself.
    fn content_fingerprint(&self) -> Value {
        serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "source": self.source,
            "failure_class": self.failure_class,
            "confirmed_facts": self.confirmed_facts,
            "failed_assumptions": self.failed_assumptions,
            "partial_patches": self.partial_patches,
            "reproduction": self.reproduction,
            "rejected_hypotheses": self.rejected_hypotheses,
            "useful_commands": self.useful_commands,
            "at_ms": self.at_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artefact(fact: &str) -> RecoveryArtefact {
        RecoveryArtefact {
            id: RecoveryArtefact::unsealed_id(),
            source: RecoverySource::ProviderOutage {
                provider: "provider-a".into(),
            },
            failure_class: Some(FailureClass::EnvironmentFailure),
            confirmed_facts: vec![fact.into()],
            failed_assumptions: vec!["provider-a was assumed available".into()],
            partial_patches: Vec::new(),
            reproduction: Vec::new(),
            rejected_hypotheses: Vec::new(),
            useful_commands: Vec::new(),
            context_digest: String::new(),
            at_ms: 10,
        }
        .seal()
        .unwrap()
    }

    #[test]
    fn sealing_is_deterministic_and_content_addressed() {
        let left = artefact("outage recorded");
        let right = artefact("outage recorded");
        assert_eq!(left, right);
        let changed = artefact("outage recorded at a different site");
        assert_ne!(left.id, changed.id);
        assert_ne!(left.context_digest, changed.context_digest);
    }

    #[test]
    fn verify_refuses_tampering() {
        let sealed = artefact("outage recorded");
        sealed.verify().unwrap();

        let mut tampered = sealed.clone();
        tampered
            .confirmed_facts
            .push("a fact nobody measured".into());
        assert!(matches!(
            tampered.verify(),
            Err(PlaybookError::DigestMismatch)
        ));

        let mut tampered = sealed.clone();
        tampered.context_digest = "0".repeat(64);
        assert!(matches!(
            tampered.verify(),
            Err(PlaybookError::DigestMismatch)
        ));

        let mut tampered = sealed;
        tampered.id = EvidenceId::new("ev-forged").unwrap();
        assert!(matches!(
            tampered.verify(),
            Err(PlaybookError::DigestMismatch)
        ));
    }
}
