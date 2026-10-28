//! Fourth-down doctrine.
//!
//! The utility order matches Lastdown: a convert whose probability sits under
//! `min_attempt` is disqualified. Playbook names that choice Convert and adds
//! Abandon for an invalid, unsafe, or hopeless drive.
//!
//! The continuation package carries measured field state and omits model
//! identity, self-assessed confidence, model YTG, and competitive rankings.

#![forbid(unsafe_code)]

use serde_json::Value;

use playbook_core::{
    accept_schema, leaks_competitive_metadata, scale_by_probability, ArtefactRef, BudgetState,
    Constraint, ContinuationPackage, Disposition, DriveId, EvidenceId, GateId, ObjectiveState,
    TaskContract, TaskIntegrity, SCHEMA_MAGIC, SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoctrineInput {
    pub integrity: TaskIntegrity,
    pub full_utility: u64,
    pub banked_utility: u64,
    pub p_score: u32,
    pub p_preserve: u32,
    pub p_finish_after_handoff: u32,
    pub min_attempt: u32,
    pub reset_bonus_ppm: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoctrineDecision {
    pub schema_version: u32,
    pub u_convert: u64,
    pub u_field_goal: u64,
    pub u_reset: u64,
    pub u_punt: u64,
    pub p_score: u32,
    pub score_disqualified: bool,
    pub chosen: Disposition,
}

pub fn evaluate(input: &DoctrineInput) -> DoctrineDecision {
    if input.integrity != TaskIntegrity::Sound {
        return DoctrineDecision {
            schema_version: SCHEMA_VERSION,
            u_convert: 0,
            u_field_goal: 0,
            u_reset: 0,
            u_punt: 0,
            p_score: input.p_score,
            score_disqualified: true,
            chosen: Disposition::Abandon,
        };
    }
    let score_disqualified = input.p_score < input.min_attempt;
    let u_convert = if score_disqualified {
        0
    } else {
        scale_by_probability(input.p_score, input.full_utility)
    };
    let u_field_goal = input.banked_utility;
    let remaining = input.full_utility.saturating_sub(input.banked_utility);
    let after = scale_by_probability(input.p_finish_after_handoff, remaining)
        .saturating_add(input.banked_utility);
    let u_punt = scale_by_probability(input.p_preserve, after);
    let u_reset = u_punt
        .saturating_mul(1_000_000u64.saturating_add(u64::from(input.reset_bonus_ppm)))
        / 1_000_000;
    let mut chosen = Disposition::Punt;
    let mut best = u_punt;
    if u_reset > best {
        chosen = Disposition::Reset;
        best = u_reset;
    }
    if u_field_goal > best {
        chosen = Disposition::FieldGoal;
        best = u_field_goal;
    }
    if u_convert > best {
        chosen = Disposition::Convert;
        best = u_convert;
    }
    if input.banked_utility == 0
        && u_convert == 0
        && u_field_goal == 0
        && u_punt == 0
        && u_reset == 0
    {
        chosen = Disposition::Abandon;
    }
    let _ = best;
    DoctrineDecision {
        schema_version: SCHEMA_VERSION,
        u_convert,
        u_field_goal,
        u_reset,
        u_punt,
        p_score: input.p_score,
        score_disqualified,
        chosen,
    }
}

#[derive(Clone, Debug)]
pub struct PackageDraft {
    pub drive_id: DriveId,
    pub task_contract: TaskContract,
    pub objective_state: ObjectiveState,
    pub validated_artefacts: Vec<ArtefactRef>,
    pub unsatisfied_gates: Vec<GateId>,
    pub evidence_refs: Vec<EvidenceId>,
    pub failure_refs: Vec<EvidenceId>,
    pub available_capabilities: Vec<playbook_core::CapabilityId>,
    pub budget_state: BudgetState,
    pub environment_snapshot: Value,
    pub continuation_constraints: Vec<Constraint>,
}

pub fn seal_package(draft: PackageDraft) -> Result<ContinuationPackage, PlaybookError> {
    accept_schema(SCHEMA_VERSION)?;
    ContinuationPackage {
        magic: SCHEMA_MAGIC.to_string(),
        schema_version: SCHEMA_VERSION,
        drive_id: draft.drive_id,
        task_contract: draft.task_contract,
        objective_state: draft.objective_state,
        validated_artefacts: draft.validated_artefacts,
        unsatisfied_gates: draft.unsatisfied_gates,
        evidence_refs: draft.evidence_refs,
        failure_refs: draft.failure_refs,
        available_capabilities: draft.available_capabilities,
        budget_state: draft.budget_state,
        environment_snapshot: draft.environment_snapshot,
        continuation_constraints: draft.continuation_constraints,
        integrity_sha256: String::new(),
    }
    .seal_unchecked()
}

pub fn package_leaks(package: &ContinuationPackage) -> bool {
    let bytes = serde_json::to_vec(package).unwrap_or_default();
    leaks_competitive_metadata(&bytes)
}

use playbook_core::PlaybookError;

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{
        forbidden_competitive_keys, leaks_competitive_metadata, CapabilityId, Constraint, Down,
        DriveBudget, DriveStatus, TaskIntegrity,
    };

    fn package_draft() -> PackageDraft {
        PackageDraft {
            drive_id: DriveId::new("drive-1").unwrap(),
            task_contract: TaskContract {
                statement: "complete the task".into(),
                workload_class: "engineering".into(),
                constraints: vec![],
                integrity: TaskIntegrity::Sound,
            },
            objective_state: ObjectiveState {
                measured_ytg_milli: 3_000,
                down: Down::FourthDown,
                possession_generation: 1,
                status: DriveStatus::Punted,
            },
            validated_artefacts: vec![],
            unsatisfied_gates: vec![GateId::new("integration").unwrap()],
            evidence_refs: vec![EvidenceId::new("evidence-1").unwrap()],
            failure_refs: vec![EvidenceId::new("failure-1").unwrap()],
            available_capabilities: vec![CapabilityId::new("elci.cargo").unwrap()],
            budget_state: BudgetState {
                initial: DriveBudget {
                    tokens: 10_000,
                    ..DriveBudget::zero()
                },
                consumed: DriveBudget {
                    tokens: 2_000,
                    ..DriveBudget::zero()
                },
                remaining_fraction_ppm: 800_000,
            },
            environment_snapshot: serde_json::json!({"platform": "linux"}),
            continuation_constraints: vec![Constraint {
                key: "offline".into(),
                value: "true".into(),
            }],
        }
    }

    fn input(p_score: u32, min_attempt: u32, banked: u64, reset_bonus: u32) -> DoctrineInput {
        DoctrineInput {
            integrity: TaskIntegrity::Sound,
            full_utility: 1_000,
            banked_utility: banked,
            p_score,
            p_preserve: 980_000,
            p_finish_after_handoff: 200_000,
            min_attempt,
            reset_bonus_ppm: reset_bonus,
        }
    }

    #[test]
    fn hail_mary_convert_is_disqualified() {
        let banned = evaluate(&input(400_000, 550_000, 0, 0));
        assert!(banned.score_disqualified);
        assert_eq!(banned.chosen, Disposition::Punt);
        let allowed = evaluate(&input(400_000, 100_000, 0, 0));
        assert_eq!(allowed.chosen, Disposition::Convert);
    }

    #[test]
    fn banked_value_can_take_the_field_goal() {
        let decision = evaluate(&DoctrineInput {
            integrity: TaskIntegrity::Sound,
            full_utility: 100,
            banked_utility: 40,
            p_score: 0,
            p_preserve: 980_000,
            p_finish_after_handoff: 0,
            min_attempt: 550_000,
            reset_bonus_ppm: 0,
        });
        assert_eq!(decision.chosen, Disposition::FieldGoal);
        assert_eq!(decision.u_field_goal, 40);
        assert_eq!(decision.u_punt, 39);
    }

    #[test]
    fn reset_bonus_must_beat_the_punt() {
        let decision = evaluate(&input(400_000, 550_000, 0, 100_000));
        assert_eq!(decision.chosen, Disposition::Reset);
        assert!(decision.u_reset > decision.u_punt);
    }

    #[test]
    fn unsafe_work_is_abandoned() {
        let decision = evaluate(&DoctrineInput {
            integrity: TaskIntegrity::Unsafe,
            ..input(900_000, 100_000, 500, 0)
        });
        assert_eq!(decision.chosen, Disposition::Abandon);
    }

    #[test]
    fn seal_package_preserves_measured_state_and_excludes_competitive_keys() {
        let package = seal_package(package_draft()).unwrap();
        assert_eq!(package.magic, "PLAYBOOK/1");
        assert_eq!(package.schema_version, 1);
        assert_eq!(package.objective_state.measured_ytg_milli, 3_000);
        assert_eq!(
            package.evidence_refs,
            vec![EvidenceId::new("evidence-1").unwrap()]
        );
        assert_eq!(
            package.failure_refs,
            vec![EvidenceId::new("failure-1").unwrap()]
        );
        assert!(!package_leaks(&package));

        let serialized = serde_json::to_vec(&package).unwrap();
        assert!(String::from_utf8_lossy(&serialized).contains("measured_ytg_milli"));
        let round_trip: ContinuationPackage = serde_json::from_slice(&serialized).unwrap();
        assert_eq!(round_trip.objective_state.measured_ytg_milli, 3_000);
        for key in forbidden_competitive_keys() {
            assert!(
                !String::from_utf8_lossy(&serialized).contains(key),
                "found {key}"
            );
        }
    }

    #[test]
    fn top_level_competitive_metadata_is_detected_in_serialized_json() {
        let package = seal_package(package_draft()).unwrap();
        let clean_value = serde_json::to_value(package).unwrap();
        for (key, value) in [
            ("previous_model_identity", serde_json::json!("model-x")),
            ("own_ytg_milli", serde_json::json!(7_000)),
        ] {
            let mut injected = clean_value.clone();
            injected.as_object_mut().unwrap().insert(key.into(), value);
            let serialized = serde_json::to_vec(&injected).unwrap();

            assert!(leaks_competitive_metadata(&serialized), "missed {key}");
        }
    }
}
