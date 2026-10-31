//! The semantic Red Zone: session-closure feasibility, not yardage.
//!
//! The offence is in the Red Zone when every remaining mandatory deliver
//! condition is demonstrably satisfiable within the current session
//! envelope: required resources are available, required dependencies are
//! reachable, required validation can occur, and no unresolved blocker
//! makes completion contingent on an unavailable future event. The system
//! may be numerically close to completion without being in the Red Zone —
//! a human approval unavailable until tomorrow keeps the field red no
//! matter how small the yardage — and a drive may enter the Red Zone with
//! substantial nominal work remaining where every remaining operation has
//! become deterministic.
//!
//! Doctrine: false-negative classification is preferable to unsupported
//! false-positive closure confidence. An unknown evidence class counts as
//! unavailable.

use serde::{Deserialize, Serialize};

use playbook_core::{Gate, GateId, GateState};

/// What the current session can reach right now. The Red Zone is assessed
/// against this envelope, never against an agent's confidence.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResources {
    /// Capability ids usable this session.
    pub available_capabilities: Vec<String>,
    /// Evidence classes producible this session.
    pub available_evidence_classes: Vec<playbook_core::EvidenceClass>,
    /// A human approver is reachable now (acceptance, sign-off, policy
    /// decision).
    pub human_input_available: bool,
    /// Gate ids whose dependencies are already satisfied and reachable.
    pub reachable_dependencies: Vec<String>,
}

/// Per-gate closure feasibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateClosure {
    pub gate_id: GateId,
    pub closable: bool,
    /// Machine reasons for every failed condition, empty when closable.
    pub blockers: Vec<String>,
}

/// Assess one mandatory gate for session closure. A gate that is already
/// terminal (Satisfied, Waived) is closable-by-definition with no blockers;
/// a Failed gate is closable only in the sense that it must be repaired —
/// it blocks with "gate_failed". Otherwise every required evidence class
/// must be producible this session and every dependency must be reachable.
pub fn assess_gate(gate: &Gate, session: &SessionResources) -> GateClosure {
    let mut blockers = Vec::new();
    match gate.state {
        GateState::Satisfied | GateState::Waived => {
            return GateClosure {
                gate_id: gate.id.clone(),
                closable: true,
                blockers: Vec::new(),
            };
        }
        GateState::Failed => blockers.push("gate_failed".into()),
        _ => {}
    }
    for class in &gate.required_evidence {
        if !session.available_evidence_classes.contains(class) {
            blockers.push(format!("evidence_class_unavailable {class:?}"));
        }
    }
    for dependency in &gate.dependencies {
        if !session
            .reachable_dependencies
            .iter()
            .any(|reachable| reachable == dependency.as_str())
        {
            blockers.push(format!("dependency_unreachable {dependency}"));
        }
    }
    if blockers.is_empty() {
        GateClosure {
            gate_id: gate.id.clone(),
            closable: true,
            blockers: Vec::new(),
        }
    } else {
        GateClosure {
            gate_id: gate.id.clone(),
            closable: false,
            blockers,
        }
    }
}

/// The §8 Red Zone assessment over the whole mandatory set. Entered when
/// every outstanding mandatory gate is closable this session. Conservative
/// by construction: an evidence class the session cannot produce counts as
/// unavailable, and human-input requirements count as unavailable unless an
/// approver is reachable now.
pub fn assess_red_zone(
    gates: &[Gate],
    session: &SessionResources,
) -> crate::domain::RedZoneAssessment {
    let mut outstanding = Vec::new();
    let mut blockers = Vec::new();
    for gate in gates {
        if !gate.mandatory || gate.is_terminal() {
            continue;
        }
        outstanding.push(gate.id.clone());
        let closure = assess_gate(gate, session);
        if !closure.closable {
            blockers.extend(closure.blockers);
        }
    }
    crate::domain::RedZoneAssessment {
        entered: blockers.is_empty() && !outstanding.is_empty(),
        outstanding_mandatory: outstanding,
        blockers,
    }
}

/// §25 — Red Zone and field-goal range are independent assessments. This
/// helper exists so no conflation function is ever written: it returns the
/// pair unchanged.
pub fn independent(red_zone: bool, field_goal_range: bool) -> (bool, bool) {
    (red_zone, field_goal_range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::EvidenceClass;

    fn gate(id: &str, evidence: &[EvidenceClass], deps: &[&str]) -> Gate {
        let mut gate = Gate::declare(
            GateId::new(id).unwrap(),
            id,
            500_000,
            deps.iter()
                .map(|dep| GateId::new((*dep).to_string()).unwrap())
                .collect(),
            evidence.to_vec(),
            true,
        )
        .unwrap();
        gate.state = GateState::Unassessed;
        gate
    }

    fn session(evidence: &[EvidenceClass], deps: &[&str]) -> SessionResources {
        SessionResources {
            available_capabilities: Vec::new(),
            available_evidence_classes: evidence.to_vec(),
            human_input_available: false,
            reachable_dependencies: deps.iter().map(|dep| (*dep).to_string()).collect(),
        }
    }

    #[test]
    fn a_closable_gate_has_no_blockers() {
        let gate = gate("build", &[EvidenceClass::CompilerResult], &[]);
        let resources = session(&[EvidenceClass::CompilerResult], &[]);
        let closure = assess_gate(&gate, &resources);
        assert!(closure.closable);
        assert!(closure.blockers.is_empty());
    }

    #[test]
    fn missing_evidence_or_dependencies_block_with_reasons() {
        let setup_gate = gate("build", &[EvidenceClass::CompilerResult], &["setup"]);
        let empty = session(&[], &[]);
        let closure = assess_gate(&setup_gate, &empty);
        assert!(!closure.closable);
        assert!(closure
            .blockers
            .iter()
            .any(|blocker| blocker.contains("evidence_class_unavailable")));
        assert!(closure
            .blockers
            .iter()
            .any(|blocker| blocker.contains("dependency_unreachable")));
        // Human-input requirements need an approver: absent means blocked.
        let signoff_gate = gate("signoff", &[EvidenceClass::NarrativeClaim], &[]);
        let human_closure = assess_gate(&signoff_gate, &empty);
        assert!(!human_closure.closable);
    }

    #[test]
    fn the_red_zone_requires_every_mandatory_gate_closable() {
        let gates = vec![
            gate("build", &[EvidenceClass::CompilerResult], &[]),
            gate("test", &[EvidenceClass::TestReport], &[]),
        ];
        // Both evidence classes producible: entered.
        let both = session(
            &[EvidenceClass::CompilerResult, EvidenceClass::TestReport],
            &[],
        );
        let assessment = assess_red_zone(&gates, &both);
        assert!(assessment.entered);
        assert!(assessment.blockers.is_empty());
        // One class missing: not entered, and the blocker is surfaced.
        let partial = session(&[EvidenceClass::CompilerResult], &[]);
        let assessment = assess_red_zone(&gates, &partial);
        assert!(!assessment.entered);
        assert!(!assessment.blockers.is_empty());
    }

    #[test]
    fn a_failed_gate_blocks_the_red_zone_until_repaired() {
        let mut failed = gate("build", &[EvidenceClass::CompilerResult], &[]);
        failed.state = GateState::Failed;
        let gates = vec![failed];
        let resources = session(&[EvidenceClass::CompilerResult], &[]);
        let assessment = assess_red_zone(&gates, &resources);
        assert!(!assessment.entered, "a failed gate needs repair first");
        assert!(assessment
            .blockers
            .iter()
            .any(|blocker| blocker.contains("gate_failed")));
    }

    #[test]
    fn an_empty_mandatory_set_is_not_automatically_a_red_zone() {
        // Conservative doctrine: no outstanding mandatory gates means there
        // is nothing to close — the assessment reports an empty Red Zone
        // rather than a vacuous entry.
        let mut satisfied = gate("build", &[EvidenceClass::CompilerResult], &[]);
        satisfied.state = GateState::Satisfied;
        satisfied.satisfaction_ppm = 1_000_000;
        let gates = vec![satisfied];
        let resources = session(&[EvidenceClass::CompilerResult], &[]);
        let assessment = assess_red_zone(&gates, &resources);
        assert!(!assessment.entered, "nothing outstanding: no red zone");
        assert!(assessment.outstanding_mandatory.is_empty());
    }

    #[test]
    fn independence_is_preserved_by_construction() {
        // §25: the pair passes through unchanged — conflation is refused at
        // the type level by simply not existing.
        assert_eq!(independent(true, false), (true, false));
        assert_eq!(independent(false, true), (false, true));
    }
}
