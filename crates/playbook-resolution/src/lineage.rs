//! Remediation lineage (§40): every retry and repair retains ancestry, so
//! the recovery tree is reconstructable from the record alone (§37).

use serde::{Deserialize, Serialize};

use playbook_core::sha256_hex;

/// One node of the recovery tree: an attempt, the refusal it met, the
/// resolution chosen, and the child attempt it produced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptNode {
    pub attempt_id: String,
    /// The attempt this one descends from, if any.
    pub parent: Option<String>,
    /// The resolution action taken after this attempt's refusal.
    pub resolution: Option<String>,
    /// The refusal digest, when this attempt was refused.
    pub refusal_digest: Option<String>,
    pub at_ms: u64,
}

/// The append-only recovery tree (§40).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionLineage {
    attempts: Vec<AttemptNode>,
}

impl ResolutionLineage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record the root attempt.
    pub fn root(&mut self, attempt_id: &str, at_ms: u64) {
        self.attempts.push(AttemptNode {
            attempt_id: attempt_id.into(),
            parent: None,
            resolution: None,
            refusal_digest: None,
            at_ms,
        });
    }

    /// Record a refusal on an attempt and the resolution chosen for it,
    /// producing the child attempt id (deterministic from the lineage).
    pub fn refuse(
        &mut self,
        attempt_id: &str,
        refusal_digest: &str,
        resolution: &str,
    ) -> Option<String> {
        let position = self
            .attempts
            .iter()
            .position(|n| n.attempt_id == attempt_id)?;
        let at_ms = self.attempts[position].at_ms + 1;
        let node = &mut self.attempts[position];
        node.refusal_digest = Some(refusal_digest.to_string());
        node.resolution = Some(resolution.to_string());
        let child = format!(
            "{}+{}",
            attempt_id,
            &sha256_hex(format!("{attempt_id}:{refusal_digest}:{resolution}").as_bytes())[..8]
        );
        self.attempts.push(AttemptNode {
            attempt_id: child.clone(),
            parent: Some(attempt_id.to_string()),
            resolution: None,
            refusal_digest: None,
            at_ms,
        });
        Some(child)
    }

    pub fn attempts(&self) -> &[AttemptNode] {
        &self.attempts
    }

    /// The depth of the longest chain — the loop guard's raw material.
    pub fn depth(&self) -> u32 {
        fn chain(nodes: &[AttemptNode], id: &str) -> u32 {
            let node = nodes.iter().find(|n| n.attempt_id == id);
            match node.and_then(|n| n.parent.clone()) {
                Some(parent) => 1 + chain(nodes, &parent),
                None => 1,
            }
        }
        self.attempts
            .iter()
            .map(|n| chain(&self.attempts, &n.attempt_id))
            .max()
            .unwrap_or(0)
    }
}
