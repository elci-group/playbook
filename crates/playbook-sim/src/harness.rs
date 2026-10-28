//! Shared drive setup for the MVP and the failure-injection suite.

use playbook_core::{
    DriveBudget, DriveId, EvidenceClass, Gate, GateId, Personnel, PersonnelId, PersonnelKind,
    PlaybookError, TaskContract, TaskIntegrity, PPM_SCALE,
};
use playbook_elci::FixturePlane;
use playbook_policy::Policy;
use playbook_state::{started_budget, Authority};

pub fn person(id: &str, kind: PersonnelKind, cost: u64, own_ytg_milli: Option<u32>) -> Personnel {
    Personnel {
        id: PersonnelId::new(id).expect("personnel id"),
        kind,
        display_name: id.to_string(),
        cost_tokens: cost,
        latency_ms: 20,
        capability_fit_ppm: PPM_SCALE,
        reliability_ppm: 900_000,
        quota_remaining_ppm: PPM_SCALE,
        evidence_quality_ppm: PPM_SCALE,
        own_ytg_milli,
    }
}

pub fn roster() -> Vec<Personnel> {
    vec![
        person(
            "cargo-toolchain",
            PersonnelKind::DeterministicProcedure,
            40,
            None,
        ),
        person("rust-analyser", PersonnelKind::Tool, 30, None),
        person(
            "coding-specialist",
            PersonnelKind::SpecialistAgent,
            5_000,
            None,
        ),
    ]
}

pub fn task_contract(integrity: TaskIntegrity) -> TaskContract {
    TaskContract {
        statement: "satisfy the engineering deliver gates".into(),
        workload_class: "engineering".into(),
        constraints: Vec::new(),
        integrity,
    }
}

pub fn gate(
    id: &str,
    weight_ppm: u32,
    deps: &[&str],
    class: EvidenceClass,
) -> Result<Gate, PlaybookError> {
    Gate::declare(
        GateId::new(id)?,
        id,
        weight_ppm,
        deps.iter()
            .map(|dep| GateId::new(*dep))
            .collect::<Result<Vec<_>, _>>()?,
        vec![class],
        true,
    )
}

pub fn gate_one(id: &str, class: EvidenceClass) -> Gate {
    gate(id, 1_000_000, &[], class).expect("gate")
}

pub fn weighted_gate(id: &str, weight_ppm: u32, deps: &[&str], class: EvidenceClass) -> Gate {
    gate(id, weight_ppm, deps, class).expect("gate")
}

pub fn standard_gates() -> Result<Vec<Gate>, PlaybookError> {
    Ok(vec![
        gate(
            "implementation",
            400_000,
            &[],
            EvidenceClass::CompilerResult,
        )?,
        gate(
            "unit_tests",
            300_000,
            &["implementation"],
            EvidenceClass::TestReport,
        )?,
        gate(
            "integration_tests",
            300_000,
            &["unit_tests"],
            EvidenceClass::TestReport,
        )?,
    ])
}

pub fn open_drive(
    plane: FixturePlane,
    gates: Vec<Gate>,
) -> Result<(Authority, FixturePlane), PlaybookError> {
    open_with(plane, gates, Policy::builtin())
}

pub fn open_with(
    mut plane: FixturePlane,
    gates: Vec<Gate>,
    policy: Policy,
) -> Result<(Authority, FixturePlane), PlaybookError> {
    let mut authority = Authority::start(
        DriveId::new("drive-mvp")?,
        task_contract(TaskIntegrity::Sound),
        started_budget(1_000, 1_000),
        policy,
        roster(),
        1,
    )?;
    for gate in gates {
        authority.declare_gate(gate)?;
    }
    authority.preflight(&mut plane)?;
    Ok((authority, plane))
}

pub fn quota_pressure(quota: u64) -> DriveBudget {
    DriveBudget {
        provider_quota: quota,
        ..DriveBudget::zero()
    }
}
