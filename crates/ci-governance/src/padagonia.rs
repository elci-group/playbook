//! Padagonia (§20) — the epistemic history graph.
//!
//! Assertions of fact about Playbook, related to evidence by edge kind.
//!
//! # Immutable-evidence rule
//!
//! The graph stores only REFERENCES (strings naming evidence
//! envelopes, e.g. a revision commit or evidence digest). It holds no
//! field capable of carrying an evidence payload: there is no envelope
//! field, no gate records, no revision freezes anywhere in this
//! module. The graph records what we believed and which evidence
//! bears on it; the evidence itself lives solely in
//! `ci_evidence::CiEvidence` envelopes. The graph is never a
//! replacement for that evidence — only an index over it.

use serde::{Deserialize, Serialize};

/// The identity of an assertion. An opaque string wrapper.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AssertionId(pub String);

impl AssertionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An assertion of fact — a claim the organisation makes about the
/// system, which evidence may support, contradict, corroborate, or
/// supersede.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub id: AssertionId,
    pub text: String,
}

/// How one piece of evidence (by reference) relates to an assertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// The evidence supports the assertion.
    SupportedBy,
    /// The evidence contradicts the assertion.
    ContradictedBy,
    /// The assertion has been replaced by another understanding.
    SupersededBy,
    /// Independent evidence points the same way as existing support.
    CorroboratedBy,
}

/// A relationship between an assertion and a piece of evidence — by
/// reference only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    pub from: AssertionId,
    /// A reference to an evidence envelope (never the envelope itself).
    pub to_evidence: String,
    pub kind: EdgeKind,
}

/// The epistemic status of an assertion, derived from its edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    /// Support (or corroboration) and no contradiction or supersession.
    Supported,
    /// At least one contradicting evidence reference.
    Contradicted,
    /// The assertion has been superseded — this outranks everything.
    Superseded,
    /// Simultaneously supported/corroborated AND contradicted.
    Contested,
}

/// The epistemic history graph. Stores assertions and evidence
/// *references* only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpistemicGraph {
    assertions: Vec<Assertion>,
    relations: Vec<Relation>,
}

impl EpistemicGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new assertion and return its identity.
    pub fn assert(&mut self, text: impl Into<String>) -> AssertionId {
        let id = AssertionId::new(format!("assertion-{}", self.assertions.len() + 1));
        self.assertions.push(Assertion {
            id: id.clone(),
            text: text.into(),
        });
        id
    }

    /// Relate an assertion to evidence by reference. No-op if the
    /// assertion is unknown.
    pub fn relate(&mut self, from: AssertionId, evidence_ref: impl Into<String>, kind: EdgeKind) {
        if !self.assertions.iter().any(|a| a.id == from) {
            return;
        }
        self.relations.push(Relation {
            from,
            to_evidence: evidence_ref.into(),
            kind,
        });
    }

    /// The assertion's text, if present.
    pub fn assertion_text(&self, id: &AssertionId) -> Option<&str> {
        self.assertions
            .iter()
            .find(|a| &a.id == id)
            .map(|a| a.text.as_str())
    }

    /// All assertions that carry at least one `ContradictedBy` edge.
    pub fn contradictions(&self) -> Vec<(&AssertionId, &String)> {
        self.relations
            .iter()
            .filter(|relation| relation.kind == EdgeKind::ContradictedBy)
            .map(|relation| (&relation.from, &relation.to_evidence))
            .collect()
    }

    /// Derive the assertion's status mechanically:
    /// `Superseded` > `Contested` > `Contradicted` > `Supported`;
    /// unknown assertions report `Supported` (no evidence against).
    pub fn status(&self, id: &AssertionId) -> AssertionStatus {
        let mut superseded = false;
        let mut contradicted = false;
        let mut supported = false;
        for relation in &self.relations {
            if &relation.from != id {
                continue;
            }
            match relation.kind {
                EdgeKind::SupersededBy => superseded = true,
                EdgeKind::ContradictedBy => contradicted = true,
                EdgeKind::SupportedBy | EdgeKind::CorroboratedBy => supported = true,
            }
        }
        if superseded {
            AssertionStatus::Superseded
        } else if contradicted && supported {
            AssertionStatus::Contested
        } else if contradicted {
            AssertionStatus::Contradicted
        } else {
            AssertionStatus::Supported
        }
    }
}
