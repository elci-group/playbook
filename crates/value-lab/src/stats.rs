//! The lab's statistics: honest descriptors and bootstrap confidence
//! intervals from a deterministic seeded resampler — no float PRNG
//! dependency, reproducible from the protocol hash.

use serde::{Deserialize, Serialize};

/// A confidence interval over a statistic, in the samples' units.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceInterval {
    pub low: i64,
    pub high: i64,
    pub level_ppm: u32,
    pub iterations: u32,
}

pub fn mean(samples: &[i64]) -> Option<i64> {
    if samples.is_empty() {
        return None;
    }
    let total: i128 = samples.iter().map(|s| i128::from(*s)).sum();
    Some((total / samples.len() as i128) as i64)
}

pub fn median(samples: &[i64]) -> Option<i64> {
    percentile(samples, 500_000)
}

/// The `per_ppm` percentile (500_000 = median).
pub fn percentile(samples: &[i64], per_ppm: u32) -> Option<i64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<i64> = samples.to_vec();
    sorted.sort();
    let rank = (u64::from(per_ppm) * u64::try_from(sorted.len()).unwrap_or(u64::MAX)) / 1_000_000;
    let index = (rank as usize).min(sorted.len() - 1);
    Some(sorted[index])
}

pub fn stddev(samples: &[i64]) -> Option<i64> {
    let average = mean(samples)?;
    if samples.len() < 2 {
        return Some(0);
    }
    let variance: i128 = samples
        .iter()
        .map(|s| {
            let delta = i128::from(*s) - i128::from(average);
            delta * delta
        })
        .sum::<i128>()
        / (samples.len() as i128 - 1);
    Some((variance as f64).sqrt() as i64)
}

/// Element-wise paired differences, aligned by index (the caller aligns
/// paired runs by assignment before calling).
pub fn paired_differences(a: &[i64], b: &[i64]) -> Vec<i64> {
    a.iter()
        .zip(b.iter())
        .map(|(left, right)| left - right)
        .collect()
}

/// A tiny deterministic LCG: the bootstrap must be reproducible from the
/// protocol seed, not from entropy.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 16
    }
}

/// A bootstrap confidence interval over `statistic(samples)`, `iterations`
/// resamples at the given confidence level. Deterministic for a given
/// seed (§31: conditions reproducible even when inference is not).
pub fn bootstrap_ci(
    samples: &[i64],
    statistic: fn(&[i64]) -> Option<i64>,
    iterations: u32,
    level_ppm: u32,
    seed: u64,
) -> Option<ConfidenceInterval> {
    if samples.is_empty() || iterations == 0 {
        return None;
    }
    let mut rng = Lcg(seed.max(1));
    let mut estimates: Vec<i64> = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let mut resample = Vec::with_capacity(samples.len());
        for _ in 0..samples.len() {
            let index = (rng.next() as usize) % samples.len();
            resample.push(samples[index]);
        }
        if let Some(estimate) = statistic(&resample) {
            estimates.push(estimate);
        }
    }
    estimates.sort();
    let low_rank = (u64::from(1_000_000 - level_ppm) / 2)
        * u64::try_from(estimates.len()).unwrap_or(0)
        / 1_000_000;
    let high_rank = ((1_000_000u64 + u64::from(level_ppm)) / 2).saturating_sub(1)
        * u64::try_from(estimates.len()).unwrap_or(0)
        / 1_000_000;
    Some(ConfidenceInterval {
        low: estimates[(low_rank as usize).min(estimates.len() - 1)],
        high: estimates[(high_rank as usize).min(estimates.len() - 1)],
        level_ppm,
        iterations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_read_the_samples() {
        assert_eq!(mean(&[10, 20, 30]), Some(20));
        assert_eq!(median(&[30, 10, 20]), Some(20));
        assert_eq!(percentile(&[10, 20, 30, 40], 900_000), Some(40));
        assert_eq!(mean(&[]), None);
        assert_eq!(median(&[5]), Some(5));
        assert!(stddev(&[10, 10, 10]) == Some(0));
        assert!(stddev(&[0, 20]) > Some(0));
    }

    #[test]
    fn paired_differences_align_by_assignment() {
        assert_eq!(paired_differences(&[100, 200], &[90, 210]), vec![10, -10]);
    }

    #[test]
    fn the_bootstrap_is_deterministic_and_sane() {
        let samples: Vec<i64> = (0..50).map(|s| s * 10).collect();
        let first = bootstrap_ci(&samples, median, 400, 950_000, 42).unwrap();
        let second = bootstrap_ci(&samples, median, 400, 950_000, 42).unwrap();
        assert_eq!(first, second, "the seed reproduces the interval");
        assert!(first.low <= first.high);
        // The median of 0..=490 is 240; the interval should bracket the
        // neighbourhood, not the extremes.
        assert!(first.low <= 250 && first.high >= 230, "{first:?}");
    }
}
