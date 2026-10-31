//! `value-lab` — the independent research evaluator for Project VALUE.
//!
//! Playbook is the experimental subject. This crate decides what happened.
//! It reconstructs counterfactuals from frozen decision snapshots, computes
//! opportunity-cost-adjusted value and regret, scores Shadow Defence's
//! predictions against outcomes, classifies the study verdict against
//! pre-registered thresholds, and emits the VALUE artifacts. Nothing in
//! here imports Playbook's own view of whether it succeeded.
//!
//! Methodological law (directive §0): internal architectural correctness is
//! not evidence of external effectiveness. The lab exists so Playbook can
//! be proven wrong.

#![forbid(unsafe_code)]

pub mod corpus;
pub mod frozen;
pub mod lab;
pub mod report;
pub mod run;
pub mod stats;
pub mod value;

pub use corpus::{Assignment, Corpus, Stratum, TaskClass};
pub use frozen::{DecisionSnapshot, ExperimentalSnap, ModelEntry, PersonnelOption, Regime};
pub use lab::{
    ablation_table, classify, emit_protocol, huddle_comparison, lastdown_audit, stopped_by,
    AblationRow, Fault, FaultClass, FaultSchedule, FrozenThresholds, LabHealth, Phase,
    StopCondition, Verdict,
};
pub use report::emit_scoreboard;
pub use run::{RunRecord, Scoreboard, ScoreRow};
pub use stats::{bootstrap_ci, mean, median, paired_differences, percentile, stddev};
pub use value::{ocav, regret, utility, ShadowPrediction, Utility, UtilityWeights};
