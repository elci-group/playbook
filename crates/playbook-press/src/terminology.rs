//! Terminology Engine

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::demystify::{TermMapping, TranslationAction, TermCategory, TermEntry, TerminologyMap};
use crate::{AudienceId, AudienceProfile};

/// Terminology engine for managing term translations
pub struct TerminologyEngine {
    mappings: HashMap<String, TermMapping>,
    first_use_tracker: HashMap<String, std::collections::HashSet<String>>,
}

impl TerminologyEngine {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
            first_use_tracker: HashMap::new(),
        }
    }

    pub fn load_map(&mut self, map: TerminologyMap) {
        for mapping in map.iter() {
            self.mappings.insert(mapping.internal_term.clone(), mapping.clone());
        }
    }

    /// Translate text for a given audience
    pub fn translate(&mut self, text: &str, audience: &AudienceProfile) -> String {
        let mut result = text.to_string();
        let first_use_set = self.first_use_tracker.entry(audience.id.0.clone()).or_default();

        for mapping in self.mappings.values() {
            if !mapping.audience_restrictions.is_empty()
                && !mapping.audience_restrictions.contains(&audience.id) {
                continue;
            }

            let should_define = matches!(mapping.action, TranslationAction::Define)
                && !first_use_set.contains(&mapping.internal_term);

            if matches!(mapping.action, TranslationAction::Translate)
                || (should_define && first_use_set.insert(mapping.internal_term.clone())) {
                if should_define {
                    if let Some(def) = &mapping.definition {
                        let replacement = format!("{} ({})", mapping.internal_term, def);
                        result = result.replace(&mapping.internal_term, &replacement);
                    }
                } else {
                    result = result.replace(&mapping.internal_term, &mapping.external_term);
                }
            }
        }

        result
    }

    /// Get mapping for a term
    pub fn get_mapping(&self, internal_term: &str) -> Option<&TermMapping> {
        self.mappings.get(internal_term)
    }

    /// Add a mapping
    pub fn add_mapping(&mut self, mapping: TermMapping) {
        self.mappings.insert(mapping.internal_term.clone(), mapping);
    }

    /// Reset first-use tracking for an audience
    pub fn reset_first_use(&mut self, audience: &AudienceId) {
        self.first_use_tracker.remove(&audience.0);
    }
}

impl Default for TerminologyEngine {
    fn default() -> Self {
        let mut engine = Self::new();
        engine.load_map(TerminologyMap::playbook_standard());
        engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminology_engine_creation() {
        let engine = TerminologyEngine::default();
        assert!(!engine.mappings.is_empty());
    }

    #[test]
    fn terminology_translation() {
        let mut engine = TerminologyEngine::default();
        let audience = AudienceProfile::general_public();

        let input = "The model is on the injury list and will not run the offensive drive.";
        let output = engine.translate(input, &audience);

        assert!(output.contains("temporarily unavailable inference route"));
        assert!(output.contains("progression-focused execution sequence"));
    }

    #[test]
    fn first_use_definition() {
        let mut engine = TerminologyEngine::default();
        let audience = AudienceProfile::general_public();

        let input = "PEEVVE is our framework. PEEVVE stands for Point-Evidence-Explanation-Validation-Visualisation-Evaluation.";
        let output = engine.translate(input, &audience);

        assert!(output.contains("Point-Evidence-Explanation-Validation-Visualisation-Evaluation"));
    }
}