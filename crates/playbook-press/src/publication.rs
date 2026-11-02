//! Publication Request, Package, and Verdict Types

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::peevve::{PeevvePackage, Point, Evidence, Explanation, Validation};
use crate::provenance::ProvenanceRecord;
use crate::visualisation::VisualisationSpec;
use crate::evaluation::EvaluationBlock;
use crate::demystify::TerminologyMap;
use crate::style::StyleProfile;
use crate::factcheck::AssertionStatus;
use crate::visualisation::DataSourceType;
use crate::audience::AudienceProfile;
use crate::channel::PublicationChannel;
use crate::policy::VisualisationPolicy;
use crate::peevve::{Point, Evidence, Explanation, Validation};

/// Unique identifier for a publication
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PublicationId(pub String);

impl PublicationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn from_string(s: String) -> Self {
        Self(s)
    }
}

impl Default for PublicationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PublicationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The purpose of a publication
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationPurpose {
    Report,
    TechnicalSummary,
    StatusBriefing,
    ExecutiveSummary,
    InvestorMaterial,
    AcademicMaterial,
    CustomerCommunication,
    PublicDocumentation,
    ReleaseNotes,
    IncidentReport,
    PrMaterial,
    BenchmarkResults,
    Diagram,
    WebsiteCopy,
    Custom(String),
}

/// Level of detail requested
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailLevel {
    Minimal,
    Standard,
    Detailed,
    Forensic,
}

/// Publication request submitted to the Press Officer
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRequest {
    pub publication_id: PublicationId,
    pub subject: String,
    pub purpose: PublicationPurpose,
    pub audience: AudienceProfile,
    pub channel: PublicationChannel,

    /// Source claim IDs from the authoritative claims registry
    pub source_claim_ids: Vec<String>,
    /// Source evidence IDs from the evidence registry
    pub source_evidence_ids: Vec<String>,

    /// Requested detail level
    pub requested_detail: DetailLevel,
    /// Optional style profile override
    pub style_profile: Option<String>,
    /// Visualisation policy for this publication
    pub visualisation_policy: VisualisationPolicy,
}

/// Verdict rendered by the Press Officer
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationVerdict {
    /// Approved for publication as-is
    Approved,
    /// Approved with specific caveats attached
    ApprovedWithCaveats,
    /// Requires clarification before proceeding
    RequiresClarification,
    /// Withheld due to insufficient evidence support
    WithheldUnsupported,
    /// Withheld due to policy restriction (e.g., legal, security)
    WithheldPolicy,
}

impl PublicationVerdict {
    pub fn is_approved(self) -> bool {
        matches!(self, Self::Approved | Self::ApprovedWithCaveats)
    }

    pub fn is_withheld(self) -> bool {
        matches!(self, Self::WithheldUnsupported | Self::WithheldPolicy)
    }
}

/// Complete publication package produced by the Press Officer
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationPackage {
    pub publication_id: PublicationId,

    /// PEEVVE-structured content
    pub point: Vec<Point>,
    pub evidence: Vec<Evidence>,
    pub explanation: Vec<Explanation>,
    pub validation: Vec<Validation>,
    pub visualisations: Vec<VisualisationSpec>,
    pub evaluation: Vec<EvaluationBlock>,

    /// Terminology mappings used
    pub terminology_map: TerminologyMap,
    /// Citations to authoritative sources
    pub citations: Vec<CitationRef>,
    /// Caveats attached to the publication
    pub caveats: Vec<Caveat>,
    /// Overall confidence profile
    pub confidence: ConfidenceProfile,

    /// Style profile applied
    pub style_profile: StyleProfile,

    /// Final publication verdict
    pub verdict: PublicationVerdict,
}

/// Confidence profile for a publication
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceProfile {
    pub overall: f64,
    pub by_claim: std::collections::HashMap<String, f64>,
    pub caveats: Vec<String>,
}

impl Default for ConfidenceProfile {
    fn default() -> Self {
        Self {
            overall: 1.0,
            by_claim: Default::default(),
            caveats: Default::default(),
        }
    }
}

/// Citation reference
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitationRef {
    pub citation_id: String,
    pub source: String,
    pub uri: Option<String>,
    pub accessed_at: String,  // ISO 8601 timestamp string
}

/// Caveat attached to a publication
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caveat {
    pub caveat_id: String,
    pub text: String,
    pub severity: CaveatSeverity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaveatSeverity {
    Info,
    Warning,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_id_generation() {
        let id1 = PublicationId::new();
        let id2 = PublicationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn publication_verdict_helpers() {
        assert!(PublicationVerdict::Approved.is_approved());
        assert!(PublicationVerdict::ApprovedWithCaveats.is_approved());
        assert!(!PublicationVerdict::RequiresClarification.is_approved());
        assert!(PublicationVerdict::WithheldUnsupported.is_withheld());
        assert!(PublicationVerdict::WithheldPolicy.is_withheld());
    }

    #[test]
    fn publication_request_defaults() {
        let req = PublicationRequest {
            publication_id: PublicationId::new(),
            subject: "Test".into(),
            purpose: crate::PublicationPurpose::Report,
            audience: AudienceProfile::technical(),
            channel: PublicationChannel::Report,
            source_claim_ids: vec![],
            source_evidence_ids: vec![],
            requested_detail: DetailLevel::Standard,
            style_profile: None,
            visualisation_policy: VisualisationPolicy::default(),
        };
        assert_eq!(req.requested_detail, DetailLevel::Standard);
    }
}