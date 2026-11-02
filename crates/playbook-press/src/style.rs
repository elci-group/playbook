//! Style Profiles and Policy

use serde::{Deserialize, Serialize};

use crate::policy::{MarketingInflationPolicy, UnsupportedClaimsPolicy};
use crate::audience::{Tone, EvidenceDensity};

/// Style profile identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StyleProfileId(pub String);

impl StyleProfileId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

/// Complete style profile
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StyleProfile {
    pub id: StyleProfileId,
    pub name: String,
    pub tone: Tone,
    pub claim_strength_policy: ClaimStrengthPolicy,
    pub uncertainty_visibility: UncertaintyVisibility,
    pub metaphor_policy: MetaphorPolicy,
    pub evidence_density: EvidenceDensity,
    pub visualisation_preference: VisualisationPreference,
    pub marketing_inflation_policy: MarketingInflationPolicy,
    pub unsupported_claims_policy: UnsupportedClaimsPolicy,
}

impl StyleProfile {
    /// Technical confident style
    pub fn technical_confident() -> Self {
        Self {
            id: StyleProfileId::new("technical_confident"),
            name: "Technical Confident".into(),
            tone: Tone::Technical,
            claim_strength_policy: ClaimStrengthPolicy::EvidenceBound,
            uncertainty_visibility: UncertaintyVisibility::Mandatory,
            metaphor_policy: MetaphorPolicy::DefineFirstUse,
            evidence_density: EvidenceDensity::High,
            visualisation_preference: VisualisationPreference::Preferred,
            marketing_inflation_policy: MarketingInflationPolicy::Forbidden,
            unsupported_claims_policy: UnsupportedClaimsPolicy::Withhold,
        }
    }

    /// Executive confident style
    pub fn executive_confident() -> Self {
        Self {
            id: StyleProfileId::new("executive_confident"),
            name: "Executive Confident".into(),
            tone: Tone::Confident,
            claim_strength_policy: ClaimStrengthPolicy::EvidenceBound,
            uncertainty_visibility: UncertaintyVisibility::Explicit,
            metaphor_policy: MetaphorPolicy::DefineFirstUse,
            evidence_density: EvidenceDensity::Standard,
            visualisation_preference: VisualisationPreference::WhenUseful,
            marketing_inflation_policy: MarketingInflationPolicy::Forbidden,
            unsupported_claims_policy: UnsupportedClaimsPolicy::Withhold,
        }
    }

    /// Academic neutral style
    pub fn academic_neutral() -> Self {
        Self {
            id: StyleProfileId::new("academic_neutral"),
            name: "Academic Neutral".into(),
            tone: Tone::Neutral,
            claim_strength_policy: ClaimStrengthPolicy::Conservative,
            uncertainty_visibility: UncertaintyVisibility::Mandatory,
            metaphor_policy: MetaphorPolicy::None,
            evidence_density: EvidenceDensity::High,
            visualisation_preference: VisualisationPreference::Preferred,
            marketing_inflation_policy: MarketingInflationPolicy::Forbidden,
            unsupported_claims_policy: UnsupportedClaimsPolicy::Withhold,
        }
    }

    /// Legal exact style
    pub fn legal_exact() -> Self {
        Self {
            id: StyleProfileId::new("legal_exact"),
            name: "Legal Exact".into(),
            tone: Tone::Formal,
            claim_strength_policy: ClaimStrengthPolicy::Conservative,
            uncertainty_visibility: UncertaintyVisibility::Mandatory,
            metaphor_policy: MetaphorPolicy::None,
            evidence_density: EvidenceDensity::Forensic,
            visualisation_preference: VisualisationPreference::None,
            marketing_inflation_policy: MarketingInflationPolicy::Forbidden,
            unsupported_claims_policy: UnsupportedClaimsPolicy::Reject,
        }
    }
}

/// Policy for claim strength in communications
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStrengthPolicy {
    /// Bound to available evidence
    EvidenceBound,
    /// Conservative - one level below evidence
    Conservative,
    /// Permit optimistic framing
    Optimistic,
}

/// Uncertainty visibility requirement
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyVisibility {
    Mandatory,
    Explicit,
    Implicit,
    Minimal,
}

/// Metaphor handling policy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaphorPolicy {
    None,
    DefineFirstUse,
    Preserve,
    Encourage,
}

/// Visualisation preference
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualisationPreference {
    None,
    WhenUseful,
    Preferred,
    Required,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_profiles() {
        let tech = StyleProfile::technical_confident();
        assert_eq!(tech.claim_strength_policy, ClaimStrengthPolicy::EvidenceBound);
        assert_eq!(tech.uncertainty_visibility, UncertaintyVisibility::Mandatory);

        let exec = StyleProfile::executive_confident();
        assert_eq!(exec.tone, Tone::Confident);
    }
}
