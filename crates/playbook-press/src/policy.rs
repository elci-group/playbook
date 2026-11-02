//! Press Policy Configuration

use serde::{Deserialize, Serialize};

/// Complete press policy for a franchise
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressPolicy {
    pub franchise_id: String,
    pub default_audience: crate::AudienceProfile,
    pub default_style: crate::StyleProfile,
    pub default_channel: crate::PublicationChannel,
    pub default_detail: crate::DetailLevel,
    pub visualisation_policy: VisualisationPolicy,
    pub marketing_inflation_policy: MarketingInflationPolicy,
    pub unsupported_claims_policy: UnsupportedClaimsPolicy,
    pub footnotes_on_first_use: bool,
    pub citation_style: CitationStyle,
    pub disclaimer_template: Option<String>,
}

impl PressPolicy {
    pub fn default() -> Self {
        Self {
            franchise_id: "default".into(),
            default_audience: crate::AudienceProfile::technical(),
            default_style: crate::StyleProfile::technical_confident(),
            default_channel: crate::PublicationChannel::Report,
            default_detail: crate::DetailLevel::Standard,
            visualisation_policy: VisualisationPolicy::default(),
            marketing_inflation_policy: MarketingInflationPolicy::Forbidden,
            unsupported_claims_policy: UnsupportedClaimsPolicy::Withhold,
            footnotes_on_first_use: true,
            citation_style: CitationStyle::default(),
            disclaimer_template: None,
        }
    }

    pub fn defensive_fortress() -> Self {
        let mut policy = Self::default();
        policy.default_audience = crate::AudienceProfile::technical();
        policy.default_style = crate::StyleProfile::technical_confident();
        policy.visualisation_policy = VisualisationPolicy::WhenUseful;
        policy.unsupported_claims_policy = crate::UnsupportedClaimsPolicy::Reject;
        policy
    }

    pub fn air_raid() -> Self {
        let mut policy = Self::default();
        policy.default_audience = crate::AudienceProfile::executive();
        policy.default_style = crate::StyleProfile::executive_confident();
        policy.visualisation_policy = VisualisationPolicy::WhenUseful;
        policy.marketing_inflation_policy = MarketingInflationPolicy::Flagged;
        policy
    }

    pub fn ball_control() -> Self {
        let mut policy = Self::default();
        policy.default_audience = crate::AudienceProfile::technical();
        policy.default_style = crate::StyleProfile::technical_confident();
        policy.visualisation_policy = VisualisationPolicy::Preferred;
        policy.unsupported_claims_policy = crate::UnsupportedClaimsPolicy::Withhold;
        policy
    }
}

/// Visualisation policy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualisationPolicy {
    None,
    WhenUseful,
    Preferred,
    Required,
}

impl Default for VisualisationPolicy {
    fn default() -> Self {
        Self::WhenUseful
    }
}

/// Marketing inflation policy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingInflationPolicy {
    Forbidden,
    Flagged,
    Permitted,
}

impl Default for MarketingInflationPolicy {
    fn default() -> Self {
        Self::Forbidden
    }
}

/// Unsupported claims policy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsupportedClaimsPolicy {
    Withhold,
    Flag,
    Reject,
}

impl Default for UnsupportedClaimsPolicy {
    fn default() -> Self {
        Self::Withhold
    }
}

/// Citation style
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStyle {
    Numeric,
    AuthorDate,
    Footnote,
    Bracket,
}

impl Default for CitationStyle {
    fn default() -> Self {
        Self::Numeric
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_presets() {
        let default = PressPolicy::default();
        assert_eq!(default.marketing_inflation_policy, MarketingInflationPolicy::Forbidden);

        let fortress = PressPolicy::defensive_fortress();
        assert_eq!(fortress.unsupported_claims_policy, crate::UnsupportedClaimsPolicy::Reject);

        let air_raid = PressPolicy::air_raid();
        assert_eq!(air_raid.marketing_inflation_policy, MarketingInflationPolicy::Flagged);

        let ball = PressPolicy::ball_control();
        assert_eq!(ball.visualisation_policy, VisualisationPolicy::Preferred);
    }
}