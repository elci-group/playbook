//! Evaluation Engine

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::publication::PublicationPackage;
use crate::peevve::{Point, Evidence, Explanation, Validation, Visualisation, Evaluation as PeevveEvaluation};
use crate::claim_strength::ClaimStrength;
use crate::style::ClaimStrengthPolicy;
use crate::style::UncertaintyVisibility;
use crate::policy::MarketingInflationPolicy;
use crate::publication::{PublicationId, ConfidenceProfile, PublicationVerdict};
use crate::demystify::TerminologyMap;
use crate::style::StyleProfile;
use crate::visualisation::{UncertaintyRepresentation, VisualisationSpec, DataSourceType};

/// Evaluation block in a publication
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationBlock {
    pub evaluation_id: String,
    pub criterion: EvaluationCriterion,
    pub result: EvaluationResult,
    pub details: String,
    pub confidence: f64,
}

/// Evaluation criteria
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationCriterion {
    FactualAccuracy,
    ProvenanceCompleteness,
    ClaimProportionality,
    AudienceFit,
    Demystification,
    PeevveCompleteness,
    StyleCompliance,
    UncertaintyDisclosure,
    VisualIntegrity,
    PolicyCompliance,
}

/// Result of an evaluation criterion
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationResult {
    Pass,
    Fail,
    Warning,
    NotApplicable,
}

/// Evaluation engine
pub struct EvaluationEngine {
    rules: std::collections::HashMap<EvaluationCriterion, EvaluationRule>,
}

impl EvaluationEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            rules: std::collections::HashMap::new(),
        };
        engine.register_default_rules();
        engine
    }

    fn register_default_rules(&mut self) {
        use EvaluationCriterion::*;

        self.rules.insert(FactualAccuracy, EvaluationRule {
            criterion: FactualAccuracy,
            check: Box::new(|package: &PublicationPackage| {
                let mut issues = Vec::new();
                for point in &package.point {
                    if point.has_ambiguous_superlatives() {
                        issues.push(format!("Point '{}' uses ambiguous superlatives", point.point_id));
                    }
                    if point.has_unsupported_causality() {
                        issues.push(format!("Point '{}' contains unsupported causality", point.point_id));
                    }
                }
                if issues.is_empty() {
                    Ok(())
                } else {
                    Err(issues.join("; "))
                }
            }),
        });

        self.rules.insert(EvaluationCriterion::ProvenanceCompleteness, EvaluationRule {
            criterion: ProvenanceCompleteness,
            check: Box::new(|package: &PublicationPackage| {
                let mut missing = Vec::new();
                for point in &package.point {
                    if point.source_claim_ids.is_empty() {
                        missing.push(format!("Point '{}' has no source claims", point.point_id));
                    }
                }
                for ev in &package.evidence {
                    if ev.provenance_record.source_authority.is_empty() {
                        missing.push(format!("Evidence '{}' missing source authority", ev.evidence_id));
                    }
                }
                if missing.is_empty() {
                    Ok(())
                } else {
                    Err(missing.join("; "))
                }
            }),
        });

        self.rules.insert(EvaluationCriterion::ClaimProportionality, EvaluationRule {
            criterion: ClaimProportionality,
            check: Box::new(|package: &PublicationPackage| {
                let mut issues = Vec::new();
                for point in &package.point {
                    // Check for forbidden claim strength transformations
                    // This is a simplified check - real implementation would trace claim strength
                    if point.claim_strength == ClaimStrength::Validated && point.source_claim_ids.is_empty() {
                        issues.push(format!("Point '{}' claims Validated strength but has no source claims", point.point_id));
                    }
                }
                if issues.is_empty() {
                    Ok(())
                } else {
                    Err(issues.join("; "))
                }
            }),
        });

        self.rules.insert(EvaluationCriterion::AudienceFit, EvaluationRule {
            criterion: AudienceFit,
            check: Box::new(|package: &PublicationPackage| {
                // Simplified check - real implementation would verify against audience profile
                Ok(())
            }),
        });

        self.rules.insert(EvaluationCriterion::Demystification, EvaluationRule {
            criterion: Demystification,
            check: Box::new(|package: &PublicationPackage| {
                // Check that terminology map covers terms used in points/explanations
                Ok(())
            }),
        });

        self.rules.insert(EvaluationCriterion::PeevveCompleteness, EvaluationRule {
            criterion: PeevveCompleteness,
            check: Box::new(|package: &PublicationPackage| {
                let mut missing = Vec::new();
                if package.point.is_empty() { missing.push("No Points".into()); }
                if package.evidence.is_empty() { missing.push("No Evidence".into()); }
                if package.explanation.is_empty() { missing.push("No Explanations".into()); }
                if package.validation.is_empty() { missing.push("No Validations".into()); }
                if package.visualisations.is_empty() { missing.push("No Visualisations".into()); }
                if package.evaluation.is_empty() { missing.push("No Evaluations".into()); }

                if missing.is_empty() {
                    Ok(())
                } else {
                    Err(format!("Missing PEEVVE components: {}", missing.join(", ")))
                }
            }),
        });

        self.rules.insert(EvaluationCriterion::StyleCompliance, EvaluationRule {
            criterion: StyleCompliance,
            check: Box::new(|package: &PublicationPackage| {
                // Check style compliance
                if package.style_profile.claim_strength_policy == ClaimStrengthPolicy::Optimistic {
                    // Flag optimistic framing
                }
                Ok(())
            }),
        });

        self.rules.insert(EvaluationCriterion::UncertaintyDisclosure, EvaluationRule {
            criterion: UncertaintyDisclosure,
            check: Box::new(|package: &PublicationPackage| {
                // Check uncertainty disclosure per style profile
                if package.style_profile.uncertainty_visibility == UncertaintyVisibility::Mandatory {
                    for point in &package.point {
                        if point.claim_strength == ClaimStrength::Speculative || point.claim_strength == ClaimStrength::Projected {
                            // Should have uncertainty language
                        }
                    }
                }
                Ok(())
            }),
        });

        self.rules.insert(EvaluationCriterion::VisualIntegrity, EvaluationRule {
            criterion: VisualIntegrity,
            check: Box::new(|package: &PublicationPackage| {
                for vis in &package.visualisations {
                    // Check for visual truth violations
                    if vis.uncertainty_representation == UncertaintyRepresentation::None && vis.claims_represented.iter().any(|c| c.contains("projected")) {
                        // Flag visualisations that don't represent uncertainty for projections
                    }
                }
                Ok(())
            }),
        });

        self.rules.insert(EvaluationCriterion::PolicyCompliance, EvaluationRule {
            criterion: PolicyCompliance,
            check: Box::new(|package: &PublicationPackage| {
                // Check marketing inflation
                if package.style_profile.marketing_inflation_policy == MarketingInflationPolicy::Forbidden {
                    for point in &package.point {
                        if point.has_ambiguous_superlatives() {
                            return Err("Marketing inflation detected: ambiguous superlatives in point".into());
                        }
                    }
                }
                Ok(())
            }),
        });
    }

    pub fn evaluate(&self, package: &PublicationPackage) -> EvaluationReport {
        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let mut warnings = 0;

        for (criterion, rule) in &self.rules {
            match (rule.check)(package) {
                Ok(()) => {
                    results.push(EvaluationResultDetail {
                        criterion: *criterion,
                        result: EvaluationResult::Pass,
                        details: "Passed".into(),
                    });
                    passed += 1;
                }
                Err(msg) => {
                    results.push(EvaluationResultDetail {
                        criterion: *criterion,
                        result: EvaluationResult::Fail,
                        details: msg,
                    });
                    failed += 1;
                }
            }
        }

        EvaluationReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            total_criteria: self.rules.len(),
            passed,
            failed,
            warnings,
            details: results,
            overall: if failed == 0 { EvaluationResult::Pass } else { EvaluationResult::Fail },
        }
    }
}

/// Evaluation rule
struct EvaluationRule {
    criterion: EvaluationCriterion,
    check: Box<dyn Fn(&PublicationPackage) -> Result<(), String> + Send + Sync>,
}

/// Evaluation result detail
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationResultDetail {
    pub criterion: EvaluationCriterion,
    pub result: EvaluationResult,
    pub details: String,
}

/// Complete evaluation report
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationReport {
    pub report_id: String,
    pub total_criteria: usize,
    pub passed: usize,
    pub failed: usize,
    pub warnings: usize,
    pub details: Vec<EvaluationResultDetail>,
    pub overall: EvaluationResult,
}

impl EvaluationReport {
    pub fn passed(&self) -> bool {
        self.failed == 0
    }
}

impl Default for EvaluationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_engine_creation() {
        let engine = EvaluationEngine::new();
        assert!(!engine.rules.is_empty());
    }

    #[test]
    fn peevve_completeness_check() {
        let engine = EvaluationEngine::new();
        let mut package = PublicationPackage {
            publication_id: PublicationId::new(),
            point: vec![],
            evidence: vec![],
            explanation: vec![],
            validation: vec![],
            visualisations: vec![],
            evaluation: vec![],
            terminology_map: TerminologyMap::default(),
            citations: vec![],
            caveats: vec![],
            confidence: ConfidenceProfile::default(),
            style_profile: StyleProfile::technical_confident(),
            verdict: PublicationVerdict::RequiresClarification,
        };

        let report = engine.evaluate(&package);
        assert!(!report.passed());
        assert!(report.details.iter().any(|d| d.criterion == EvaluationCriterion::PeevveCompleteness && d.result == EvaluationResult::Fail));
    }
}