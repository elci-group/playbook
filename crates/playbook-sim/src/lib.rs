//! Deterministic replay, the MVP drive, and failure-isolated adapters.
//!
//! Policy comparison resimulates a historical log. It does not edit the log.

#![forbid(unsafe_code)]

mod harness;
mod mvp;
mod normalise;

pub use harness::{
    gate_one, open_drive, open_with, person, quota_pressure, roster, standard_gates, task_contract,
    weighted_gate,
};
pub use mvp::{
    compare_policies, control_duplicate_failures, run_mvp, stage_names, MvpReport, PolicyComparison,
};
pub use normalise::{normalise_test_report, try_tov_normalise, NormaliseOutcome};

pub use playbook_core::{
    leaks_competitive_metadata, ArtefactId, ArtefactRef, Disposition, Down, DriveBudget, DriveId,
    DriveStatus, Evidence, EvidenceClass, EvidenceId, FailureClass, Gate, GateId, Measurement,
    Personnel, PersonnelId, PersonnelKind, PlayDefinition, PlayKind, PlayStatus, PlaybookError,
    SideEffectClass, TaskIntegrity, ToolFailureKind, PPM_SCALE,
};
pub use playbook_elci::{classify_status, CapabilityPlane, FixturePlane};
pub use playbook_events::DriveEvent;
pub use playbook_lastdown::package_leaks;
pub use playbook_policy::Policy;
pub use playbook_routing::HistoricalPriorTable;
pub use playbook_security::authorize;
pub use playbook_state::{
    apply, classify_measurement, fold, recover, started_budget, trajectory, ApplyMode, Authority,
    DriveState,
};
