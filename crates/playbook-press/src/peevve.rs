//! PEEVVE — Canonical Human-Explanation Framework
//!
//! PEEVVE stands for:
//! - P: Point
//! - E: Evidence
//! - E: Explanation
//! - V: Validation
//! - V: Visualisation
//! - E: Evaluation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::claim_strength::ClaimStrength;
use crate::provenance::ProvenanceRecord;
use crate::visualisation::VisualisationSpec;
use crate::evaluation::EvaluationBlock;

/// A complete PEEVVE-structured package
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeevvePackage {
    pub points: Vec<Point>,
    pub evidence: Vec<Evidence>,
    pub explanations: Vec<Explanation>,
    pub validations: Vec<Validation>,
    pub visualisations: Vec<VisualisationSpec>,
    pub evaluations: Vec<EvaluationBlock>,
    pub component_map: HashMap<String, Vec<PeevveComponent>>,
}

impl Default for PeevvePackage {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            evidence: Vec::new(),
            explanations: Vec::new(),
            validations: Vec::new(),
            visualisations: Vec::new(),
            evaluations: Vec::new(),
            component_map: HashMap::new(),
        }
    }
}

/// Enumeration of PEEVVE components for cross-referencing
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeevveComponent {
    Point(String),
    Evidence(String),
    Explanation(String),
    Validation(String),
    Visualisation(String),
    Evaluation(String),
}

/// P — Point: The exact assertion being made
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub point_id: String,
    pub text: String,
    pub claim_strength: ClaimStrength,
    pub atomic: bool,
    pub source_claim_ids: Vec<String>,
}

impl Point {
    pub fn new(text: String, claim_strength: ClaimStrength) -> Self {
        Self {
            point_id: uuid::Uuid::new_v4().to_string(),
            text,
            claim_strength,
            atomic: true,
            source_claim_ids: Vec::new(),
        }
    }

    /// Check if the point uses ambiguous superlatives (forbidden per PEEVVE)
    pub fn has_ambiguous_superlatives(&self) -> bool {
        let forbidden = ["best", "worst", "fastest", "slowest", "most", "least", "always", "never", "perfect", "complete", "total", "absolute", "ultimate"];
        let lower = self.text.to_lowercase();
        forbidden.iter().any(|word| lower.contains(word))
    }

    /// Check for unsupported causality
    pub fn has_unsupported_causality(&self) -> bool {
        let indicators = ["because", "therefore", "causes", "leads to", "results in", "due to", "proves that"];
        let lower = self.text.to_lowercase();
        indicators.iter().any(|word| lower.contains(word))
    }
}

/// E — Evidence: Authoritative observations supporting the Point
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub evidence_id: String,
    pub evidence_type: EvidenceType,
    pub description: String,
    pub source_authority: String,
    pub provenance_record: ProvenanceRecord,
    pub supports_points: Vec<String>,
    pub confidence: f64,
}

/// Types of evidence admissible in PEEVVE
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    CiEvidence,
    BenchmarkMeasurement,
    JournalEntry,
    TestOutcome,
    Commit,
    Trace,
    ProviderResponse,
    DeterministicClassification,
    AcceptanceGateResult,
    ExternalSource,
}

impl Evidence {
    pub fn new(evidence_type: EvidenceType, description: String, source_authority: String, provenance: ProvenanceRecord) -> Self {
        Self {
            evidence_id: uuid::Uuid::new_v4().to_string(),
            evidence_type,
            description,
            source_authority,
            provenance_record: provenance,
            supports_points: Vec::new(),
            confidence: 1.0,
        }
    }
}

/// E — Explanation: Converts fact into fact + meaning
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Explanation {
    pub explanation_id: String,
    pub text: String,
    pub explains_point_ids: Vec<String>,
    pub references_evidence_ids: Vec<String>,
    pub demystified_terms: Vec<String>,
}

impl Explanation {
    pub fn new(text: String, explains_points: Vec<String>, references_evidence: Vec<String>) -> Self {
        Self {
            explanation_id: uuid::Uuid::new_v4().to_string(),
            text,
            explains_point_ids: explains_points,
            references_evidence_ids: references_evidence,
            demystified_terms: Vec::new(),
        }
    }
}

/// V — Validation: Why should the reader trust the Evidence?
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    pub validation_id: String,
    pub method: ValidationMethod,
    pub description: String,
    pub validates_evidence_ids: Vec<String>,
    pub authority: String,
    pub confidence: f64,
}

/// Validation methods per the directive
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationMethod {
    DeterministicReproduction,
    IndependentAuthority,
    Tests,
    Ci,
    Replay,
    CrossModelCorroboration,
    ChecksumVerification,
    BenchmarkMethodology,
    IndependentEvaluator,
    ExternalPrimarySource,
}

impl Validation {
    pub fn new(method: ValidationMethod, description: String, authority: String, evidence_ids: Vec<String>) -> Self {
        Self {
            validation_id: uuid::Uuid::new_v4().to_string(),
            method,
            description,
            validates_evidence_ids: evidence_ids,
            authority,
            confidence: 1.0,
        }
    }
}

/// V — Visualisation: Visual representation of data
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visualisation {
    pub visualisation_id: String,
    pub spec: VisualisationSpec,
    pub rendered_data: Option<String>,
}

impl Visualisation {
    pub fn new(spec: VisualisationSpec) -> Self {
        Self {
            visualisation_id: uuid::Uuid::new_v4().to_string(),
            spec,
            rendered_data: None,
        }
    }
}

/// E — Evaluation: Assessment of the claim
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evaluation {
    pub evaluation_id: String,
    pub block: EvaluationBlock,
}

impl Evaluation {
    pub fn new(block: EvaluationBlock) -> Self {
        Self {
            evaluation_id: uuid::Uuid::new_v4().to_string(),
            block,
        }
    }
}

/// Helper trait for PEEVVE components
pub trait PeevveElement {
    fn element_id(&self) -> &str;
    fn component_type(&self) -> &'static str;
}

impl PeevveElement for Point {
    fn element_id(&self) -> &str { &self.point_id }
    fn component_type(&self) -> &'static str { "Point" }
}

impl PeevveElement for Evidence {
    fn element_id(&self) -> &str { &self.evidence_id }
    fn component_type(&self) -> &'static str { "Evidence" }
}

impl PeevveElement for Explanation {
    fn element_id(&self) -> &str { &self.explanation_id }
    fn component_type(&self) -> &'static str { "Explanation" }
}

impl PeevveElement for Validation {
    fn element_id(&self) -> &str { &self.validation_id }
    fn component_type(&self) -> &'static str { "Validation" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_creation() {
        let point = Point::new("Playbook classified 8/8 cases correctly".into(), ClaimStrength::Observed);
        assert!(!point.text.is_empty());
        assert_eq!(point.claim_strength, ClaimStrength::Observed);
    }

    #[test]
    fn point_forbidden_language_detection() {
        let point = Point::new("Playbook is the best system ever".into(), ClaimStrength::Observed);
        assert!(point.has_ambiguous_superlatives());
    }

    #[test]
    fn point_causality_detection() {
        let point = Point::new("Playbook caused the test to pass because it is the best".into(), ClaimStrength::Observed);
        assert!(point.has_unsupported_causality());
    }

    #[test]
    fn evidence_creation() {
        let prov = ProvenanceRecord::new("test".into(), "test".into());
        let evidence = Evidence::new(EvidenceType::CiEvidence, "CI passed".into(), "ci".into(), prov);
        assert_eq!(evidence.evidence_type, EvidenceType::CiEvidence);
    }

    #[test]
    fn peevve_package_default() {
        let pkg = PeevvePackage::default();
        assert!(pkg.points.is_empty());
        assert!(pkg.evidence.is_empty());
    }
}