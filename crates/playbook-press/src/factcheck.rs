//! Fact-Checking Engine

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{ClaimStrength, ProvenanceRecord};

/// Status of a factual assertion after fact-checking
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    /// Fully supported by evidence
    Supported,
    /// Partially supported by evidence
    PartiallySupported,
    /// Contradicted by evidence
    Contradicted,
    /// Insufficient evidence to verify
    Unverified,
}


/// Result of fact-checking a single assertion
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactCheckResult {
    pub assertion_id: String,
    pub assertion_text: String,
    pub status: AssertionStatus,
    pub confidence: f64,
    pub matched_evidence: Vec<String>,
    pub contradicting_evidence: Vec<String>,
    pub gaps: Vec<String>,
}

/// Complete fact-check report for a publication
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactCheckReport {
    pub report_id: String,
    pub publication_id: String,
    pub results: Vec<FactCheckResult>,
    pub summary: FactCheckSummary,
    pub checked_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactCheckSummary {
    pub total_assertions: usize,
    pub supported: usize,
    pub partially_supported: usize,
    pub contradicted: usize,
    pub unverified: usize,
    pub overall_confidence: f64,
    pub passed: bool,
}

/// Fact-checking engine that verifies assertions against evidence
pub struct FactCheckEngine {
    evidence_index: HashMap<String, String>, // evidence_id -> content
    claim_strength_map: HashMap<String, crate::ClaimStrength>,
}

impl FactCheckEngine {
    pub fn new() -> Self {
        Self {
            evidence_index: HashMap::new(),
            claim_strength_map: HashMap::new(),
        }
    }

    /// Load evidence from the evidence registry
    pub fn load_evidence(&mut self, evidence: Vec<(String, String)>) {
        for (id, content) in evidence {
            self.evidence_index.insert(id, content);
        }
    }

    /// Register a claim's authoritative strength
    pub fn register_claim_strength(&mut self, claim_id: String, strength: crate::ClaimStrength) {
        self.claim_strength_map.insert(claim_id, strength);
    }

    /// Fact-check a set of assertions against loaded evidence
    pub fn check_assertions(&self, assertions: Vec<(String, String)>) -> FactCheckReport {
        let mut results = Vec::new();
        let mut supported = 0;
        let mut partially = 0;
        let mut contradicted = 0;
        let mut unverified = 0;

        for (assertion_id, assertion_text) in assertions {
            let (status, confidence, matched, contradicting, gaps) = self.check_single_assertion(&assertion_text);

            match status {
                AssertionStatus::Supported => supported += 1,
                AssertionStatus::PartiallySupported => partially += 1,
                AssertionStatus::Contradicted => contradicted += 1,
                AssertionStatus::Unverified => unverified += 1,
            }

            results.push(FactCheckResult {
                assertion_id,
                assertion_text,
                status,
                confidence,
                matched_evidence: matched,
                contradicting_evidence: contradicting,
                gaps,
            });
        }

        let total = results.len();
        let overall_confidence = if total > 0 {
            results.iter().map(|r| r.confidence).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let passed = contradicted == 0 && unverified == 0;

        FactCheckReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            publication_id: "".into(),
            results,
            summary: FactCheckSummary {
                total_assertions: total,
                supported,
                partially_supported: partially,
                contradicted,
                unverified,
                overall_confidence,
                passed,
            },
            checked_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn check_single_assertion(&self, assertion: &str) -> (AssertionStatus, f64, Vec<String>, Vec<String>, Vec<String>) {
        // Simple keyword-based matching for demonstration
        // In production, this would use semantic matching, embeddings, etc.
        let assertion_lower = assertion.to_lowercase();
        let mut matched = Vec::new();
        let mut contradicting = Vec::new();
        let mut gaps = Vec::new();

        for (evidence_id, content) in &self.evidence_index {
            let content_lower = content.to_lowercase();
            // Simple word overlap scoring
            let assertion_words: std::collections::HashSet<_> = assertion_lower.split_whitespace().collect();
            let content_words: std::collections::HashSet<_> = content_lower.split_whitespace().collect();
            let overlap = assertion_words.intersection(&content_words).count();
            let assertion_len = assertion_words.len().max(1);

            if overlap as f64 / assertion_len as f64 > 0.5 {
                matched.push(content.clone());
            } else if content_lower.contains("contradict") || content_lower.contains("fail") {
                contradicting.push(content.clone());
            }
        }

        if matched.is_empty() {
            gaps.push("No supporting evidence found".into());
            (AssertionStatus::Unverified, 0.0, matched, contradicting, gaps)
        } else if contradicting.is_empty() {
            (AssertionStatus::Supported, 0.9, matched, contradicting, gaps)
        } else {
            (AssertionStatus::PartiallySupported, 0.5, matched, contradicting, gaps)
        }
    }
}

impl Default for FactCheckEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_check_engine_creation() {
        let engine = FactCheckEngine::new();
        assert!(engine.evidence_index.is_empty());
    }

    #[test]
    fn fact_check_supported_assertion() {
        let mut engine = FactCheckEngine::new();
        engine.load_evidence(vec![("EVD-1".into(), "Playbook classified 8 of 8 cases correctly".into())]);

        let report = engine.check_assertions(vec![("A1".into(), "Playbook classified 8 of 8 cases correctly".into())]);

        assert_eq!(report.results.len(), 1);
        assert_eq!(report.results[0].status, AssertionStatus::Supported);
        assert!(report.summary.passed);
    }

    #[test]
    fn fact_check_unverified_assertion() {
        let engine = FactCheckEngine::new();

        let report = engine.check_assertions(vec![("A1".into(), "Playbook is the best system ever".into())]);

        assert_eq!(report.results[0].status, AssertionStatus::Unverified);
        assert!(!report.summary.passed);
    }
}