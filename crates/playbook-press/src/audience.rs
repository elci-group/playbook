//! Audience Profiles

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for an audience profile
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AudienceId(pub String);

impl AudienceId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl From<&str> for AudienceId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for AudienceId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for AudienceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Constraints for a specific audience
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudienceConstraints {
    /// Jargon policy
    pub jargon: JargonPolicy,
    /// Maximum sentence complexity (1-5)
    pub sentence_complexity: u8,
    /// Evidence density preference
    pub evidence_density: EvidenceDensity,
    /// Visual density preference
    pub visual_density: VisualDensity,
    /// Citation density preference
    pub citation_density: CitationDensity,
    /// Technical depth (1-5)
    pub technical_depth: u8,
    /// Metaphor tolerance
    pub metaphor_tolerance: MetaphorTolerance,
    /// Uncertainty language style
    pub uncertainty_language: UncertaintyLanguage,
    /// Verbosity level
    pub verbosity: VerbosityLevel,
    /// Include recommendations
    pub recommendations: bool,
    /// Tone profile
    pub tone: Tone,
}

/// Jargon handling policy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JargonPolicy {
    /// Preserve all internal terminology
    Preserve,
    /// Define on first use
    DefineFirstUse,
    /// Translate to external terms
    Translate,
    /// Remove jargon entirely
    Remove,
}

/// Evidence density preference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDensity {
    Minimal,
    Standard,
    High,
    Forensic,
}

/// Visual density preference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualDensity {
    None,
    Sparse,
    Standard,
    Rich,
}

/// Citation density preference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationDensity {
    None,
    MaterialClaims,
    AllClaims,
    Forensic,
}

/// Metaphor tolerance
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaphorTolerance {
    None,
    DefineFirstUse,
    Preserve,
    Encourage,
}

/// Uncertainty language style
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyLanguage {
    Mandatory,
    Explicit,
    Implicit,
    Minimal,
}

/// Verbosity level
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerbosityLevel {
    Minimal,
    Concise,
    Standard,
    Verbose,
    Forensic,
}

/// Tone profile
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    Technical,
    Confident,
    Cautious,
    Neutral,
    Authoritative,
    Conversational,
    Formal,
}

/// Audience profile with constraints
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudienceProfile {
    pub id: AudienceId,
    pub name: String,
    pub constraints: AudienceConstraints,
}

impl AudienceProfile {
    /// Internal technical audience (Playbook developers)
    pub fn internal_technical() -> Self {
        Self {
            id: AudienceId::new("internal_technical"),
            name: "Internal Technical".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Preserve,
                sentence_complexity: 5,
                evidence_density: EvidenceDensity::High,
                visual_density: VisualDensity::Rich,
                citation_density: CitationDensity::Forensic,
                technical_depth: 5,
                metaphor_tolerance: MetaphorTolerance::Preserve,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Standard,
                recommendations: true,
                tone: Tone::Technical,
            },
        }
    }

    /// External technical audience (developers, engineers)
    pub fn technical() -> Self {
        Self {
            id: AudienceId::new("external_technical"),
            name: "External Technical".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::DefineFirstUse,
                sentence_complexity: 4,
                evidence_density: EvidenceDensity::High,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::AllClaims,
                technical_depth: 4,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Standard,
                recommendations: true,
                tone: Tone::Technical,
            },
        }
    }

    /// Developer audience
    pub fn developer() -> Self {
        Self {
            id: AudienceId::new("developer"),
            name: "Developer".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::DefineFirstUse,
                sentence_complexity: 4,
                evidence_density: EvidenceDensity::Standard,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 4,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Concise,
                recommendations: true,
                tone: Tone::Technical,
            },
        }
    }

    /// Executive audience
    pub fn executive() -> Self {
        Self {
            id: AudienceId::new("executive"),
            name: "Executive".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Translate,
                sentence_complexity: 3,
                evidence_density: EvidenceDensity::Standard,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 2,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Concise,
                recommendations: true,
                tone: Tone::Authoritative,
            },
        }
    }

    /// Academic audience
    pub fn academic() -> Self {
        Self {
            id: AudienceId::new("academic"),
            name: "Academic".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::DefineFirstUse,
                sentence_complexity: 5,
                evidence_density: EvidenceDensity::High,
                visual_density: VisualDensity::Rich,
                citation_density: CitationDensity::AllClaims,
                technical_depth: 5,
                metaphor_tolerance: MetaphorTolerance::None,
                uncertainty_language: UncertaintyLanguage::Mandatory,
                verbosity: VerbosityLevel::Verbose,
                recommendations: false,
                tone: Tone::Neutral,
            },
        }
    }

    /// Investor audience
    pub fn investor() -> Self {
        Self {
            id: AudienceId::new("investor"),
            name: "Investor".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Translate,
                sentence_complexity: 3,
                evidence_density: EvidenceDensity::Standard,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 2,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Concise,
                recommendations: true,
                tone: Tone::Confident,
            },
        }
    }

    /// Customer audience
    pub fn customer() -> Self {
        Self {
            id: AudienceId::new("customer"),
            name: "Customer".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Translate,
                sentence_complexity: 3,
                evidence_density: EvidenceDensity::Standard,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 2,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Standard,
                recommendations: true,
                tone: Tone::Confident,
            },
        }
    }

    /// Legal/audit audience
    pub fn legal() -> Self {
        Self {
            id: AudienceId::new("legal"),
            name: "Legal/Audit".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Preserve,
                sentence_complexity: 5,
                evidence_density: EvidenceDensity::Forensic,
                visual_density: VisualDensity::Sparse,
                citation_density: CitationDensity::Forensic,
                technical_depth: 5,
                metaphor_tolerance: MetaphorTolerance::None,
                uncertainty_language: UncertaintyLanguage::Mandatory,
                verbosity: VerbosityLevel::Forensic,
                recommendations: false,
                tone: Tone::Formal,
            },
        }
    }

    /// Audit audience
    pub fn audit() -> Self {
        Self::legal()
    }

    /// Incident communication
    pub fn incident() -> Self {
        Self {
            id: AudienceId::new("incident"),
            name: "Incident".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Translate,
                sentence_complexity: 3,
                evidence_density: EvidenceDensity::High,
                visual_density: VisualDensity::Sparse,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 3,
                metaphor_tolerance: MetaphorTolerance::None,
                uncertainty_language: UncertaintyLanguage::Mandatory,
                verbosity: VerbosityLevel::Concise,
                recommendations: false,
                tone: Tone::Cautious,
            },
        }
    }

    /// General public audience
    pub fn general_public() -> Self {
        Self {
            id: AudienceId::new("general_public"),
            name: "General Public".into(),
            constraints: AudienceConstraints {
                jargon: JargonPolicy::Translate,
                sentence_complexity: 2,
                evidence_density: EvidenceDensity::Standard,
                visual_density: VisualDensity::Standard,
                citation_density: CitationDensity::MaterialClaims,
                technical_depth: 1,
                metaphor_tolerance: MetaphorTolerance::DefineFirstUse,
                uncertainty_language: UncertaintyLanguage::Explicit,
                verbosity: VerbosityLevel::Standard,
                recommendations: false,
                tone: Tone::Neutral,
            },
        }
    }

    /// Get constraint value
    pub fn constraints(&self) -> &AudienceConstraints {
        &self.constraints
    }
}

impl Default for AudienceProfile {
    fn default() -> Self {
        Self::technical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_profiles_created() {
        let tech = AudienceProfile::technical();
        assert_eq!(tech.id.0, "external_technical");
        assert_eq!(tech.constraints.jargon, JargonPolicy::DefineFirstUse);

        let exec = AudienceProfile::executive();
        assert_eq!(exec.constraints.jargon, JargonPolicy::Translate);
        assert_eq!(exec.constraints.technical_depth, 2);
    }

    #[test]
    fn default_is_technical() {
        let default = AudienceProfile::default();
        assert_eq!(default.id, AudienceId::new("external_technical"));
    }
}