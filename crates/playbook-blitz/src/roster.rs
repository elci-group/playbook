//! Defensive formation selection.
//!
//! The front office constructs the roster; Playbook calls the defence.
//! Selection optimises expected success × response speed × independence ×
//! availability, subject to cost. Model quality alone never selects a
//! defender, and naive sameness is not parallelism: eight workers sharing a
//! rate-limit pool are one attempt wearing eight jerseys. Independence is
//! computed from shared upstream failure domains, never from brand names.

use std::collections::BTreeSet;

use playbook_core::{PersonnelId, WorkerId};
use playbook_policy::{BlitzDiversity, BlitzRosterWeights};

use crate::domain::{BlitzResourceMode, WorkerSlot};

/// Latency normaliser for the speed component: 0 ms scores the full 1000
/// per-mille, a worker at or beyond this many milliseconds scores 0.
const LATENCY_NORMALISER_MS: u64 = 2_000;

/// Cost normaliser for the cost component: estimated spend at or beyond
/// this many microunits scores the full 1000 per-mille of cost.
const COST_NORMALISER_MICROUNITS: u64 = 2_000_000;

/// A selection candidate for the defensive formation. Every number is a
/// deterministic estimate; self-reported prestige never enters the score.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerCandidate {
    pub personnel_id: PersonnelId,
    pub worker_id: WorkerId,
    pub provider: String,
    pub model_family: String,
    /// Shared upstream failure domains (rate-limit pools, datacenters,
    /// parent models). Overlap here is what independence is measured from.
    pub failure_domains: BTreeSet<String>,
    pub estimated_latency_ms: u64,
    /// Expected success of one attempt, 0..=1_000_000.
    pub success_probability_ppm: u32,
    /// Provider health, 0..=1_000_000.
    pub availability_ppm: u32,
    pub estimated_cost_microunits: u64,
    /// Runs on local compute (as opposed to a remote provider).
    pub local: bool,
}

/// Scale `value` against `normaliser` to 0..=1000 per-mille: values at or
/// beyond the normaliser score 1000, zero scores 0, the rest are
/// proportional by integer division.
fn clamp_milli(value: u64, normaliser: u64) -> u32 {
    (value.min(normaliser).saturating_mul(1_000) / normaliser) as u32
}

/// Deterministic roster score for one candidate: success, speed,
/// independence, and availability components — each clamped to 0..=1000
/// per-mille — are weighted by `weights` and summed
/// huddle-participation-style, with the cost component subtracted.
///
/// * success = `success_probability_ppm / 1000`
/// * speed = `1000 − (estimated_latency_ms scaled against a 2_000 ms
///   normaliser)`: a 0 ms worker scores 1000, one at or beyond two seconds
///   scores 0
/// * independence carries a neutral 1000 in the base score. Independence is
///   a property of the formation, not of one worker, so the candidate's own
///   independence weight is applied through [`marginal_independence_milli`]
///   during selection: shared failure domains re-order and repair the
///   formation rather than flatter individual candidates.
/// * availability = `availability_ppm / 1000`
/// * cost = `estimated_cost_microunits scaled against a 2_000_000 microunit
///   normaliser`
pub fn worker_score(candidate: &WorkerCandidate, weights: &BlitzRosterWeights) -> i64 {
    let success = i64::from(candidate.success_probability_ppm.min(1_000_000) / 1_000);
    let speed_milli = 1_000u32.saturating_sub(clamp_milli(
        candidate.estimated_latency_ms,
        LATENCY_NORMALISER_MS,
    ));
    let availability = i64::from(candidate.availability_ppm.min(1_000_000) / 1_000);
    let cost = i64::from(clamp_milli(
        candidate.estimated_cost_microunits,
        COST_NORMALISER_MICROUNITS,
    ));
    success * i64::from(weights.success)
        + i64::from(speed_milli) * i64::from(weights.speed)
        + 1_000 * i64::from(weights.independence)
        + availability * i64::from(weights.availability)
        - cost * i64::from(weights.cost)
}

/// Whether two candidates sit in the same failure dimension — the same
/// provider when repairing the provider floor, the same model family when
/// repairing the family floor.
fn shares_dimension(
    left: &WorkerCandidate,
    right: &WorkerCandidate,
    provider_dimension: bool,
) -> bool {
    if provider_dimension {
        left.provider == right.provider
    } else {
        left.model_family == right.model_family
    }
}

fn family_count(picked: &[&WorkerCandidate], family: &str) -> usize {
    picked
        .iter()
        .filter(|candidate| candidate.model_family == family)
        .count()
}

fn provider_count(picked: &[&WorkerCandidate], provider: &str) -> usize {
    picked
        .iter()
        .filter(|candidate| candidate.provider == provider)
        .count()
}

fn distinct_providers(picked: &[&WorkerCandidate]) -> usize {
    picked
        .iter()
        .map(|candidate| candidate.provider.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn distinct_families(picked: &[&WorkerCandidate]) -> usize {
    picked
        .iter()
        .map(|candidate| candidate.model_family.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn floors_met(picked: &[&WorkerCandidate], diversity: &BlitzDiversity) -> bool {
    distinct_providers(picked) >= diversity.min_providers
        && distinct_families(picked) >= diversity.min_model_families
}

/// The canonical formation order: score-descending, then the resource-mode
/// bias among equally-scored candidates (remote first under
/// `POWER_CRITICAL`, local first under `NETWORK_CRITICAL`, indifferent
/// otherwise), then worker-id-ascending.
fn formation_order(
    left: &WorkerCandidate,
    right: &WorkerCandidate,
    weights: &BlitzRosterWeights,
    mode: BlitzResourceMode,
) -> std::cmp::Ordering {
    worker_score(right, weights)
        .cmp(&worker_score(left, weights))
        .then_with(|| match mode.prefers_remote() {
            // `local` ascending seats the remote worker first; inverted for
            // the local-first modes.
            Some(true) => left.local.cmp(&right.local),
            Some(false) => right.local.cmp(&left.local),
            None => std::cmp::Ordering::Equal,
        })
        .then_with(|| left.worker_id.as_str().cmp(right.worker_id.as_str()))
}

/// One greedy-cap repair: admit the best-scoring candidate that introduces
/// a missing provider (or family), removing the worst-scoring pick that is
/// redundant in that dimension, never exceeding `max_same_family` and never
/// leaving either diversity floor worse than before the swap.
fn repair_dimension<'a>(
    picked: &mut Vec<&'a WorkerCandidate>,
    pool: &[&'a WorkerCandidate],
    diversity: &BlitzDiversity,
    provider_dimension: bool,
) -> bool {
    let found = pool.iter().position(|pool_candidate| {
        let candidate = *pool_candidate;
        !picked.contains(&candidate)
            && !picked
                .iter()
                .any(|seated| shares_dimension(seated, candidate, provider_dimension))
            && family_count(picked, &candidate.model_family) < diversity.max_same_family
    });
    let Some(incoming) = found.map(|at| pool[at]) else {
        return false;
    };
    for index in (0..picked.len()).rev() {
        let outgoing = picked[index];
        let redundant = if provider_dimension {
            provider_count(picked, &outgoing.provider) > 1
        } else {
            family_count(picked, &outgoing.model_family) > 1
        };
        if !redundant {
            continue;
        }
        let mut next: Vec<&WorkerCandidate> = picked.clone();
        next.remove(index);
        next.push(incoming);
        let provider_floor = diversity.min_providers.min(distinct_providers(picked));
        let family_floor = diversity.min_model_families.min(distinct_families(picked));
        if distinct_providers(&next) >= provider_floor && distinct_families(&next) >= family_floor {
            picked.remove(index);
            picked.push(incoming);
            return true;
        }
    }
    false
}

/// Swap in repairs until both diversity floors hold or no admissible swap
/// remains. Each accepted swap strictly increases one distinct count, so
/// the loop terminates deterministically.
fn repair_floors<'a>(
    picked: &mut Vec<&'a WorkerCandidate>,
    pool: &[&'a WorkerCandidate],
    diversity: &BlitzDiversity,
) {
    loop {
        if floors_met(picked, diversity) {
            return;
        }
        let providers_before = distinct_providers(picked);
        let families_before = distinct_families(picked);
        let _ = repair_dimension(picked, pool, diversity, true);
        let _ = repair_dimension(picked, pool, diversity, false);
        if distinct_providers(picked) == providers_before
            && distinct_families(picked) == families_before
        {
            return;
        }
    }
}

/// Select the defensive formation: score-descending, worker-id-ascending on
/// ties, capped at `max_workers`, filtered by `min_score`, subject to the
/// diversity floors (`min_providers`, `min_model_families`,
/// `max_same_family`) and the resource-mode formation bias — under
/// `POWER_CRITICAL` remote candidates rank ahead of equally-scored local
/// ones, under `NETWORK_CRITICAL` the inverse, otherwise indifferent.
///
/// The greedy pass walks the canonical order, skipping candidates that
/// would push one `model_family` past `max_same_family`. If the diversity
/// floors are then unmet, the best-scoring candidates that repair the
/// missing provider or family are swapped in for the worst-scoring
/// redundant same-family picks. An empty formation is the honest default
/// when no candidate clears the floor.
pub fn select(
    candidates: &[WorkerCandidate],
    max_workers: u16,
    min_score: i64,
    diversity: &BlitzDiversity,
    weights: &BlitzRosterWeights,
    mode: BlitzResourceMode,
) -> Vec<WorkerCandidate> {
    let mut pool: Vec<&WorkerCandidate> = candidates
        .iter()
        .filter(|candidate| worker_score(candidate, weights) >= min_score)
        .collect();
    pool.sort_by(|left, right| formation_order(left, right, weights, mode));

    let cap = usize::from(max_workers);
    let mut picked: Vec<&WorkerCandidate> = Vec::new();
    for candidate in &pool {
        if picked.len() >= cap {
            break;
        }
        if family_count(&picked, &candidate.model_family) >= diversity.max_same_family {
            continue;
        }
        picked.push(candidate);
    }
    repair_floors(&mut picked, &pool, diversity);

    let mut chosen: Vec<WorkerCandidate> = picked.into_iter().cloned().collect();
    chosen.sort_by(|left, right| formation_order(left, right, weights, mode));
    chosen
}

/// Marginal independence of one extra worker in per-mille: 1000 minus the
/// largest Jaccard overlap (shared failure domains over union, per-mille)
/// against any already-selected failure-domain set. A first worker is fully
/// independent (1000); a worker sharing every declared domain with an
/// already-selected twin is a re-run, not a second attempt (0). Two workers
/// with no declared domains share nothing measurable (overlap 0).
pub fn marginal_independence_milli(failure_domains: &[String], selected: &[Vec<String>]) -> u32 {
    let domains: BTreeSet<&str> = failure_domains.iter().map(String::as_str).collect();
    let worst_overlap_milli = selected
        .iter()
        .map(|other| {
            let other_domains: BTreeSet<&str> = other.iter().map(String::as_str).collect();
            let shared = domains.intersection(&other_domains).count();
            let union = domains.union(&other_domains).count();
            // Per-mille Jaccard overlap; an empty union shares nothing.
            (shared.saturating_mul(1_000))
                .checked_div(union)
                .unwrap_or(0) as u32
        })
        .max()
        .unwrap_or(0);
    1_000 - worst_overlap_milli
}

/// Effective parallelism of a formation in per-mille: the sum of each
/// worker's marginal independence in dispatch order. Eight correlated
/// workers might total well under 8_000 milli — the raw headcount is not
/// the number of attempts the defence actually gets.
pub fn effective_parallelism_milli(workers: &[WorkerSlot]) -> u32 {
    let mut selected: Vec<Vec<String>> = Vec::new();
    let mut total: u32 = 0;
    for slot in workers {
        total = total.saturating_add(marginal_independence_milli(
            &slot.failure_domains,
            &selected,
        ));
        selected.push(slot.failure_domains.clone());
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights() -> BlitzRosterWeights {
        playbook_policy::Policy::builtin().blitz.roster_weights
    }

    fn diversity() -> BlitzDiversity {
        playbook_policy::Policy::builtin().blitz.diversity
    }

    fn candidate(id: &str, provider: &str, family: &str, domains: &[&str]) -> WorkerCandidate {
        WorkerCandidate {
            personnel_id: PersonnelId::new(format!("personnel-{id}")).unwrap(),
            worker_id: WorkerId::new(id).unwrap(),
            provider: provider.to_string(),
            model_family: family.to_string(),
            failure_domains: domains.iter().map(|domain| domain.to_string()).collect(),
            estimated_latency_ms: 500,
            success_probability_ppm: 900_000,
            availability_ppm: 900_000,
            estimated_cost_microunits: 100_000,
            local: false,
        }
    }

    fn slot(id: &str, domains: &[&str]) -> WorkerSlot {
        WorkerSlot {
            worker_id: WorkerId::new(id).unwrap(),
            personnel_id: PersonnelId::new(format!("personnel-{id}")).unwrap(),
            provider: "alpha".to_string(),
            model_family: "f1".to_string(),
            failure_domains: domains.iter().map(|domain| domain.to_string()).collect(),
            wave: 1,
            dispatched_at_ms: 0,
            deadline_ms: 1_000,
        }
    }

    #[test]
    fn selection_is_scored_capped_and_deterministic() {
        let weights = weights();
        let mut weak = candidate("w-d", "alpha", "f1", &["dc-1"]);
        weak.success_probability_ppm = 300_000;
        let candidates = vec![
            candidate("w-c", "alpha", "f1", &["dc-1"]),
            candidate("w-a", "alpha", "f1", &["dc-1"]),
            weak,
        ];
        let picked = select(
            &candidates,
            2,
            2_000,
            &diversity(),
            &weights,
            BlitzResourceMode::Normal,
        );
        let ids: Vec<&str> = picked.iter().map(|c| c.worker_id.as_str()).collect();
        // Score first; equal scores break by worker id ascending.
        assert_eq!(ids, vec!["w-a", "w-c"]);
        // Below the policy floor nobody fields: an empty formation is honest.
        assert!(select(
            &candidates,
            2,
            100_000,
            &diversity(),
            &weights,
            BlitzResourceMode::Normal
        )
        .is_empty());
    }

    #[test]
    fn diversity_floors_are_repaired() {
        let weights = weights();
        let diversity = diversity(); // min 2 providers, 2 families, 3 per family
        let mut beta = candidate("w-d", "beta", "f2", &["dc-2"]);
        beta.success_probability_ppm = 800_000; // scores below every alpha
        let candidates = vec![
            candidate("w-a", "alpha", "f1", &["dc-1"]),
            candidate("w-b", "alpha", "f1", &["dc-1"]),
            candidate("w-c", "alpha", "f1", &["dc-1"]),
            beta,
        ];
        let picked = select(
            &candidates,
            3,
            2_000,
            &diversity,
            &weights,
            BlitzResourceMode::Normal,
        );
        assert_eq!(picked.len(), 3);
        let providers: BTreeSet<&str> = picked.iter().map(|c| c.provider.as_str()).collect();
        let families: BTreeSet<&str> = picked.iter().map(|c| c.model_family.as_str()).collect();
        assert_eq!(providers.len(), 2);
        assert_eq!(families.len(), 2);
        // The best-scoring repair came in; the worst-scoring redundant
        // alpha made way, never exceeding the same-family cap.
        assert!(picked.iter().any(|c| c.provider == "beta"));
        assert_eq!(picked.iter().filter(|c| c.provider == "alpha").count(), 2);
        assert!(picked.iter().any(|c| c.worker_id.as_str() == "w-a"));
        assert!(picked.iter().any(|c| c.worker_id.as_str() == "w-b"));
    }

    #[test]
    fn max_same_family_is_respected() {
        let weights = weights();
        let diversity = BlitzDiversity {
            min_providers: 1,
            min_model_families: 1,
            max_same_family: 2,
        };
        let candidates = vec![
            candidate("w-a", "alpha", "f1", &["dc-1"]),
            candidate("w-b", "alpha", "f1", &["dc-1"]),
            candidate("w-c", "alpha", "f1", &["dc-1"]),
            candidate("w-d", "beta", "f2", &["dc-2"]),
        ];
        let picked = select(
            &candidates,
            3,
            2_000,
            &diversity,
            &weights,
            BlitzResourceMode::Normal,
        );
        assert_eq!(picked.len(), 3);
        assert_eq!(picked.iter().filter(|c| c.model_family == "f1").count(), 2);
        assert!(picked.iter().any(|c| c.provider == "beta"));
    }

    #[test]
    fn resource_mode_bias_reorders_equal_scores() {
        let weights = weights();
        let diversity = diversity();
        let mut local = candidate("w-a", "alpha", "f1", &["dc-1"]);
        local.local = true;
        let remote = candidate("w-z", "beta", "f2", &["dc-2"]);
        let candidates = vec![local, remote];
        let pick = |mode: BlitzResourceMode| {
            select(&candidates, 1, 2_000, &diversity, &weights, mode)
                .remove(0)
                .worker_id
                .as_str()
                .to_string()
        };
        // POWER_CRITICAL prefers remote; NETWORK_CRITICAL prefers local;
        // with no preference the worker-id tie-break decides.
        assert_eq!(pick(BlitzResourceMode::PowerCritical), "w-z");
        assert_eq!(pick(BlitzResourceMode::NetworkCritical), "w-a");
        assert_eq!(pick(BlitzResourceMode::Normal), "w-a");
    }

    #[test]
    fn marginal_independence_rewards_the_first_worker_only_when_alone() {
        assert_eq!(
            marginal_independence_milli(&["dc-1".to_string()], &[]),
            1_000
        );
        let identical = vec!["dc-1".to_string()];
        assert_eq!(
            marginal_independence_milli(&identical, std::slice::from_ref(&identical)),
            0
        );
        let partial = vec!["dc-1".to_string(), "dc-2".to_string()];
        let selected = vec![vec!["dc-1".to_string()]];
        // shared dc-1 over union {dc-1, dc-2} → overlap 500.
        assert_eq!(marginal_independence_milli(&partial, &selected), 500);
    }

    #[test]
    fn overlapping_failure_domains_shrink_effective_parallelism() {
        let independent = vec![
            slot("w-a", &["dc-1"]),
            slot("w-b", &["dc-2"]),
            slot("w-c", &["dc-3"]),
            slot("w-d", &["dc-4"]),
        ];
        assert_eq!(effective_parallelism_milli(&independent), 4_000);
        let correlated = vec![
            slot("w-a", &["dc-1", "pool-9"]),
            slot("w-b", &["dc-1"]),
            slot("w-c", &["dc-3"]),
            slot("w-d", &["dc-4"]),
        ];
        // w-b shares dc-1 over a two-domain union with w-a → 500 marginal,
        // so the formation fields 3_500 milli, not the raw 4_000.
        let effective = effective_parallelism_milli(&correlated);
        assert!(effective < 4_000);
        assert_eq!(effective, 3_500);
    }
}
