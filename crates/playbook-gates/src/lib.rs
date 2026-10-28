//! Deliver-gate graph.
//!
//! Models may propose gates. Only evidence, or an explicit waiver, moves them.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;

use playbook_core::{
    Evidence, EvidenceId, Gate, GateId, GateState, PlaybookError, SatisfactionSource, Waiver,
};
use playbook_evidence::{score, targets_gate};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngestOutcome {
    pub ignored: bool,
    pub changed: Vec<GateId>,
    pub failed: Vec<GateId>,
    pub reopened: u32,
    pub environment: Option<serde_json::Value>,
}

pub fn apply_evidence(
    gates: &mut [Gate],
    evidence: &Evidence,
) -> Result<IngestOutcome, PlaybookError> {
    evidence.verify()?;
    let scored = score(evidence.class, &evidence.body)?;
    if evidence.class == playbook_core::EvidenceClass::NarrativeClaim || !scored.applies_to_gates {
        return Ok(IngestOutcome {
            ignored: true,
            changed: Vec::new(),
            failed: Vec::new(),
            reopened: 0,
            environment: scored.environment,
        });
    }

    let mut changed = Vec::new();
    let mut failed = Vec::new();
    let mut reopened = 0u32;
    for _ in 0..gates.len().max(1) {
        let terminal = terminal_ids(gates);
        let mut progressed = false;
        for gate in gates.iter_mut() {
            if !gate.required_evidence.contains(&evidence.class) {
                continue;
            }
            if !targets_gate(&evidence.body, gate.id.as_str()) {
                continue;
            }
            if !gate
                .dependencies
                .iter()
                .all(|dep| terminal.contains(dep.as_str()))
            {
                if gate.state == GateState::Unassessed {
                    gate.state = GateState::Blocked;
                    progressed = true;
                }
                continue;
            }
            let before = gate.satisfaction_ppm;
            let before_state = gate.state;
            if scored.failed && scored.satisfaction_ppm <= before {
                if before_state == GateState::Satisfied {
                    reopened += 1;
                }
                if before_state != GateState::Failed || before != scored.satisfaction_ppm {
                    gate.satisfaction_ppm = scored.satisfaction_ppm;
                    gate.state = if scored.satisfaction_ppm == 0 {
                        GateState::Failed
                    } else {
                        GateState::InProgress
                    };
                    gate.satisfaction_source = SatisfactionSource::Evidence;
                    gate.confidence_ppm = scored.satisfaction_ppm;
                    gate.satisfying_evidence = Some(evidence.id.clone());
                    changed.push(gate.id.clone());
                    failed.push(gate.id.clone());
                    progressed = true;
                }
            } else if !scored.failed && scored.satisfaction_ppm > before {
                gate.satisfaction_ppm = scored.satisfaction_ppm;
                gate.state = if gate.satisfaction_ppm == playbook_core::PPM_SCALE {
                    GateState::Satisfied
                } else {
                    GateState::InProgress
                };
                gate.satisfaction_source = SatisfactionSource::Evidence;
                gate.confidence_ppm = scored.satisfaction_ppm;
                gate.satisfying_evidence = Some(evidence.id.clone());
                changed.push(gate.id.clone());
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    Ok(IngestOutcome {
        ignored: false,
        changed,
        failed,
        reopened,
        environment: scored.environment,
    })
}

pub fn waive(gates: &mut [Gate], gate_id: &GateId, waiver: Waiver) -> Result<(), PlaybookError> {
    let gate = gates
        .iter_mut()
        .find(|gate| &gate.id == gate_id)
        .ok_or_else(|| PlaybookError::UnknownGate(gate_id.to_string()))?;
    gate.waiver = Some(waiver);
    gate.state = GateState::Waived;
    gate.satisfaction_source = SatisfactionSource::Waiver;
    Ok(())
}

pub fn invalidate_evidence(gates: &mut [Gate], evidence_id: &EvidenceId) -> Vec<GateId> {
    let mut failed = Vec::new();
    for gate in gates.iter_mut() {
        if gate.satisfying_evidence.as_ref() == Some(evidence_id) && gate.state != GateState::Waived
        {
            gate.satisfaction_ppm = 0;
            gate.confidence_ppm = 0;
            gate.state = GateState::Failed;
            gate.satisfaction_source = SatisfactionSource::Unset;
            gate.satisfying_evidence = None;
            failed.push(gate.id.clone());
        }
    }
    failed
}

pub fn is_complete(gates: &[Gate]) -> bool {
    let mandatory: Vec<&Gate> = gates.iter().filter(|gate| gate.mandatory).collect();
    !mandatory.is_empty() && mandatory.iter().all(|gate| gate.is_terminal())
}

pub fn mandatory_failed(gates: &[Gate]) -> bool {
    gates
        .iter()
        .any(|gate| gate.mandatory && gate.state == GateState::Failed)
}

pub fn open_mandatory(gates: &[Gate]) -> Option<&Gate> {
    gates
        .iter()
        .find(|gate| gate.mandatory && !gate.is_terminal())
}

fn terminal_ids(gates: &[Gate]) -> BTreeSet<String> {
    gates
        .iter()
        .filter(|gate| gate.is_terminal())
        .map(|gate| gate.id.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{Evidence, EvidenceClass, Gate, GateState, PlaybookError, Waiver};
    use serde_json::json;

    fn gate(id: &str, deps: &[&str], class: EvidenceClass) -> Gate {
        Gate::declare(
            playbook_core::GateId::new(id).unwrap(),
            id,
            500_000,
            deps.iter()
                .map(|dep| playbook_core::GateId::new(*dep).unwrap())
                .collect(),
            vec![class],
            true,
        )
        .unwrap()
    }

    #[test]
    fn dependency_blocks_satisfaction() {
        let mut gates = vec![
            gate("implementation", &[], EvidenceClass::CompilerResult),
            gate("unit_tests", &["implementation"], EvidenceClass::TestReport),
        ];
        let evidence = Evidence::seal(
            EvidenceClass::TestReport,
            json!({
                "gate_id": "unit_tests",
                "command": "cargo test",
                "repository_hash": "abc",
                "test_suite": "unit",
                "exit_code": 0,
                "passed": 2,
                "failed": 0,
                "ignored": 0,
                "timestamp": "1"
            }),
            1,
        )
        .unwrap();
        apply_evidence(&mut gates, &evidence).unwrap();
        assert_eq!(gates[1].state, GateState::Blocked);
        assert_eq!(gates[1].satisfaction_ppm, 0);
    }

    #[test]
    fn waiver_without_authority_is_rejected() {
        let err = Waiver::new(" ", "because").unwrap_err();
        assert_eq!(err, PlaybookError::WaiverAuthority);
    }
}
