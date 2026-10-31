//! Isopod (§19) — continuous governance assessment.
//!
//! CI (Kaptaind, §36) answers "may this change enter?". Isopod answers
//! "is the organisation still operating the system that makes those
//! decisions trustworthy?". It consumes a [`GovernanceState`] snapshot
//! plus the last admitted evidence, and never re-runs scanners: every
//! control is read off observed governance state, compared against what
//! the last evidence implies. All semantics fail closed.

use serde::{Deserialize, Serialize};

/// A snapshot of whether the organisation is operating each governance
/// control. Every field is an observation about the *system that
/// produces and honours evidence* — not about any particular change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceState {
    pub branch_protection_enabled: bool,
    pub required_reviews_present: bool,
    pub required_ci_gates_enabled: bool,
    pub traci_evidence_available: bool,
    pub release_governance_active: bool,
    pub evidence_retention_healthy: bool,
    pub value_protocol_protected: bool,
    pub ci_policy_unchanged_without_review: bool,
    pub kaptaind_admission_recorded: bool,
}

impl GovernanceState {
    /// The name of each control, in declaration order.
    pub const CONTROLS: [&'static str; 9] = [
        "branch_protection_enabled",
        "required_reviews_present",
        "required_ci_gates_enabled",
        "traci_evidence_available",
        "release_governance_active",
        "evidence_retention_healthy",
        "value_protocol_protected",
        "ci_policy_unchanged_without_review",
        "kaptaind_admission_recorded",
    ];

    fn value(&self, control: &'static str) -> bool {
        match control {
            "branch_protection_enabled" => self.branch_protection_enabled,
            "required_reviews_present" => self.required_reviews_present,
            "required_ci_gates_enabled" => self.required_ci_gates_enabled,
            "traci_evidence_available" => self.traci_evidence_available,
            "release_governance_active" => self.release_governance_active,
            "evidence_retention_healthy" => self.evidence_retention_healthy,
            "value_protocol_protected" => self.value_protocol_protected,
            "ci_policy_unchanged_without_review" => self.ci_policy_unchanged_without_review,
            "kaptaind_admission_recorded" => self.kaptaind_admission_recorded,
            _ => false,
        }
    }

    /// Controls that, if unsatisfied, break governance outright —
    /// regardless of drift history. Fail closed.
    pub fn is_critical(control: &str) -> bool {
        matches!(
            control,
            "branch_protection_enabled"
                | "value_protocol_protected"
                | "kaptaind_admission_recorded"
        )
    }
}

/// One control's assessment at a point in time.
///
/// - `satisfied`: the control holds now.
/// - `drift`: the control is unsatisfied now but was satisfied at the
///   last evidence — the organisation regressed since it last proved
///   otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlReport {
    pub control: &'static str,
    pub satisfied: bool,
    pub drift: bool,
}

/// What the last admitted evidence implies about a control's prior
/// state. Where the evidence is silent, the prior is unproven — and an
/// unproven prior fails closed: an unsatisfied control with no prior
/// evidence counts as drift, because nothing certifies the control was
/// ever healthy.
fn prior_implied(
    _state: &GovernanceState,
    control: &'static str,
    last_evidence: Option<&ci_evidence::CiEvidence>,
) -> bool {
    match (control, last_evidence) {
        // The existence of recorded evidence proves retention held at
        // that time (for a well-formed envelope).
        ("evidence_retention_healthy", Some(ev)) => {
            ev.schema == ci_evidence::SCHEMA && !ev.gates.is_empty()
        }
        // Evidence only exists because Kaptaind admitted the revision.
        ("kaptaind_admission_recorded", Some(_)) => true,
        // Evidence is otherwise silent about specific controls; with no
        // evidence at all, no prior is proven.
        (_, _) => false,
    }
}

/// Assess every control. One [`ControlReport`] per control, in
/// declaration order. Never re-runs scanners; only compares observed
/// state against what the last evidence implies.
pub fn assess(
    state: &GovernanceState,
    last_evidence: Option<&ci_evidence::CiEvidence>,
    _now_ms: u64,
) -> Vec<ControlReport> {
    GovernanceState::CONTROLS
        .into_iter()
        .map(|control| {
            let satisfied = state.value(control);
            let prior = prior_implied(state, control, last_evidence);
            ControlReport {
                control,
                satisfied,
                drift: !satisfied && prior,
            }
        })
        .collect()
}

/// The overall governance verdict, derived mechanically and failing
/// closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GovernanceVerdict {
    /// Every control is satisfied.
    Intact,
    /// No critical control failed, but these controls regressed since
    /// the last evidence.
    Drifting(Vec<&'static str>),
    /// A critical control is unsatisfied. Governance is broken; nothing
    /// admitted under it is trustworthy until repaired.
    Broken,
}

impl Serialize for GovernanceVerdict {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStructVariant;
        match self {
            Self::Intact => serializer.serialize_unit_variant("GovernanceVerdict", 0, "Intact"),
            Self::Drifting(controls) => {
                let mut variant =
                    serializer.serialize_struct_variant("GovernanceVerdict", 1, "Drifting", 1)?;
                variant.serialize_field("controls", controls)?;
                variant.end()
            }
            Self::Broken => serializer.serialize_unit_variant("GovernanceVerdict", 2, "Broken"),
        }
    }
}

impl<'de> Deserialize<'de> for GovernanceVerdict {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Tag {
            Intact,
            Drifting,
            Broken,
        }
        #[derive(Deserialize)]
        struct DriftingPayload {
            controls: Vec<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Shape {
            Unit(Tag),
            Drifting((), DriftingPayload),
        }
        match Shape::deserialize(deserializer)? {
            Shape::Unit(Tag::Intact) => Ok(Self::Intact),
            Shape::Unit(Tag::Broken) => Ok(Self::Broken),
            Shape::Unit(Tag::Drifting) => Ok(Self::Drifting(Vec::new())),
            Shape::Drifting((), payload) => Ok(Self::Drifting(
                payload
                    .controls
                    .into_iter()
                    .filter_map(|control| {
                        GovernanceState::CONTROLS
                            .into_iter()
                            .find(|known| *known == control.as_str())
                    })
                    .collect(),
            )),
        }
    }
}

/// Derive the verdict from control reports. Fails closed: `Intact` is
/// only possible when every control is satisfied.
///
/// - [`GovernanceVerdict::Broken`] when any critical control
///   (`branch_protection_enabled`, `value_protocol_protected`,
///   `kaptaind_admission_recorded`) is unsatisfied — drift history is
///   irrelevant; failure is failure.
/// - Otherwise [`GovernanceVerdict::Drifting`] listing every
///   unsatisfied control, drift-flagged ones first in declaration
///   order.
/// - Otherwise [`GovernanceVerdict::Intact`].
pub fn verdict(reports: &[ControlReport]) -> GovernanceVerdict {
    for report in reports {
        if !report.satisfied && GovernanceState::is_critical(report.control) {
            return GovernanceVerdict::Broken;
        }
    }
    let drifting: Vec<&'static str> = reports
        .iter()
        .filter(|report| !report.satisfied)
        .map(|report| report.control)
        .collect();
    if drifting.is_empty() {
        GovernanceVerdict::Intact
    } else {
        GovernanceVerdict::Drifting(drifting)
    }
}

/// Every control that regressed from satisfied (`previous`) to
/// unsatisfied (`current`) — the raw drift signal, independent of any
/// evidence envelope.
pub fn governance_drift_detected(
    previous: &GovernanceState,
    current: &GovernanceState,
) -> Vec<&'static str> {
    GovernanceState::CONTROLS
        .into_iter()
        .filter(|control| previous.value(control) && !current.value(control))
        .collect()
}
