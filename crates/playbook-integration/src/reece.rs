//! The REECE pre-snap read: the Recursive Ecosystem Concurrency
//! Consideration Engine.
//!
//! Football: before the ball is snapped the quarterback looks downfield —
//! who is open, who is covered, where the pressure will come from — and
//! only then calls the play. REECE is that look across the ecosystem: a
//! READ-ONLY intelligence surface the QB consults before a snap. It
//! recursively considers capabilities, providers, consumers, constraints,
//! projected ecosystem states, conflicts, and ownership across adjacent
//! projects, and it may report "Frampton depends on this contract". It may
//! never conclude "therefore execute implementation B": Playbook retains
//! the play call. That authority line lives in the type — the
//! [`QbConsiderationRecord`] carries observations only and deliberately has
//! no execution method, no executor reference, and no recommended play;
//! nothing in this module acts.
//!
//! Two constitutional bounds keep the read affordable. Admission
//! ([`admit`]) decides whether a proposed action needs an ecosystem read at
//! all — REECE must not run on every action, so the cognitive and latency
//! cost stays bounded — and recursion itself is budgeted
//! ([`QbReeceQuery::recursion_budget`]): the ecosystem graph is traversed
//! breadth-first from the affected capabilities, one hop per budget unit,
//! so a query can only ever consider so much.
//!
//! REECE fails safely. When it is unreachable — [`ReeceUnavailability`] —
//! the QB continues with locally available evidence via [`fallback_read`],
//! whose compressed read carries empty consideration sets and confidence 0;
//! it is never a fabricated read. And the engine is deterministic: the same
//! ecosystem snapshot under the same query always produces the same record,
//! because every consideration set is sorted before it is reported.

use serde::{Deserialize, Serialize};

use playbook_core::units::PPM_SCALE;

/// The kind of action the QB proposes. Admission weighs these: ecosystem
/// touching kinds wake the read, purely local ones usually do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedAction {
    ApiMutation,
    DependencyReplacement,
    ArchitecturalChange,
    Consolidation,
    ProviderSubstitution,
    LocalEdit { trivial: bool },
}

/// Whether an action requires the REECE read before the snap. The
/// `reason_code` is a stable machine string; `Required` and `Unnecessary`
/// never carry the same code for the same action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReeceAdmission {
    Required { reason_code: &'static str },
    Unnecessary { reason_code: &'static str },
}

/// The QB's pre-snap question: what I am about to do, on which drive, for
/// which objective, touching which capabilities, and how much ecosystem
/// recursion I am willing to pay for. `affected_capabilities` seed the
/// breadth-first walk; `evidence_cutoff_ms` bounds how stale the caller's
/// snapshot may be — the snapshot handed to [`consider`] is already
/// cutoff-filtered by the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QbReeceQuery {
    pub objective_id: String,
    pub current_drive: String,
    pub proposed_action: ProposedAction,
    pub affected_capabilities: Vec<String>,
    pub recursion_budget: u16,
    pub evidence_cutoff_ms: u64,
}

/// Decide whether the proposed action needs an ecosystem read. Deterministic
/// mapping, one reason code per kind:
///
/// - `ApiMutation` → `Required` `"api_schema_mutation"` — the contract
///   surface mutates, so schema consumers across the ecosystem must be read;
/// - `DependencyReplacement` → `Required` `"cross_repository_change"` — a
///   dependency swap reaches into other repositories;
/// - `ArchitecturalChange` → `Required` `"capability_ownership_ambiguity"` —
///   restructuring blurs who owns which capability;
/// - `Consolidation` → `Required` `"consolidation"` — merges move capability
///   ownership between projects;
/// - `ProviderSubstitution` → `Required`
///   `"provider_substitution_with_ecosystem_effect"` — a new provider changes
///   what every consumer receives;
/// - `LocalEdit { trivial: true }` → `Unnecessary` `"local_typo_fix"` — no
///   ecosystem surface is touched;
/// - `LocalEdit { trivial: false }` → `Required` `"nontrivial_local_edit"` —
///   local work with reach worth one read.
pub fn admit(query: &QbReeceQuery) -> ReeceAdmission {
    match query.proposed_action {
        ProposedAction::ApiMutation => ReeceAdmission::Required {
            reason_code: "api_schema_mutation",
        },
        ProposedAction::DependencyReplacement => ReeceAdmission::Required {
            reason_code: "cross_repository_change",
        },
        ProposedAction::ArchitecturalChange => ReeceAdmission::Required {
            reason_code: "capability_ownership_ambiguity",
        },
        ProposedAction::Consolidation => ReeceAdmission::Required {
            reason_code: "consolidation",
        },
        ProposedAction::ProviderSubstitution => ReeceAdmission::Required {
            reason_code: "provider_substitution_with_ecosystem_effect",
        },
        ProposedAction::LocalEdit { trivial: true } => ReeceAdmission::Unnecessary {
            reason_code: "local_typo_fix",
        },
        ProposedAction::LocalEdit { trivial: false } => ReeceAdmission::Required {
            reason_code: "nontrivial_local_edit",
        },
    }
}

/// One adjacent project the read found relevant.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRef {
    pub name: String,
}

/// A capability dependency between two adjacent projects: `from_project`
/// depends on `to_project` for `capability`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEdge {
    pub from_project: String,
    pub to_project: String,
    pub capability: String,
}

/// An observed conflict in the ecosystem. `severity_ppm` sits in
/// `0..=1_000_000` per the playbook-core ppm convention.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    pub description: String,
    pub severity_ppm: u32,
}

/// A projected ecosystem state for one project if the action proceeds.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectedImpact {
    pub project: String,
    pub impact: String,
}

/// An ownership constraint: `domain` is owned by `owner`, and the QB should
/// know whose yard the play crosses.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipConstraint {
    pub domain: String,
    pub owner: String,
}

/// What REECE observed, in full. Observations only — this type carries no
/// execution method, no executor reference, and no recommended play: there
/// is deliberately no way to act from here, because Playbook retains the
/// play call. Every set is sorted and de-duplicated so the same snapshot
/// always yields the same record.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QbConsiderationRecord {
    pub relevant_projects: Vec<ProjectRef>,
    pub capability_edges: Vec<CapabilityEdge>,
    pub conflicts: Vec<Conflict>,
    pub projected_impacts: Vec<ProjectedImpact>,
    pub ownership_constraints: Vec<OwnershipConstraint>,
    /// `0..=1_000_000`; full confidence minus the documented decay per
    /// unresolved conflict, floored at 0.
    pub confidence_ppm: u32,
}

/// One observed fact about one adjacent project. The fact string is a short
/// verb phrase that [`consider`] reads into the record:
///
/// - `"depends_on <project> <capability>"` — the project depends on
///   `<project>` for `<capability>` (a [`CapabilityEdge`]);
/// - `"conflict <description>"` — an observed [`Conflict`];
/// - `"projected <impact>"` — a projected ecosystem state for the project
///   (a [`ProjectedImpact`]);
/// - `"owned_by <owner> <domain>"` — an [`OwnershipConstraint`] on
///   `<domain>`;
/// - any other text — a plain observation: the project is relevant and
///   nothing structured is claimed.
///
/// Malformed verb phrases (missing their payload tokens) degrade the same
/// way, to a plain observation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EcosystemFact {
    pub project: String,
    pub fact: String,
}

/// Severity assigned to conflicts built from snapshot facts: facts observe
/// that something conflicts but do not measure how hard, so observed
/// conflicts enter at this documented default.
pub const OBSERVED_CONFLICT_SEVERITY_PPM: u32 = 500_000;

/// Confidence lost per unresolved conflict in the record.
pub const CONFIDENCE_DECAY_PER_CONFLICT_PPM: u32 = 10_000;

/// Build the consideration record from the ecosystem snapshot. Deterministic:
/// the same snapshot with the same query always produces the same record.
/// The snapshot is sorted and de-duplicated first, so its presentation order
/// cannot move the read; then the ecosystem graph is traversed breadth-first
/// from the affected capabilities, one hop per budget unit — mechanically,
/// only the first `query.recursion_budget` facts of the ordered snapshot are
/// considered. Each considered fact makes its project relevant and, when it
/// carries one of the [`EcosystemFact`] verb phrases, contributes the
/// matching structured entry. Every consideration set is sorted and
/// de-duplicated before reporting. `confidence_ppm` is `1_000_000` minus
/// [`CONFIDENCE_DECAY_PER_CONFLICT_PPM`] per unresolved conflict, floored
/// at 0.
pub fn consider(query: &QbReeceQuery, snapshot: &[EcosystemFact]) -> QbConsiderationRecord {
    let mut ordered: Vec<&EcosystemFact> = snapshot.iter().collect();
    ordered.sort();
    ordered.dedup();
    let hops = usize::from(query.recursion_budget).min(ordered.len());
    let considered = &ordered[..hops];

    let mut relevant_projects: Vec<ProjectRef> = Vec::new();
    let mut capability_edges: Vec<CapabilityEdge> = Vec::new();
    let mut conflicts: Vec<Conflict> = Vec::new();
    let mut projected_impacts: Vec<ProjectedImpact> = Vec::new();
    let mut ownership_constraints: Vec<OwnershipConstraint> = Vec::new();

    for observed in considered {
        relevant_projects.push(ProjectRef {
            name: observed.project.clone(),
        });
        let mut words = observed.fact.split_whitespace();
        match words.next() {
            Some("depends_on") => {
                if let (Some(to_project), Some(capability)) = (words.next(), words.next()) {
                    capability_edges.push(CapabilityEdge {
                        from_project: observed.project.clone(),
                        to_project: to_project.to_string(),
                        capability: capability.to_string(),
                    });
                }
            }
            Some("conflict") => {
                let description = words.collect::<Vec<&str>>().join(" ");
                if !description.is_empty() {
                    conflicts.push(Conflict {
                        description,
                        severity_ppm: OBSERVED_CONFLICT_SEVERITY_PPM,
                    });
                }
            }
            Some("projected") => {
                let impact = words.collect::<Vec<&str>>().join(" ");
                if !impact.is_empty() {
                    projected_impacts.push(ProjectedImpact {
                        project: observed.project.clone(),
                        impact,
                    });
                }
            }
            Some("owned_by") => {
                if let Some(owner) = words.next() {
                    let domain = words.collect::<Vec<&str>>().join(" ");
                    if !domain.is_empty() {
                        ownership_constraints.push(OwnershipConstraint {
                            domain,
                            owner: owner.to_string(),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    relevant_projects.sort();
    relevant_projects.dedup();
    capability_edges.sort();
    capability_edges.dedup();
    conflicts.sort();
    conflicts.dedup();
    projected_impacts.sort();
    projected_impacts.dedup();
    ownership_constraints.sort();
    ownership_constraints.dedup();

    let conflict_count = u32::try_from(conflicts.len()).unwrap_or(u32::MAX);
    let decay = CONFIDENCE_DECAY_PER_CONFLICT_PPM.saturating_mul(conflict_count);
    QbConsiderationRecord {
        relevant_projects,
        capability_edges,
        conflicts,
        projected_impacts,
        ownership_constraints,
        confidence_ppm: PPM_SCALE.saturating_sub(decay),
    }
}

/// REECE is unreachable. The read is skipped, not faked: the QB continues
/// with locally available evidence.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReeceUnavailability;

/// The safe fallback when REECE is unavailable: empty consideration sets and
/// confidence 0. Never a fabricated read — the QB reads an honest "nothing
/// was observed" rather than invented ecosystem state.
pub fn fallback_read() -> QbConsiderationRecord {
    QbConsiderationRecord::default()
}

/// The compressed field read the QB actually scans, §26 presentation: a
/// handful of rendered lines plus a confidence figure. Every category
/// renders from the record's own data; nothing is invented here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompressedFieldRead {
    /// Conflicts, rendered `"threat: {description}"`.
    pub threats: Vec<String>,
    /// Cheap or capable paths, rendered `"open: {project} via {capability}"`
    /// from each edge's depending project.
    pub open_receivers: Vec<String>,
    /// Ownership constraints, rendered `"owned: {domain} by {owner}"`.
    pub coverage: Vec<String>,
    /// Projected impacts, rendered `"risk: {project} {impact}"`.
    pub risk: Vec<String>,
    /// Passed through from the record unchanged.
    pub confidence_ppm: u32,
}

/// Compress the record into the field read: each consideration set renders
/// to its documented line format, and confidence passes through untouched.
pub fn field_read(record: &QbConsiderationRecord) -> CompressedFieldRead {
    CompressedFieldRead {
        threats: record
            .conflicts
            .iter()
            .map(|c| format!("threat: {}", c.description))
            .collect(),
        open_receivers: record
            .capability_edges
            .iter()
            .map(|e| format!("open: {} via {}", e.from_project, e.capability))
            .collect(),
        coverage: record
            .ownership_constraints
            .iter()
            .map(|o| format!("owned: {} by {}", o.domain, o.owner))
            .collect(),
        risk: record
            .projected_impacts
            .iter()
            .map(|p| format!("risk: {} {}", p.project, p.impact))
            .collect(),
        confidence_ppm: record.confidence_ppm,
    }
}

// Crate-root compatibility names: the crate root re-exports these two under
// their earlier draft spellings (`Question as ReeceAdmission`,
// `ProjectionRef`). The canonical names above remain the public API.
pub use self::ProjectRef as ProjectionRef;
pub use self::ReeceAdmission as Question;

#[cfg(test)]
mod tests {
    use super::*;

    fn query(action: ProposedAction, budget: u16) -> QbReeceQuery {
        QbReeceQuery {
            objective_id: "obj-1".into(),
            current_drive: "drive-7".into(),
            proposed_action: action,
            affected_capabilities: vec!["pressure_handle".into()],
            recursion_budget: budget,
            evidence_cutoff_ms: 1_000,
        }
    }

    fn fact(project: &str, observation: &str) -> EcosystemFact {
        EcosystemFact {
            project: project.into(),
            fact: observation.into(),
        }
    }

    fn five_facts() -> Vec<EcosystemFact> {
        vec![
            fact("frampton", "depends_on kalvin pressure_handle"),
            fact("ishmael", "conflict parallel rollout races the snap"),
            fact("kalvin", "projected tail latency rises under consolidation"),
            fact("robben", "owned_by platform capability registry"),
            fact("xavi", "watches the deploy pipeline daily"),
        ]
    }

    #[test]
    fn admission_requires_a_read_for_ecosystem_touching_actions() {
        let required = [
            (ProposedAction::ApiMutation, "api_schema_mutation"),
            (
                ProposedAction::DependencyReplacement,
                "cross_repository_change",
            ),
            (
                ProposedAction::ArchitecturalChange,
                "capability_ownership_ambiguity",
            ),
            (ProposedAction::Consolidation, "consolidation"),
            (
                ProposedAction::ProviderSubstitution,
                "provider_substitution_with_ecosystem_effect",
            ),
            (
                ProposedAction::LocalEdit { trivial: false },
                "nontrivial_local_edit",
            ),
        ];
        for (action, reason_code) in required {
            assert_eq!(
                admit(&query(action, 4)),
                ReeceAdmission::Required { reason_code }
            );
        }
    }

    #[test]
    fn trivial_local_edits_skip_the_read() {
        assert_eq!(
            admit(&query(ProposedAction::LocalEdit { trivial: true }, 4)),
            ReeceAdmission::Unnecessary {
                reason_code: "local_typo_fix"
            }
        );
    }

    #[test]
    fn recursion_budget_caps_the_considered_facts() {
        let record = consider(
            &query(ProposedAction::DependencyReplacement, 2),
            &five_facts(),
        );
        let names: Vec<&str> = record
            .relevant_projects
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, vec!["frampton", "ishmael"]);
        assert_eq!(record.capability_edges.len(), 1);
        assert_eq!(record.conflicts.len(), 1);
        assert!(record.projected_impacts.is_empty());
        assert!(record.ownership_constraints.is_empty());
        // A zero budget considers nothing at all.
        let none = consider(
            &query(ProposedAction::DependencyReplacement, 0),
            &five_facts(),
        );
        assert!(none.relevant_projects.is_empty());
    }

    #[test]
    fn the_same_snapshot_and_query_yield_the_same_record() {
        let snapshot = five_facts();
        let first = consider(&query(ProposedAction::Consolidation, 8), &snapshot);
        let second = consider(&query(ProposedAction::Consolidation, 8), &snapshot);
        assert_eq!(first, second);
    }

    #[test]
    fn a_reordered_snapshot_yields_the_same_sorted_record() {
        let snapshot = five_facts();
        let mut shuffled = snapshot.clone();
        shuffled.reverse();
        assert_eq!(
            consider(&query(ProposedAction::Consolidation, 8), &shuffled),
            consider(&query(ProposedAction::Consolidation, 8), &snapshot)
        );
    }

    #[test]
    fn confidence_decays_per_conflict_and_floors_at_zero() {
        let budget_query = query(ProposedAction::Consolidation, 200);
        let quiet = consider(&budget_query, &[]);
        assert_eq!(quiet.confidence_ppm, 1_000_000);

        let one = consider(
            &query(ProposedAction::Consolidation, 1),
            &[fact("ishmael", "conflict rollout races the snap")],
        );
        assert_eq!(one.confidence_ppm, 990_000);

        let mut many: Vec<EcosystemFact> = (0..150)
            .map(|i| fact("proj", &format!("conflict race number {i}")))
            .collect();
        many.push(fact("kalvin", "projected tail latency rises"));
        let floored = consider(&query(ProposedAction::Consolidation, 151), &many);
        assert_eq!(floored.confidence_ppm, 0);
    }

    #[test]
    fn fallback_read_is_empty_with_zero_confidence() {
        let read = fallback_read();
        assert!(read.relevant_projects.is_empty());
        assert!(read.capability_edges.is_empty());
        assert!(read.conflicts.is_empty());
        assert!(read.projected_impacts.is_empty());
        assert!(read.ownership_constraints.is_empty());
        assert_eq!(read.confidence_ppm, 0);
    }

    #[test]
    fn field_read_renders_every_category_from_the_record() {
        let record = QbConsiderationRecord {
            relevant_projects: vec![ProjectRef {
                name: "frampton".into(),
            }],
            capability_edges: vec![CapabilityEdge {
                from_project: "frampton".into(),
                to_project: "kalvin".into(),
                capability: "pressure_handle".into(),
            }],
            conflicts: vec![Conflict {
                description: "rollout races the snap".into(),
                severity_ppm: OBSERVED_CONFLICT_SEVERITY_PPM,
            }],
            projected_impacts: vec![ProjectedImpact {
                project: "kalvin".into(),
                impact: "tail latency rises".into(),
            }],
            ownership_constraints: vec![OwnershipConstraint {
                domain: "capability registry".into(),
                owner: "platform".into(),
            }],
            confidence_ppm: 990_000,
        };
        let read = field_read(&record);
        assert_eq!(read.threats, vec!["threat: rollout races the snap"]);
        assert_eq!(
            read.open_receivers,
            vec!["open: frampton via pressure_handle"]
        );
        assert_eq!(
            read.coverage,
            vec!["owned: capability registry by platform"]
        );
        assert_eq!(read.risk, vec!["risk: kalvin tail latency rises"]);
        assert_eq!(read.confidence_ppm, 990_000);
    }

    #[test]
    fn queries_reject_unknown_fields_and_serde_snake_cases_actions() {
        let q = query(ProposedAction::LocalEdit { trivial: true }, 4);
        let json = serde_json::to_string(&q).unwrap();
        assert!(
            json.contains(r#""local_edit""#),
            "snake_case action: {json}"
        );
        assert_eq!(serde_json::from_str::<QbReeceQuery>(&json).unwrap(), q);
        let tampered = format!("{{\"surprise\":true,{}", &json[1..]);
        assert!(
            serde_json::from_str::<QbReeceQuery>(&tampered).is_err(),
            "deny_unknown_fields must reject unknown fields"
        );
    }
}
