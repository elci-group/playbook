//! Blitz economics and post-blitz learning.
//!
//! A blitz purchases recovery latency with redundant inference. This module
//! prices the purchase after the whistle: what verified recovery cost, what
//! the parallel premium was over the sequential counterfactual, what early
//! cancellation saved, and what the scouting rows say about who is worth
//! sending next time. Every number derives deterministically from the
//! closed record and the sequential estimate; the ledger remembers nothing
//! the record does not show.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::{BlitzRecord, WorkerScouting};

/// The counterfactual: what one sequential worker was estimated to need for
/// the same defensive objective. The premium and the latency gain are
/// measured against this estimate, never against a narrative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequentialEstimate {
    /// Estimated time to verified recovery for the sequential strategy.
    pub ttr_ms: u64,
    /// Estimated total cost of that sequential attempt.
    pub cost_microunits: u64,
}

/// The economic reconstruction of one closed blitz. Rates are per-mille;
/// every field is saturating integer arithmetic over the record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct BlitzReport {
    /// Race clock to the first candidate of any quality.
    pub time_to_first_candidate_ms: Option<u64>,
    /// Race clock to the first candidate that passed verification.
    pub time_to_first_verified_ms: Option<u64>,
    /// Race clock to committed recovery.
    pub time_to_recovery_ms: Option<u64>,
    /// Workers producing a verified candidate over workers fielded.
    pub worker_success_rate_ppm: u32,
    /// Distinct providers with at least one verified candidate over
    /// distinct providers fielded.
    pub provider_success_rate_ppm: u32,
    /// Distinct model families with at least one verified candidate over
    /// distinct families fielded.
    pub model_success_rate_ppm: u32,
    /// Verified candidates over emitted candidates; `None` when nothing was
    /// emitted.
    pub candidate_quality_ppm: Option<u32>,
    pub raw_parallelism: u16,
    pub effective_parallelism_milli: u32,
    pub tokens_to_recovery: u64,
    pub cost_to_recovery_microunits: u64,
    /// `record.spent.tokens − tokens_to_recovery`, saturating: everything
    /// the race kept spending after the objective was met. An unrecovered
    /// blitz keeps all of it — the point of recovery never arrived.
    pub tokens_wasted_after_recovery: u64,
    /// Avoided tokens over everything cancellable
    /// (`avoided / (avoided + wasted)`); `None` when there was nothing to
    /// cancel.
    pub cancellation_efficiency_ppm: Option<u32>,
    /// Sequential estimate minus actual recovery time; `None` with no
    /// winner or no gain.
    pub latency_gain_ms: Option<u64>,
    /// Record total cost minus the sequential estimate. Negative means the
    /// blitz was the cheaper strategy outright.
    pub blitz_premium_microunits: i64,
    /// Premium per full second of latency gained. `None` when the gain
    /// prices no whole second; `Some(0)` when the premium is non-positive.
    pub cost_per_second_recovered: Option<u64>,
}

/// Integer parts-per-million rate; an empty denominator rates 0, never an
/// error.
fn rate_ppm(numerator: u64, denominator: u64) -> u32 {
    numerator
        .checked_mul(1_000_000)
        .and_then(|scaled| scaled.checked_div(denominator))
        .map(|rate| rate.min(u64::from(u32::MAX)) as u32)
        .unwrap_or(0)
}

/// Distinct values of one scouting dimension, counting either every fielded
/// row or only the rows that verified.
fn distinct_dimension(
    results: &[WorkerScouting],
    verified_only: bool,
    dimension: fn(&WorkerScouting) -> &str,
) -> usize {
    results
        .iter()
        .filter(|row| !verified_only || row.candidate_verified)
        .map(dimension)
        .collect::<BTreeSet<_>>()
        .len()
}

/// Derive the report from a closed record and the counterfactual
/// sequential estimate. A record with no outcome contributes only what it
/// demonstrably spent: the premium against the sequential baseline, and —
/// recovery never arriving — its entire token spend counted after the
/// point of recovery.
pub fn report(record: &BlitzRecord, sequential: &SequentialEstimate) -> BlitzReport {
    let spent_cost = i64::try_from(record.spent.cost_microunits).unwrap_or(i64::MAX);
    let sequential_cost = i64::try_from(sequential.cost_microunits).unwrap_or(i64::MAX);
    let premium = spent_cost - sequential_cost;
    let mut derived = BlitzReport {
        blitz_premium_microunits: premium,
        ..BlitzReport::default()
    };
    let Some(outcome) = record.outcome.as_ref() else {
        derived.tokens_wasted_after_recovery = record.spent.tokens;
        return derived;
    };
    let results = &outcome.worker_results;
    let fielded = results.len() as u64;
    let verified = results.iter().filter(|row| row.candidate_verified).count() as u64;
    let emitted = results.iter().filter(|row| row.produced_candidate).count() as u64;
    derived.time_to_first_candidate_ms = outcome.first_candidate_ms;
    derived.time_to_first_verified_ms = outcome.time_to_verified_ms;
    derived.time_to_recovery_ms = outcome.time_to_recovery_ms;
    derived.worker_success_rate_ppm = rate_ppm(verified, fielded);
    derived.provider_success_rate_ppm = rate_ppm(
        distinct_dimension(results, true, |row| row.provider.as_str()) as u64,
        distinct_dimension(results, false, |row| row.provider.as_str()) as u64,
    );
    derived.model_success_rate_ppm = rate_ppm(
        distinct_dimension(results, true, |row| row.model_family.as_str()) as u64,
        distinct_dimension(results, false, |row| row.model_family.as_str()) as u64,
    );
    derived.candidate_quality_ppm = (emitted > 0).then(|| rate_ppm(verified, emitted));
    derived.raw_parallelism = outcome.raw_parallelism;
    derived.effective_parallelism_milli = outcome.effective_parallelism_milli;
    derived.tokens_to_recovery = outcome.tokens_to_recovery;
    derived.cost_to_recovery_microunits = outcome.cost_to_recovery_microunits;
    let wasted = record
        .spent
        .tokens
        .saturating_sub(outcome.tokens_to_recovery);
    derived.tokens_wasted_after_recovery = wasted;
    derived.cancellation_efficiency_ppm = outcome.cancellation.as_ref().and_then(|cancel| {
        let denominator = cancel.avoided_tokens.saturating_add(wasted);
        (denominator > 0).then(|| rate_ppm(cancel.avoided_tokens, denominator))
    });
    derived.latency_gain_ms = match (outcome.winner.as_ref(), outcome.time_to_recovery_ms) {
        (Some(_), Some(ttr)) if sequential.ttr_ms > ttr => Some(sequential.ttr_ms - ttr),
        _ => None,
    };
    derived.cost_per_second_recovered = match derived.latency_gain_ms {
        Some(gain_ms) if gain_ms / 1_000 > 0 => {
            if premium <= 0 {
                Some(0)
            } else {
                Some(premium as u64 / (gain_ms / 1_000))
            }
        }
        _ => None,
    };
    derived
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        BlitzClass, BlitzClosure, BlitzCost, BlitzOutcome, BlitzResourceMode, CancelSummary,
        SelectedWinner, Severity,
    };
    use playbook_core::{BlitzId, CandidateId, PersonnelId, WorkerId};

    fn scouting(
        id: &str,
        provider: &str,
        family: &str,
        produced: bool,
        verified: bool,
    ) -> WorkerScouting {
        WorkerScouting {
            personnel_id: PersonnelId::new(format!("personnel-{id}")).unwrap(),
            provider: provider.to_string(),
            model_family: family.to_string(),
            wave: 1,
            produced_candidate: produced,
            candidate_verified: verified,
            tokens: 5_000,
            cost_microunits: 400_000,
            latency_ms: 3_000,
        }
    }

    fn recovered_record() -> BlitzRecord {
        BlitzRecord {
            id: BlitzId::new("blitz-1").unwrap(),
            class: BlitzClass::Instinctive,
            closure: BlitzClosure::Recovered,
            reason_code: "pressure_floor".to_string(),
            target_hash: "abc".to_string(),
            resource_mode: BlitzResourceMode::LatencyCritical,
            severity: Severity::Critical,
            outcome: Some(BlitzOutcome {
                target_hash: "abc".to_string(),
                winner: Some(SelectedWinner {
                    candidate_id: CandidateId::new("cand-2").unwrap(),
                    worker_id: WorkerId::new("w-b").unwrap(),
                    at_ms: 4_000,
                    human_confirmed: false,
                }),
                first_candidate_ms: Some(3_500),
                time_to_verified_ms: Some(4_000),
                time_to_recovery_ms: Some(4_000),
                raw_parallelism: 4,
                effective_parallelism_milli: 3_400,
                correlated_failures: 1,
                tokens_to_recovery: 22_000,
                cost_to_recovery_microunits: 1_800_000,
                cancellation: Some(CancelSummary {
                    cancelled: 3,
                    latency_ms: 120,
                    avoided_tokens: 6_000,
                    avoided_cost_microunits: 500_000,
                }),
                worker_results: vec![
                    scouting("w-a", "alpha", "f1", false, false),
                    scouting("w-b", "alpha", "f1", true, true),
                    scouting("w-c", "beta", "f2", true, false),
                    scouting("w-d", "gamma", "f3", true, true),
                ],
                failure: None,
            }),
            spent: BlitzCost {
                tokens: 30_000,
                cost_microunits: 2_500_000,
                duration_ms: 8_000,
                workers: 4,
                waves: 1,
            },
            at_ms: 1_000,
            closed_at_ms: 9_000,
        }
    }

    fn sequential() -> SequentialEstimate {
        SequentialEstimate {
            ttr_ms: 10_000,
            cost_microunits: 1_000_000,
        }
    }

    #[test]
    fn full_derivation_is_deterministic() {
        let record = recovered_record();
        let first = report(&record, &sequential());
        assert_eq!(first, report(&record, &sequential()));
        assert_eq!(first.time_to_first_candidate_ms, Some(3_500));
        assert_eq!(first.time_to_first_verified_ms, Some(4_000));
        assert_eq!(first.time_to_recovery_ms, Some(4_000));
        // Two of four workers verified.
        assert_eq!(first.worker_success_rate_ppm, 500_000);
        // Two of three providers and two of three families verified.
        assert_eq!(first.provider_success_rate_ppm, 666_666);
        assert_eq!(first.model_success_rate_ppm, 666_666);
        // Two of three emitted candidates verified.
        assert_eq!(first.candidate_quality_ppm, Some(666_666));
        assert_eq!(first.raw_parallelism, 4);
        assert_eq!(first.effective_parallelism_milli, 3_400);
        assert_eq!(first.tokens_to_recovery, 22_000);
        assert_eq!(first.cost_to_recovery_microunits, 1_800_000);
    }

    #[test]
    fn premium_gain_waste_and_cancellation_math() {
        let derived = report(&recovered_record(), &sequential());
        // 2_500_000 − 1_000_000.
        assert_eq!(derived.blitz_premium_microunits, 1_500_000);
        // 10_000 − 4_000.
        assert_eq!(derived.latency_gain_ms, Some(6_000));
        // 1_500_000 over six whole seconds.
        assert_eq!(derived.cost_per_second_recovered, Some(250_000));
        // 30_000 − 22_000.
        assert_eq!(derived.tokens_wasted_after_recovery, 8_000);
        // 6_000 avoided of 14_000 cancellable.
        assert_eq!(derived.cancellation_efficiency_ppm, Some(428_571));

        // A blitz cheaper than the sequential baseline carries a negative
        // premium and prices no seconds.
        let mut cheaper = recovered_record();
        cheaper.spent.cost_microunits = 800_000;
        let derived = report(&cheaper, &sequential());
        assert_eq!(derived.blitz_premium_microunits, -200_000);
        assert_eq!(derived.cost_per_second_recovered, Some(0));

        // A sub-second gain prices no whole second.
        let mut tight_baseline = sequential();
        tight_baseline.ttr_ms = 4_500;
        let derived = report(&recovered_record(), &tight_baseline);
        assert_eq!(derived.latency_gain_ms, Some(500));
        assert_eq!(derived.cost_per_second_recovered, None);
    }

    #[test]
    fn cancellation_with_nothing_to_cancel_reports_none() {
        let mut record = recovered_record();
        if let Some(outcome) = record.outcome.as_mut() {
            outcome.cancellation = Some(CancelSummary {
                cancelled: 0,
                latency_ms: 0,
                avoided_tokens: 0,
                avoided_cost_microunits: 0,
            });
            outcome.tokens_to_recovery = record.spent.tokens;
        }
        let derived = report(&record, &sequential());
        assert_eq!(derived.tokens_wasted_after_recovery, 0);
        assert_eq!(derived.cancellation_efficiency_ppm, None);
    }

    #[test]
    fn unrecovered_records_keep_only_what_they_cost() {
        let mut record = recovered_record();
        record.closure = BlitzClosure::Exhausted;
        record.outcome = None;
        record.spent.tokens = 12_000;
        record.spent.cost_microunits = 900_000;
        let derived = report(&record, &sequential());
        assert_eq!(derived.time_to_first_candidate_ms, None);
        assert_eq!(derived.time_to_first_verified_ms, None);
        assert_eq!(derived.time_to_recovery_ms, None);
        assert_eq!(derived.worker_success_rate_ppm, 0);
        assert_eq!(derived.provider_success_rate_ppm, 0);
        assert_eq!(derived.model_success_rate_ppm, 0);
        assert_eq!(derived.candidate_quality_ppm, None);
        assert_eq!(derived.raw_parallelism, 0);
        assert_eq!(derived.effective_parallelism_milli, 0);
        assert_eq!(derived.tokens_to_recovery, 0);
        assert_eq!(derived.cost_to_recovery_microunits, 0);
        // Nothing was recovered, so everything spent sits after recovery.
        assert_eq!(derived.tokens_wasted_after_recovery, 12_000);
        assert_eq!(derived.cancellation_efficiency_ppm, None);
        assert_eq!(derived.latency_gain_ms, None);
        assert_eq!(derived.blitz_premium_microunits, -100_000);
        assert_eq!(derived.cost_per_second_recovered, None);
    }
}
