//! Claim Strength Taxonomy and Anti-Inflation Policy

use serde::{Deserialize, Serialize};
use std::fmt;

/// Canonical epistemic statuses for external claims
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStrength {
    /// Directly observed
    Observed,
    /// Measured with instruments/methodology
    Measured,
    /// Validated through independent authority
    Validated,
    /// Replicated independently
    Replicated,
    /// Inferred from evidence
    Inferred,
    /// Projected from current data
    Projected,
    /// Speculative/hypothetical
    Speculative,
}

impl ClaimStrength {
    /// Returns the ordinal rank (higher = stronger)
    pub fn rank(self) -> u8 {
        match self {
            Self::Speculative => 1,
            Self::Projected => 2,
            Self::Inferred => 3,
            Self::Observed => 4,
            Self::Measured => 5,
            Self::Validated => 6,
            Self::Replicated => 7,
        }
    }

    /// Check if this strength is at least as strong as another
    pub fn at_least(self, other: Self) -> bool {
        self.rank() >= other.rank()
    }

    /// Check if upgrading from one to another is permitted
    pub fn can_upgrade_to(self, target: Self) -> bool {
        // Can only upgrade with additional evidence/validation
        matches!((self, target), (Self::Observed, Self::Measured) | (Self::Measured, Self::Validated) | (Self::Validated, Self::Replicated))
    }

    /// Forbidden transformations
    pub fn is_forbidden_transformation(from: Self, to: Self) -> bool {
        matches!((from, to),
            (Self::Measured, Self::Validated) |
            (Self::Projected, Self::Observed) |
            (Self::Inferred, Self::Validated) |
            (Self::Speculative, Self::Measured)
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Observed => "OBSERVED",
            Self::Measured => "MEASURED",
            Self::Validated => "VALIDATED",
            Self::Replicated => "REPLICATED",
            Self::Inferred => "INFERRED",
            Self::Projected => "PROJECTED",
            Self::Speculative => "SPECULATIVE",
        }
    }
}

impl fmt::Display for ClaimStrength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Error for invalid claim strength operations
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClaimStrengthError {
    #[error("forbidden claim strength transformation: {from} -> {to}")]
    ForbiddenTransformation { from: ClaimStrength, to: ClaimStrength },
    #[error("insufficient evidence for claim strength upgrade")]
    InsufficientEvidence,
    #[error("invalid claim strength: {0}")]
    Invalid(String),
}

impl ClaimStrength {
    /// Attempt to upgrade claim strength with evidence check
    pub fn try_upgrade(self, target: Self, has_evidence: bool) -> Result<Self, ClaimStrengthError> {
        if Self::is_forbidden_transformation(self, target) {
            return Err(ClaimStrengthError::ForbiddenTransformation { from: self, to: target });
        }
        if !self.can_upgrade_to(target) {
            return Err(ClaimStrengthError::InsufficientEvidence);
        }
        if !has_evidence {
            return Err(ClaimStrengthError::InsufficientEvidence);
        }
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_strength_ranking() {
        assert!(ClaimStrength::Replicated.rank() > ClaimStrength::Validated.rank());
        assert!(ClaimStrength::Validated.rank() > ClaimStrength::Measured.rank());
        assert!(ClaimStrength::Measured.rank() > ClaimStrength::Observed.rank());
        assert!(ClaimStrength::Observed.rank() > ClaimStrength::Inferred.rank());
        assert!(ClaimStrength::Inferred.rank() > ClaimStrength::Projected.rank());
        assert!(ClaimStrength::Projected.rank() > ClaimStrength::Speculative.rank());
    }

    #[test]
    fn forbidden_transformations() {
        assert!(ClaimStrength::is_forbidden_transformation(ClaimStrength::Measured, ClaimStrength::Validated));
        assert!(ClaimStrength::is_forbidden_transformation(ClaimStrength::Projected, ClaimStrength::Observed));
        assert!(ClaimStrength::is_forbidden_transformation(ClaimStrength::Inferred, ClaimStrength::Validated));
        assert!(ClaimStrength::is_forbidden_transformation(ClaimStrength::Speculative, ClaimStrength::Measured));
    }

    #[test]
    fn permitted_upgrades() {
        assert!(ClaimStrength::Observed.can_upgrade_to(ClaimStrength::Measured));
        assert!(ClaimStrength::Measured.can_upgrade_to(ClaimStrength::Validated));
        assert!(ClaimStrength::Validated.can_upgrade_to(ClaimStrength::Replicated));
    }

    #[test]
    fn upgrade_requires_evidence() {
        let result = ClaimStrength::Observed.try_upgrade(ClaimStrength::Measured, false);
        assert!(result.is_err());
        let result = ClaimStrength::Observed.try_upgrade(ClaimStrength::Measured, true);
        assert!(result.is_ok());
    }

    #[test]
    fn anti_inflation_examples() {
        // 8/8 on frozen corpus ≠ 100% accurate generally
        let corpus_result = ClaimStrength::Measured;
        let general_claim = ClaimStrength::Validated;
        assert!(ClaimStrength::is_forbidden_transformation(corpus_result, general_claim));

        // Prototype passed acceptance test ≠ production ready
        let prototype = ClaimStrength::Observed;
        let production = ClaimStrength::Validated;
        assert!(ClaimStrength::is_forbidden_transformation(prototype, production));

        // Lowest latency among tested ≠ fastest available
        let tested = ClaimStrength::Measured;
        let absolute = ClaimStrength::Validated;
        assert!(ClaimStrength::is_forbidden_transformation(tested, absolute));

        // Current evidence supports ≠ proves universally
        let supports = ClaimStrength::Inferred;
        let proves = ClaimStrength::Validated;
        assert!(ClaimStrength::is_forbidden_transformation(supports, proves));
    }
}