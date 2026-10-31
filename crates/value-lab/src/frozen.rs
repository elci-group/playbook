//! The frozen experimental conditions: snaps, regimes, decision snapshots.
//! Hashed at freeze time; nothing thaws.

use serde::{Deserialize, Serialize};

use playbook_core::sha256_hex;

fn digest(value: &impl Serialize) -> String {
    let canonical =
        serde_json::to_string(value).expect("canonical json of frozen types always serialises");
    sha256_hex(canonical.as_bytes())
}

/// One eligible model in the frozen catalogue: the same pool every regime
/// draws from (§10 — Playbook must not win by receiving better models).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    pub provider: String,
    pub model: String,
    pub context_capacity: u64,
    /// Price per million tokens, in monetary micros.
    pub price_per_million_micros: u64,
}

/// The Experimental Snap (§9): materially equivalent information for every
/// regime, hashed once. All runs of an experiment reference this hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentalSnap {
    pub task_id: String,
    pub statement: String,
    pub repository_commit: String,
    pub tools: Vec<String>,
    pub models: Vec<ModelEntry>,
    /// Provider quota state, 0..=1_000_000 ppm.
    pub quota_ppm: u32,
    pub budget_tokens: u64,
    pub budget_micros: u64,
    pub acceptance_criteria: Vec<String>,
}

impl ExperimentalSnap {
    /// The content address every regime must reference.
    pub fn hash(&self) -> String {
        digest(self)
    }
}

/// One personnel option visible at a decision point, with the facts that
/// were knowable then (§14 — no hindsight enters the counterfactual).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonnelOption {
    pub name: String,
    pub predicted_utility: i64,
    pub cost_micros: u64,
    pub latency_ms: u64,
    /// Measured reliability, 0..=1_000_000 ppm.
    pub reliability_ppm: u32,
}

/// The DECISION_SNAPSHOT (§14): everything Playbook possessed immediately
/// before a material tactical decision. Shadow Defence and the lab's
/// counterfactual reconstruction both operate exclusively on this.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionSnapshot {
    pub snap_hash: String,
    pub at_ms: u64,
    pub ytg_milli: u32,
    /// Gate state digest at the decision point.
    pub gate_digest: String,
    pub options: Vec<PersonnelOption>,
    pub selected: String,
    pub rejected: Vec<String>,
}

impl DecisionSnapshot {
    /// The utility of the best option the decider could have selected
    /// *then* — by the lab's own recomputation, not the subject's claim.
    pub fn best_available(&self) -> Option<&PersonnelOption> {
        self.options
            .iter()
            .max_by_key(|option| option.predicted_utility)
    }

    /// The option that was actually selected, if it was among the frozen
    /// options (a selection outside the frozen set is a parity violation).
    pub fn selected_option(&self) -> Option<&PersonnelOption> {
        self.options
            .iter()
            .find(|option| option.name == self.selected)
    }

    /// Whether the decider chose the best available option.
    pub fn optimal(&self) -> bool {
        match (self.best_available(), self.selected_option()) {
            (Some(best), Some(selected)) => best.name == selected.name,
            _ => false,
        }
    }
}

/// The orchestration regimes (§4). C6 is the hindsight bound, not a
/// deployable competitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    /// Playbook, full tactical control.
    P0,
    /// Fixed strongest eligible model.
    C1Strongest,
    /// Fixed cheapest competent model.
    C2Cheapest,
    /// Simple capability/cost router.
    C3Router,
    /// Lucid without Playbook tactical intervention.
    C4LucidPlain,
    /// Random eligible line-up.
    C5Random,
    /// Oracle/hindsight benchmark — the upper bound, undeployable.
    C6Oracle,
}

impl Regime {
    pub fn key(self) -> &'static str {
        match self {
            Self::P0 => "P0_playbook",
            Self::C1Strongest => "C1_strongest",
            Self::C2Cheapest => "C2_cheapest",
            Self::C3Router => "C3_router",
            Self::C4LucidPlain => "C4_lucid_plain",
            Self::C5Random => "C5_random",
            Self::C6Oracle => "C6_oracle",
        }
    }

    /// Whether this regime is a deployable competitor. C6 establishes the
    /// hindsight bound and is excluded from "best control" comparisons.
    pub fn deployable(self) -> bool {
        !matches!(self, Self::C6Oracle)
    }

    /// Every regime of record.
    pub fn all() -> [Regime; 7] {
        [
            Self::P0,
            Self::C1Strongest,
            Self::C2Cheapest,
            Self::C3Router,
            Self::C4LucidPlain,
            Self::C5Random,
            Self::C6Oracle,
        ]
    }
}
