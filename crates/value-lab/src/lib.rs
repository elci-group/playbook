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
pub mod derive;
pub mod frozen;
pub mod integrity;
pub mod lab;
pub mod live;
pub mod report;
pub mod run;
pub mod stats;
pub mod value;

pub use corpus::{corpus_for_study, Assignment, Corpus, Stratum, TaskClass};
pub use live::{chat, corpus, free_models, play, validated, Completion, DraftRow, LiveTask};
pub use derive::decision_snapshots;
pub use frozen::{DecisionSnapshot, ExperimentalSnap, ModelEntry, PersonnelOption, Regime};
pub use integrity::{
    forbidden_authority_detected, gather, integrity_gate, verify_protocol_unchanged,
    IntegrityInputs, IntegrityVerdict, ProtocolHash, ResearchIntegrityEvidence,
};
pub use lab::emit_protocol;
pub use lab::{
    ablation_table, classify, huddle_comparison, lastdown_audit, stopped_by, AblationRow, Fault,
    FaultClass, FaultSchedule, FrozenThresholds, LabHealth, Phase, Verdict,
};
pub use report::{
    emit_ablations, emit_corpus, emit_economics, emit_failures, emit_replication, emit_report,
    emit_scoreboard, emit_shadow, Scoreboard,
};
pub use run::{
    pareto_frontier, read_results, scoreboard, vectors, write_results, RegimeVector, RunRecord,
    ScoreRow,
};
pub use stats::{bootstrap_ci, mean, median, paired_differences, percentile, stddev};
pub use value::{ocav, regret, utility, ShadowPrediction, Utility, UtilityWeights};
