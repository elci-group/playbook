//! The refusal-resolution projection (§36): the Matchday surface's view
//! of one diagnosis, its authorised prescription and the recovery
//! lineage. A pure projection with strict parity — every field is read
//! from `playbook-resolution` types; nothing here invents a UI-only
//! fact. The one exception is deliberate and load-bearing: the
//! acceptance standard is NEVER shown as changed unless an explicit
//! policy revision is recorded (§36 invariant — the type system
//! enforces it, see `for_policy_revision`).

use serde::{Deserialize, Serialize};

use playbook_resolution::classify::{Confidence, Diagnosis, RefusalFamily};
use playbook_resolution::lineage::ResolutionLineage;
use playbook_resolution::policy::Prescription;

/// The projected refusal-resolution state for one refusal under review.
/// Every field is a deterministic read of the authoritative resolution
/// types — same diagnosis, prescription and lineage always project the
/// same snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionSnapshot {
    refusal_key: &'static str,
    family_key: &'static str,
    confidence_key: &'static str,
    deterministic: bool,
    /// §36: always `false` from projection. There is no setter reachable
    /// from a diagnosis/prescription pair; only `for_policy_revision`
    /// can set it, and only with a recorded policy hash.
    acceptance_standard_changed: bool,
    actions: Vec<&'static str>,
    escalated: bool,
    lineage_depth: u32,
    rationale: String,
    /// The recorded policy hash, present only on a policy revision.
    policy_hash: Option<String>,
}

impl ResolutionSnapshot {
    /// Project one refusal-resolution state. Pure: the same inputs
    /// always yield the same snapshot, so a replay of the same journal
    /// projects the film-room panel identically. The acceptance
    /// standard is `false` by construction.
    pub fn from_resolution(
        diagnosis: &Diagnosis,
        prescription: &Prescription,
        lineage: &ResolutionLineage,
    ) -> Self {
        Self {
            refusal_key: diagnosis.primary.key(),
            family_key: family_key(diagnosis.primary.family()),
            confidence_key: confidence_key(diagnosis.confidence),
            deterministic: diagnosis.deterministic,
            acceptance_standard_changed: false,
            actions: prescription.actions.iter().map(|a| a.key()).collect(),
            escalated: prescription.escalated,
            lineage_depth: lineage.depth(),
            rationale: diagnosis.rationale.clone(),
            policy_hash: None,
        }
    }

    /// The policy-revision constructor (§36): the ONLY way to mark the
    /// acceptance standard changed. Demands the hash of the recorded
    /// policy revision that authorised it — no diagnosis, prescription
    /// or lineage can ever produce this state.
    pub fn for_policy_revision(
        diagnosis: &Diagnosis,
        prescription: &Prescription,
        lineage: &ResolutionLineage,
        policy_hash: String,
    ) -> Self {
        let mut snapshot = Self::from_resolution(diagnosis, prescription, lineage);
        snapshot.acceptance_standard_changed = true;
        snapshot.policy_hash = Some(policy_hash);
        snapshot
    }

    pub fn refusal_key(&self) -> &'static str {
        self.refusal_key
    }

    pub fn family_key(&self) -> &'static str {
        self.family_key
    }

    pub fn confidence_key(&self) -> &'static str {
        self.confidence_key
    }

    pub fn deterministic(&self) -> bool {
        self.deterministic
    }

    pub fn acceptance_standard_changed(&self) -> bool {
        self.acceptance_standard_changed
    }

    pub fn actions(&self) -> &[&'static str] {
        &self.actions
    }

    pub fn escalated(&self) -> bool {
        self.escalated
    }

    pub fn lineage_depth(&self) -> u32 {
        self.lineage_depth
    }

    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    pub fn policy_hash(&self) -> Option<&str> {
        self.policy_hash.as_deref()
    }
}

/// The wire form of a projection (for `--refusal <json>` review in the
/// film room). Deliberately carries NO acceptance-standard field: even a
/// hand-written snapshot file cannot assert a changed acceptance
/// standard (§36). `ResolutionSnapshot::from_parts` interns the strings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionParts {
    pub refusal_key: String,
    pub family_key: String,
    pub confidence_key: String,
    pub deterministic: bool,
    pub actions: Vec<String>,
    pub escalated: bool,
    pub lineage_depth: u32,
    pub rationale: String,
}

impl From<ProjectionParts> for ResolutionSnapshot {
    fn from(parts: ProjectionParts) -> Self {
        let intern = |value: String| -> &'static str { Box::leak(value.into_boxed_str()) };
        Self {
            refusal_key: intern(parts.refusal_key),
            family_key: intern(parts.family_key),
            confidence_key: intern(parts.confidence_key),
            deterministic: parts.deterministic,
            acceptance_standard_changed: false,
            actions: parts.actions.into_iter().map(intern).collect(),
            escalated: parts.escalated,
            lineage_depth: parts.lineage_depth,
            rationale: parts.rationale,
            policy_hash: None,
        }
    }
}

fn family_key(family: RefusalFamily) -> &'static str {
    match family {
        RefusalFamily::Execution => "execution",
        RefusalFamily::Capability => "capability",
        RefusalFamily::WorkProduct => "work_product",
        RefusalFamily::Evidence => "evidence",
        RefusalFamily::Governance => "governance",
    }
}

fn confidence_key(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Unknown => "unknown",
        Confidence::Low => "low",
        Confidence::Moderate => "moderate",
        Confidence::High => "high",
        Confidence::Certain => "certain",
    }
}

#[cfg(test)]
mod tests {
    use super::ResolutionSnapshot;
    use playbook_resolution::classify::{Confidence, Diagnosis, RefusalCause};
    use playbook_resolution::lineage::ResolutionLineage;
    use playbook_resolution::policy::{Action, Prescription};

    fn fixture() -> (Diagnosis, Prescription, ResolutionLineage) {
        let diagnosis = Diagnosis {
            primary: RefusalCause::OutputTruncation,
            confidence: Confidence::Certain,
            deterministic: true,
            rationale: "finish_reason 'length' with structurally incomplete output".into(),
        };
        let prescription = Prescription {
            actions: vec![Action::ExpandEnvelope, Action::DecomposeWork],
            suggested_envelope: Some(8_000),
            suggested_chunks: Some(3),
            rationale: "token starvation: widen, then split".into(),
            escalated: false,
        };
        let mut lineage = ResolutionLineage::new();
        lineage.root("attempt-1", 1_000);
        lineage.refuse("attempt-1", "deadbeef", "decompose_work");
        (diagnosis, prescription, lineage)
    }

    #[test]
    fn projection_is_deterministic() {
        let (diagnosis, prescription, lineage) = fixture();
        let first = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        let second = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        assert_eq!(first, second);
    }

    #[test]
    fn acceptance_standard_is_false_by_construction() {
        let (diagnosis, prescription, lineage) = fixture();
        let snapshot = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        assert!(!snapshot.acceptance_standard_changed());
        assert!(snapshot.policy_hash().is_none());
    }

    #[test]
    fn acceptance_standard_changes_only_via_policy_revision() {
        let (diagnosis, prescription, lineage) = fixture();
        let snapshot = ResolutionSnapshot::for_policy_revision(
            &diagnosis,
            &prescription,
            &lineage,
            "abc123".into(),
        );
        assert!(snapshot.acceptance_standard_changed());
        assert_eq!(snapshot.policy_hash(), Some("abc123"));
        // And a plain projection of the very same state stays false.
        let plain = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        assert!(!plain.acceptance_standard_changed());
    }

    #[test]
    fn projection_reads_every_field_from_the_authority() {
        let (diagnosis, prescription, lineage) = fixture();
        let snapshot = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        assert_eq!(snapshot.refusal_key(), "output_truncation");
        assert_eq!(snapshot.family_key(), "capability");
        assert_eq!(snapshot.confidence_key(), "certain");
        assert!(snapshot.deterministic());
        assert_eq!(
            snapshot.actions(),
            &["expand_envelope", "decompose_work"][..]
        );
        assert!(!snapshot.escalated());
        assert_eq!(snapshot.lineage_depth(), 2);
        assert_eq!(snapshot.rationale(), diagnosis.rationale);
    }

    #[test]
    fn escalated_prescription_projects_the_marker() {
        let (diagnosis, _, lineage) = fixture();
        let prescription = Prescription {
            actions: vec![Action::Escalate],
            suggested_envelope: None,
            suggested_chunks: None,
            rationale: "budget exhausted".into(),
            escalated: true,
        };
        let snapshot = ResolutionSnapshot::from_resolution(&diagnosis, &prescription, &lineage);
        assert!(snapshot.escalated());
    }
}
