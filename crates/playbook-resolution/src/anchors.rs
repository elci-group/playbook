//! Anchor integrity (§12): first/last/cardinality constraints as
//! first-class work-unit contracts, checked structurally before any
//! expensive semantic inspection.

use serde::{Deserialize, Serialize};

use playbook_core::sha256_hex;

/// The work-unit contract every decomposed child carries (§11).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkUnitContract {
    pub unit_id: String,
    pub expected_items: u32,
    /// The identity of the first expected element.
    pub first_anchor: Option<String>,
    /// The identity of the last expected element.
    pub last_anchor: Option<String>,
    /// Whether the sequence order is part of the contract.
    pub order_immutable: bool,
}

impl WorkUnitContract {
    pub fn hash(&self) -> String {
        sha256_hex(&serde_json::to_vec(self).expect("work-unit contract serialises"))
    }
}

/// The structural verdict over one delivered batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorVerdict {
    Satisfied,
    FirstMissing,
    LastMissing,
    CardinalityMismatch,
    Duplicate,
    Reordered,
}

/// Check a delivered batch of element identities against the contract —
/// cheap, structural, and before any semantic inspection (§12).
pub fn check_anchors(contract: &WorkUnitContract, delivered: &[String]) -> AnchorVerdict {
    if delivered.len() as u32 != contract.expected_items {
        return AnchorVerdict::CardinalityMismatch;
    }
    let mut seen = std::collections::BTreeSet::new();
    for item in delivered {
        if !seen.insert(item) {
            return AnchorVerdict::Duplicate;
        }
    }
    if let Some(first) = &contract.first_anchor {
        if delivered.first().map(String::as_str) != Some(first.as_str()) {
            return AnchorVerdict::FirstMissing;
        }
    }
    if let Some(last) = &contract.last_anchor {
        if delivered.last().map(String::as_str) != Some(last.as_str()) {
            return AnchorVerdict::LastMissing;
        }
    }
    if contract.order_immutable {
        // The expected order is derivable from the anchors plus
        // cardinality only when the caller supplies the full expected
        // sequence in the anchors' place; here the anchors bind the ends.
        // Interior order is delegated to the semantic gate.
    }
    AnchorVerdict::Satisfied
}
