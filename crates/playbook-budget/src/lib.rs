//! Multidimensional drive budget.
//!
//! Raw dimensions stay available. The effective fraction is the scarcest
//! weighted dimension, so a healthy token balance cannot hide an empty quota.

#![forbid(unsafe_code)]

use playbook_core::{BudgetWeights, DriveBudget, PlaybookError, PPM_SCALE};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetView {
    pub remaining_absolute: DriveBudget,
    pub remaining_fraction_ppm: u32,
    pub dimension_fractions_ppm: Vec<(&'static str, u32)>,
    pub burn_rate: DriveBudget,
    pub projected_completion_cost: Option<DriveBudget>,
    pub exhaustion_risk_ppm: u32,
}

pub #[tracing::instrument]
fn effective_remaining_ppm(
    initial: DriveBudget,
    consumed: DriveBudget,
    weights: BudgetWeights,
) -> u32 {
    let remaining = initial.remaining(consumed);
    let mut scarce = None;
    for (weight, initial_dim, remaining_dim) in weights.pairs(initial, remaining) {
        if weight == 0 || initial_dim == 0 {
            continue;
        }
        let fraction =
            u32::try_from(remaining_dim.saturating_mul(u64::from(PPM_SCALE)) / initial_dim)
                .unwrap_or(PPM_SCALE)
                .min(PPM_SCALE);
        scarce = Some(scarce.map_or(fraction, |current: u32| current.min(fraction)));
    }
    scarce.unwrap_or(PPM_SCALE)
}

pub #[tracing::instrument]
fn consume(
    initial: DriveBudget,
    consumed: DriveBudget,
    cost: DriveBudget,
) -> Result<DriveBudget, PlaybookError> {
    let next = consumed.checked_add(cost)?;
    if !next.fits_within(initial) {
        return Err(PlaybookError::BudgetExhausted);
    }
    if remaining_increased(initial, consumed, next) {
        return Err(PlaybookError::BudgetIncrease);
    }
    Ok(next)
}

pub #[tracing::instrument]
fn view(
    initial: DriveBudget,
    consumed: DriveBudget,
    weights: BudgetWeights,
    burn_rate: DriveBudget,
    ytg_reduced_milli: u32,
    ytg_left_milli: u32,
) -> BudgetView {
    let remaining_absolute = initial.remaining(consumed);
    let remaining_fraction_ppm = effective_remaining_ppm(initial, consumed, weights);
    let names = [
        "tokens",
        "monetary_cost_micros",
        "wall_clock_ms",
        "tool_calls",
        "inference_calls",
        "retry_count",
        "context_capacity",
        "provider_quota",
    ];
    let dimension_fractions_ppm = weights
        .pairs(initial, remaining_absolute)
        .into_iter()
        .zip(names)
        .filter_map(|((weight, initial_dim, remaining_dim), name)| {
            if weight == 0 || initial_dim == 0 {
                return None;
            }
            let fraction =
                u32::try_from(remaining_dim.saturating_mul(u64::from(PPM_SCALE)) / initial_dim)
                    .unwrap_or(PPM_SCALE)
                    .min(PPM_SCALE);
            Some((name, fraction))
        })
        .collect();
    let projected_completion_cost = if ytg_reduced_milli == 0 {
        None
    } else {
        Some(scale_budget(consumed, ytg_left_milli, ytg_reduced_milli))
    };
    let exhaustion_risk_ppm = exhaustion(
        remaining_fraction_ppm,
        projected_completion_cost,
        remaining_absolute,
        weights,
        ytg_left_milli,
    );
    BudgetView {
        remaining_absolute,
        remaining_fraction_ppm,
        dimension_fractions_ppm,
        burn_rate,
        projected_completion_cost,
        exhaustion_risk_ppm,
    }
}

#[tracing::instrument]
fn exhaustion(
    remaining_ppm: u32,
    projected: Option<DriveBudget>,
    remaining: DriveBudget,
    weights: BudgetWeights,
    ytg_left: u32,
) -> u32 {
    if ytg_left == 0 {
        return 0;
    }
    if remaining_ppm == 0 {
        return PPM_SCALE;
    }
    if projected.is_some_and(|projected| exceeds(projected, remaining, weights)) {
        return PPM_SCALE;
    }
    PPM_SCALE.saturating_sub(remaining_ppm)
}

#[tracing::instrument]
fn exceeds(projected: DriveBudget, remaining: DriveBudget, weights: BudgetWeights) -> bool {
    weights
        .pairs(projected, remaining)
        .into_iter()
        .any(|(weight, projected_dim, remaining_dim)| weight > 0 && projected_dim > remaining_dim)
}

#[tracing::instrument]
fn scale_budget(budget: DriveBudget, numer: u32, denom: u32) -> DriveBudget {
    let scale = |value: u64| value.saturating_mul(u64::from(numer)) / u64::from(denom.max(1));
    DriveBudget {
        tokens: scale(budget.tokens),
        monetary_cost_micros: scale(budget.monetary_cost_micros),
        wall_clock_ms: scale(budget.wall_clock_ms),
        tool_calls: scale(budget.tool_calls),
        inference_calls: scale(budget.inference_calls),
        retry_count: scale(budget.retry_count),
        context_capacity: scale(budget.context_capacity),
        provider_quota: scale(budget.provider_quota),
    }
}

#[tracing::instrument]
fn remaining_increased(initial: DriveBudget, before: DriveBudget, after: DriveBudget) -> bool {
    let left = initial.remaining(before);
    let right = initial.remaining(after);
    right.tokens > left.tokens
        || right.monetary_cost_micros > left.monetary_cost_micros
        || right.wall_clock_ms > left.wall_clock_ms
        || right.tool_calls > left.tool_calls
        || right.inference_calls > left.inference_calls
        || right.retry_count > left.retry_count
        || right.context_capacity > left.context_capacity
        || right.provider_quota > left.provider_quota
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracing::instrument]
fn weights() -> BudgetWeights {
        BudgetWeights {
            tokens: 1,
            monetary_cost_micros: 0,
            wall_clock_ms: 0,
            tool_calls: 0,
            inference_calls: 0,
            retry_count: 0,
            context_capacity: 0,
            provider_quota: 1,
        }
    }

    #[tracing::instrument]
fn budget(tokens: u64, quota: u64) -> DriveBudget {
        DriveBudget {
            tokens,
            provider_quota: quota,
            ..DriveBudget::zero()
        }
    }

    #[test]
    #[tracing::instrument]
fn quota_pressure_beats_a_healthy_token_balance() {
        let initial = budget(1_000, 1_000);
        let consumed = budget(100, 900);
        assert_eq!(
            effective_remaining_ppm(initial, consumed, weights()),
            100_000
        );
    }

    #[test]
    #[tracing::instrument]
fn consume_cannot_increase_remaining() {
        let initial = budget(100, 100);
        let err = consume(initial, budget(10, 0), budget(u64::MAX, 0)).unwrap_err();
        assert_eq!(err, PlaybookError::BudgetExhausted);
    }

    #[test]
    #[tracing::instrument]
fn projected_overrun_is_full_exhaustion_risk() {
        let initial = budget(100, 100);
        let consumed = budget(80, 0);
        let seen = view(initial, consumed, weights(), budget(80, 0), 20, 80);
        assert_eq!(seen.exhaustion_risk_ppm, PPM_SCALE);
        assert!(seen.projected_completion_cost.unwrap().tokens > seen.remaining_absolute.tokens);
    }
}
