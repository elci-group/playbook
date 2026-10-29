//! Participant selection and context packet construction.
//!
//! Agents do not participate merely because they are active. Default
//! exclusion is preferred: selection is by dependency relevance, and each
//! participant receives only the minimum sufficient shared state.

use playbook_core::{Evidence, Gate, Personnel, PersonnelId, PlaybookError};

use playbook_elci::HuddleDispatch;

use crate::domain::{
    ActiveHuddle, Assignment, DisclosureProfile, EvidenceSummary, GateSnapshot,
    HuddleContextPacket, HuddleParticipant, HuddleReport, HuddleRole, HuddleTrigger,
    HuddleTriggerKind, ParticipantBudget,
};
use crate::reconcile::normalize_report;

/// A selection candidate. Every component is a measured relevance score in
/// 0..=1000; self-reported capability never enters the calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParticipantCandidate {
    pub personnel_id: PersonnelId,
    pub dependency_overlap: u32,
    pub contradiction_relevance: u32,
    pub ownership_weight: u32,
    pub downstream_impact: u32,
    pub specialist_value: u32,
    pub context_cost: u32,
}

/// Deterministic participation score.
pub fn participation_score(
    candidate: &ParticipantCandidate,
    weights: &playbook_policy::ParticipantWeights,
) -> i64 {
    i64::from(candidate.dependency_overlap.min(1_000)) * i64::from(weights.dependency_overlap)
        + i64::from(candidate.contradiction_relevance.min(1_000))
            * i64::from(weights.contradiction_relevance)
        + i64::from(candidate.ownership_weight.min(1_000)) * i64::from(weights.ownership)
        + i64::from(candidate.downstream_impact.min(1_000)) * i64::from(weights.downstream_impact)
        + i64::from(candidate.specialist_value.min(1_000)) * i64::from(weights.specialist_value)
        - i64::from(candidate.context_cost.min(1_000)) * i64::from(weights.context_cost)
}

/// Select participants: score-descending, id-ascending on ties, capped by
/// the budget envelope, and filtered by the policy floor. An empty result is
/// the preferred default.
pub fn select(
    candidates: &[ParticipantCandidate],
    weights: &playbook_policy::ParticipantWeights,
    max_participants: u16,
    min_score: i64,
) -> Vec<PersonnelId> {
    let mut scored: Vec<(i64, &ParticipantCandidate)> = candidates
        .iter()
        .map(|candidate| (participation_score(candidate, weights), candidate))
        .filter(|(score, _)| *score >= min_score)
        .collect();
    scored.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            left.1
                .personnel_id
                .as_str()
                .cmp(right.1.personnel_id.as_str())
        })
    });
    scored
        .into_iter()
        .take(usize::from(max_participants))
        .map(|(_, candidate)| candidate.personnel_id.clone())
        .collect()
}

/// The disclosure-regulation seam. Jose implements this; the deterministic
/// default keeps the system honest without it.
pub trait HuddleRegulator {
    fn build_disclosure(
        &self,
        participant: &Personnel,
        trigger: &HuddleTrigger,
    ) -> DisclosureProfile;
}

/// Deterministic default: the measured personnel kind decides the profile,
/// and a strategic trigger widens visibility only for deterministic
/// procedures and humans.
pub struct DefaultRegulator;

impl HuddleRegulator for DefaultRegulator {
    fn build_disclosure(
        &self,
        participant: &Personnel,
        trigger: &HuddleTrigger,
    ) -> DisclosureProfile {
        let strategic = trigger.kind == HuddleTriggerKind::Strategic;
        match participant.kind {
            playbook_core::PersonnelKind::DeterministicProcedure
            | playbook_core::PersonnelKind::Tool
            | playbook_core::PersonnelKind::Toolchain => DisclosureProfile {
                include_gates: true,
                include_peer_reports: true,
                include_strategic_state: strategic,
                max_evidence_refs: 16,
            },
            playbook_core::PersonnelKind::SpecialistAgent => DisclosureProfile {
                include_gates: true,
                include_peer_reports: false,
                include_strategic_state: false,
                max_evidence_refs: 8,
            },
            playbook_core::PersonnelKind::Model => DisclosureProfile {
                include_gates: false,
                include_peer_reports: true,
                include_strategic_state: false,
                max_evidence_refs: 4,
            },
            playbook_core::PersonnelKind::HumanEscalation => DisclosureProfile {
                include_gates: true,
                include_peer_reports: true,
                include_strategic_state: true,
                max_evidence_refs: 32,
            },
        }
    }
}

/// The role a participant takes by default, derived from the measured kind.
pub fn role_for(participant: &Personnel) -> HuddleRole {
    match participant.kind {
        playbook_core::PersonnelKind::SpecialistAgent => HuddleRole::Specialist,
        playbook_core::PersonnelKind::HumanEscalation => HuddleRole::Coordinator,
        _ => HuddleRole::Contributor,
    }
}

fn gate_snapshots(gates: &[Gate]) -> Vec<GateSnapshot> {
    gates
        .iter()
        .map(|gate| GateSnapshot {
            gate_id: gate.id.clone(),
            weight_ppm: gate.weight_ppm,
            satisfaction_ppm: gate.satisfaction_ppm,
            state: gate.state,
        })
        .collect()
}

fn evidence_summaries(
    huddle: &ActiveHuddle,
    evidence: &[Evidence],
    profile: &DisclosureProfile,
) -> Vec<EvidenceSummary> {
    let mut referenced: Vec<&str> = huddle
        .trigger
        .evidence_refs
        .iter()
        .map(|id| id.as_str())
        .collect();
    for report in &huddle.reports {
        referenced.extend(report.verified_progress.iter().map(|id| id.as_str()));
    }
    referenced.sort_unstable();
    referenced.dedup();
    evidence
        .iter()
        .filter(|item| referenced.contains(&item.id.as_str()))
        .take(usize::from(profile.max_evidence_refs))
        .map(|item| EvidenceSummary {
            evidence_id: item.id.clone(),
            class: item.class,
            observed_at_ms: item.observed_at_ms,
            summary: format!("{:?} observation", item.class),
        })
        .collect()
}

/// Build the scoped context packet for one participant. Unauthorized fields
/// stay empty: no authorization, no disclosure.
pub fn build_packet(
    huddle: &ActiveHuddle,
    participant: &HuddleParticipant,
    task_fragment: &str,
    gates: &[Gate],
    evidence: &[Evidence],
    assignment: Assignment,
) -> HuddleContextPacket {
    let profile = &participant.disclosure_profile;
    HuddleContextPacket {
        huddle_id: huddle.id.clone(),
        trigger: huddle.trigger.clone(),
        task_fragment: task_fragment.to_string(),
        relevant_gates: if profile.include_gates {
            gate_snapshots(gates)
        } else {
            Vec::new()
        },
        relevant_evidence: evidence_summaries(huddle, evidence, profile),
        current_assignment: assignment,
        dependency_changes: if profile.include_peer_reports {
            huddle
                .reports
                .iter()
                .flat_map(|report| report.dependency_changes.iter().cloned())
                .collect()
        } else {
            Vec::new()
        },
        contradictions: if profile.include_peer_reports {
            huddle.contradictions.clone()
        } else {
            Vec::new()
        },
        budget: ParticipantBudget {
            max_tokens: huddle
                .budget
                .max_tokens
                .saturating_div(huddle.participants.len().max(1) as u64),
            max_duration_ms: huddle.budget.max_duration_ms,
        },
        requested_response_schema: "https://playbook.local/schemas/huddle_report.json".into(),
    }
}

/// Dispatch one packet through ELCI and normalize the structured response.
pub fn dispatch_packet(
    plane: &mut dyn HuddleDispatch,
    packet: &HuddleContextPacket,
) -> Result<HuddleReport, PlaybookError> {
    let payload = serde_json::to_value(packet).map_err(PlaybookError::json)?;
    let response = plane.huddle_dispatch(&payload)?;
    normalize_report(&response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{
        EventId, EvidenceClass, EvidenceId, Gate, GateId, HuddleId, PersonnelKind, PPM_SCALE,
    };
    use serde_json::json;

    fn weights() -> playbook_policy::ParticipantWeights {
        playbook_policy::Policy::builtin()
            .huddle
            .participant_weights
    }

    fn candidate(id: &str, relevance: u32) -> ParticipantCandidate {
        ParticipantCandidate {
            personnel_id: PersonnelId::new(id).unwrap(),
            dependency_overlap: relevance,
            contradiction_relevance: relevance,
            ownership_weight: relevance,
            downstream_impact: relevance,
            specialist_value: relevance,
            context_cost: 10,
        }
    }

    #[test]
    fn selection_is_scored_capped_and_deterministic() {
        let weights = weights();
        let candidates = vec![
            candidate("agent-c", 900),
            candidate("agent-a", 900),
            candidate("agent-b", 200),
        ];
        let picked = select(&candidates, &weights, 2, 1_000);
        assert_eq!(
            picked.iter().map(PersonnelId::as_str).collect::<Vec<_>>(),
            vec!["agent-a", "agent-c"]
        );
        // Below the floor nobody participates: default exclusion.
        assert!(select(&candidates, &weights, 2, 100_000).is_empty());
    }

    #[test]
    fn packets_are_scoped_by_disclosure_profile() {
        let trigger = HuddleTrigger {
            kind: HuddleTriggerKind::Failure,
            reason_code: "repeated_failed_play".into(),
            evidence_refs: vec![EvidenceId::new("ev-1").unwrap()],
            originating_event: EventId::new("evt-1").unwrap(),
        };
        let huddle = ActiveHuddle {
            id: HuddleId::new("h-1").unwrap(),
            scope: crate::domain::HuddleScope::Position,
            barrier: playbook_core::BarrierMode::Soft,
            phase: crate::domain::HuddlePhase::CollectReports,
            trigger: trigger.clone(),
            budget: playbook_core::HuddleBudget {
                max_tokens: 2_000,
                max_cost_microunits: 200_000,
                max_duration_ms: 5_000,
                max_rounds: 1,
                max_participants: 3,
            },
            spent: crate::domain::HuddleCost::zero(),
            participants: Vec::new(),
            reports: Vec::new(),
            contradictions: Vec::new(),
            exhausted: false,
            escalation: None,
            partition: Vec::new(),
            field_fingerprint: "f".into(),
            at_ms: 1,
            debt_before: 0,
            ytg_milli_before: 0,
            rework_avoided: 0,
        };
        let weak = HuddleParticipant {
            personnel_id: PersonnelId::new("weak-model").unwrap(),
            role: HuddleRole::Contributor,
            disclosure_profile: DisclosureProfile {
                include_gates: false,
                include_peer_reports: false,
                include_strategic_state: false,
                max_evidence_refs: 0,
            },
        };
        let gate = Gate::declare(
            GateId::new("implementation").unwrap(),
            "compiler accepts",
            1_000_000,
            Vec::new(),
            vec![EvidenceClass::CompilerResult],
            true,
        )
        .unwrap();
        let packet = build_packet(
            &huddle,
            &weak,
            "compile the implementation",
            &[gate],
            &[],
            Assignment {
                personnel_id: weak.personnel_id.clone(),
                operation: "compile".into(),
                target_gate: None,
            },
        );
        assert!(packet.relevant_gates.is_empty());
        assert!(packet.relevant_evidence.is_empty());
        assert!(packet.contradictions.is_empty());
        assert_eq!(packet.budget.max_tokens, 2_000);
        assert_eq!(
            packet.requested_response_schema,
            "https://playbook.local/schemas/huddle_report.json"
        );
    }

    struct Fixed;
    impl HuddleDispatch for Fixed {
        fn huddle_dispatch(
            &mut self,
            _packet: &serde_json::Value,
        ) -> Result<serde_json::Value, PlaybookError> {
            Ok(json!({
                "participant": "worker",
                "verified_progress": [],
                "blockers": [],
                "changed_assumptions": [],
                "dependency_changes": [],
                "contradictions": [],
                "proposed_action": null,
                "confidence_milli": 9_000
            }))
        }
    }

    #[test]
    fn dispatch_normalizes_a_structured_response() {
        let packet = HuddleContextPacket {
            huddle_id: HuddleId::new("h-2").unwrap(),
            trigger: HuddleTrigger {
                kind: HuddleTriggerKind::ManualAuthority,
                reason_code: "manual".into(),
                evidence_refs: Vec::new(),
                originating_event: EventId::new("evt-2").unwrap(),
            },
            task_fragment: "task".into(),
            relevant_gates: Vec::new(),
            relevant_evidence: Vec::new(),
            current_assignment: Assignment {
                personnel_id: PersonnelId::new("worker").unwrap(),
                operation: "inspect".into(),
                target_gate: None,
            },
            dependency_changes: Vec::new(),
            contradictions: Vec::new(),
            budget: ParticipantBudget {
                max_tokens: 100,
                max_duration_ms: 1_000,
            },
            requested_response_schema: "schema".into(),
        };
        let report = dispatch_packet(&mut Fixed, &packet).unwrap();
        assert_eq!(report.participant.as_str(), "worker");
        assert_eq!(report.confidence_milli, 9_000);
    }

    #[test]
    fn default_regulator_keeps_models_narrow() {
        let regulator = DefaultRegulator;
        let trigger = HuddleTrigger {
            kind: HuddleTriggerKind::Strategic,
            reason_code: "fourth_down_disposition".into(),
            evidence_refs: Vec::new(),
            originating_event: EventId::new("evt-3").unwrap(),
        };
        let person = |kind| Personnel {
            id: PersonnelId::new("p").unwrap(),
            kind,
            display_name: "p".into(),
            cost_tokens: 10,
            latency_ms: 5,
            capability_fit_ppm: PPM_SCALE,
            reliability_ppm: PPM_SCALE,
            quota_remaining_ppm: PPM_SCALE,
            evidence_quality_ppm: PPM_SCALE,
            own_ytg_milli: None,
        };
        let model = regulator.build_disclosure(&person(PersonnelKind::Model), &trigger);
        assert!(!model.include_gates);
        assert!(!model.include_strategic_state);
        let human = regulator.build_disclosure(&person(PersonnelKind::HumanEscalation), &trigger);
        assert!(human.include_strategic_state);
    }
}
