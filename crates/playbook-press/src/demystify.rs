//! Demystification Engine and Terminology Translation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::AudienceId;

/// Term mapping between internal and external terminology
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermMapping {
    pub internal_term: String,
    pub external_term: String,
    pub definition: Option<String>,
    pub action: TranslationAction,
    pub category: TermCategory,
    pub audience_restrictions: Vec<AudienceId>,
}

/// Action to take when encountering a term
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationAction {
    /// Keep the internal term as-is
    Keep,
    /// Define the term on first use
    Define,
    /// Translate to external terminology
    Translate,
    /// Remove the term entirely
    Remove,
}

/// Category of terminology
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermCategory {
    FootballMetaphor,
    TechnicalJargon,
    InternalTerm,
    Acronym,
    DomainSpecific,
    PolicyTerm,
}

/// Term entry for terminology engine
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermEntry {
    pub internal_term: String,
    pub external_term: String,
    pub definition: Option<String>,
    pub action: TranslationAction,
    pub category: TermCategory,
    pub audience_restrictions: Vec<AudienceId>,
}

/// Collection of terminology mappings
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminologyMap {
    mappings: std::collections::HashMap<String, TermMapping>,
}

impl TerminologyMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, mapping: TermMapping) {
        self.mappings.insert(mapping.internal_term.clone(), mapping);
    }

    pub fn get(&self, internal_term: &str) -> Option<&TermMapping> {
        self.mappings.get(internal_term)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TermMapping> {
        self.mappings.values()
    }

    /// Build the standard Playbook terminology map
    pub fn playbook_standard() -> Self {
        let mut map = Self::new();
        map.add(TermMapping {
            internal_term: "injury list".into(),
            external_term: "temporarily unavailable inference route".into(),
            definition: Some("A route that remains usable but whose workload should temporarily be reduced".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "recovery list".into(),
            external_term: "capacity-constrained route under workload restriction".into(),
            definition: Some("A route that is usable but operating under reduced workload capacity".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "suspension".into(),
            external_term: "provider/model excluded by policy".into(),
            definition: Some("A route that has been excluded by policy decision".into()),
            action: TranslationAction::Translate,
            category: TermCategory::PolicyTerm,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "offensive drive".into(),
            external_term: "progression-focused execution sequence".into(),
            definition: Some("A sequence of operations aimed at advancing work toward completion".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "defensive snap".into(),
            external_term: "review or verification operation".into(),
            definition: Some("An operation aimed at validating or checking work".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "red zone".into(),
            external_term: "state in which task completion is operationally close".into(),
            definition: Some("A state where the work is operationally near completion".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "yards to go".into(),
            external_term: "remaining evidence-backed work".into(),
            definition: Some("The remaining evidence-backed work needed to complete the task".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "kicker".into(),
            external_term: "bounded high-leverage completion attempt".into(),
            definition: Some("A bounded, high-leverage attempt to complete work".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec![],
        });
        map.add(TermMapping {
            internal_term: "RRP".into(),
            external_term: "Refusal Resolution Plane".into(),
            definition: Some("Playbook's subsystem for classifying and repairing refused work".into()),
            action: TranslationAction::Define,
            category: TermCategory::Acronym,
            audience_restrictions: vec!["general_public".into()],
        });
        map.add(TermMapping {
            internal_term: "PEEVVE".into(),
            external_term: "Point-Evidence-Explanation-Validation-Visualisation-Evaluation".into(),
            definition: Some("Playbook's canonical human-explanation framework".into()),
            action: TranslationAction::Define,
            category: TermCategory::Acronym,
            audience_restrictions: vec!["general_public".into(), "developer".into()],
        });
        map.add(TermMapping {
            internal_term: "Lastdown".into(),
            external_term: "possession-transfer runtime".into(),
            definition: Some("Playbook's runtime for managing possession transfer between work units".into()),
            action: TranslationAction::Define,
            category: TermCategory::InternalTerm,
            audience_restrictions: vec!["general_public".into()],
        });
        map.add(TermMapping {
            internal_term: "front office".into(),
            external_term: "strategic planning and roster management authority".into(),
            definition: Some("Playbook's strategic planning layer".into()),
            action: TranslationAction::Translate,
            category: TermCategory::FootballMetaphor,
            audience_restrictions: vec!["general_public".into(), "developer".into()],
        });
        map
    }
}

/// Demystification engine for translating internal terminology
pub struct DemystificationEngine {
    terminology_map: TerminologyMap,
    first_use_definitions: std::collections::HashSet<String>,
}

impl DemystificationEngine {
    pub fn new(terminology_map: TerminologyMap) -> Self {
        Self {
            terminology_map,
            first_use_definitions: std::collections::HashSet::new(),
        }
    }

    /// Apply demystification to text for a given audience
    pub fn demystify(&mut self, text: &str, audience: &crate::AudienceProfile) -> String {
        let mut result = text.to_string();

        // Apply terminology mappings
        for mapping in self.terminology_map.iter() {
            let action = mapping.action;
            let should_apply = mapping.audience_restrictions.is_empty()
                || mapping.audience_restrictions.contains(&audience.id);

            if !should_apply {
                continue;
            }

            match action {
                TranslationAction::Keep => {
                    // Keep internal term as-is
                }
                TranslationAction::Define => {
                    if !self.first_use_definitions.contains(&mapping.internal_term) {
                        if let Some(def) = &mapping.definition {
                            let replacement = format!("{} ({})", mapping.internal_term, def);
                            result = result.replace(&mapping.internal_term, &replacement);
                            self.first_use_definitions.insert(mapping.internal_term.clone());
                        }
                    }
                }
                TranslationAction::Translate => {
                    result = result.replace(&mapping.internal_term, &mapping.external_term);
                }
                TranslationAction::Remove => {
                    result = result.replace(&mapping.internal_term, "");
                }
            }
        }

        result
    }

    /// Reset first-use tracking for a new document
    pub fn reset_first_use(&mut self) {
        self.first_use_definitions.clear();
    }
}

impl Default for DemystificationEngine {
    fn default() -> Self {
        Self::new(TerminologyMap::playbook_standard())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demystification_translation() {
        let mut engine = DemystificationEngine::default();
        let audience = crate::AudienceProfile::general_public();

        let input = "The model entered the injury list and will not run the offensive drive.";
        let output = engine.demystify(input, &audience);

        assert!(output.contains("temporarily unavailable inference route"));
        assert!(output.contains("progression-focused execution sequence"));
    }

    #[test]
    fn first_use_definition() {
        let mut engine = DemystificationEngine::default();
        let audience = crate::AudienceProfile::general_public();

        let input = "PEEVVE is our framework. PEEVVE stands for Point-Evidence-Explanation-Validation-Visualisation-Evaluation.";
        let output = engine.demystify(input, &audience);

        // First occurrence should be defined
        assert!(output.contains("Point-Evidence-Explanation-Validation-Visualisation-Evaluation"));
        // Second occurrence should use just the acronym
    }

    #[test]
    fn audience_restriction() {
        let mut engine = DemystificationEngine::default();
        let technical = crate::AudienceProfile::technical();
        let general = crate::AudienceProfile::general_public();

        let input = "The model is on the injury list.";

        let tech_output = engine.demystify(input, &technical);
        let general_output = engine.demystify(input, &general);

        // Technical audience may keep football terms
        // General audience should translate
    }
}