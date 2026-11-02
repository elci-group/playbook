//! Provenance Records for Claims and Evidence

use serde::{Deserialize, Serialize};


/// Provenance record for a claim or evidence
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRecord {
    pub claim_id: Option<String>,
    pub source_authority: String,
    pub evidence_ids: Vec<String>,
    pub revision: String,
    pub timestamp: String,
    pub validation_authority: Option<String>,
    pub confidence: f64,
}

impl ProvenanceRecord {
    pub fn new(source_authority: String, revision: String) -> Self {
        Self {
            claim_id: None,
            source_authority,
            evidence_ids: Vec::new(),
            revision,
            timestamp: chrono::Utc::now().to_rfc3339(),
            validation_authority: None,
            confidence: 1.0,
        }
    }

    pub fn with_claim_id(mut self, claim_id: String) -> Self {
        self.claim_id = Some(claim_id);
        self
    }

    pub fn with_evidence_ids(mut self, evidence_ids: Vec<String>) -> Self {
        self.evidence_ids = evidence_ids;
        self
    }

    pub fn with_validation_authority(mut self, authority: String) -> Self {
        self.validation_authority = Some(authority);
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }
}

/// Errors for provenance operations
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ProvenanceError {
    #[error("missing required field: {0}")]
    MissingField(String),
    #[error("invalid revision format: {0}")]
    InvalidRevision(String),
    #[error("confidence out of range: {0}")]
    ConfidenceOutOfRange(f64),
    #[error("provenance chain broken: {0}")]
    ChainBroken(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_record_creation() {
        let prov = ProvenanceRecord::new("ci".into(), "abc123".into())
            .with_claim_id("CLM-00192".into())
            .with_evidence_ids(vec!["EVD-001".into()])
            .with_validation_authority("playbook-ci".into())
            .with_confidence(0.95);

        assert_eq!(prov.claim_id, Some("CLM-00192".into()));
        assert_eq!(prov.evidence_ids, vec!["EVD-001"]);
        assert_eq!(prov.validation_authority, Some("playbook-ci".into()));
        assert!((prov.confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn confidence_clamping() {
        let prov = ProvenanceRecord::new("test".into(), "rev".into()).with_confidence(1.5);
        assert_eq!(prov.confidence, 1.0);
        let prov = ProvenanceRecord::new("test".into(), "rev".into()).with_confidence(-0.5);
        assert_eq!(prov.confidence, 0.0);
    }
}