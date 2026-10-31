//! Blitz budget accounting.
//!
//! The envelope is policy property, never worker property. Every dimension
//! is a ceiling; the governor refuses any consumption delta that would pass
//! one, and no worker may expand its own share of the envelope. Once the
//! race is exhausted the only admissible moves are the defensive exits —
//! exhaustion, escalation, or failure — never an extension.

use playbook_core::{BlitzBudget, PlaybookError};

use crate::domain::BlitzCost;

/// Add one consumption delta to the running spend. Any dimension that would
/// pass its ceiling is refused; the caller must stop the race instead.
/// Workers and waves saturate at their ceilings the way huddle rounds do:
/// counting past the cap is a no-op, not an expansion.
pub fn consume(
    spent: BlitzCost,
    budget: &BlitzBudget,
    delta: &BlitzCost,
) -> Result<BlitzCost, PlaybookError> {
    let tokens = spent.tokens.saturating_add(delta.tokens);
    let cost_microunits = spent.cost_microunits.saturating_add(delta.cost_microunits);
    let duration_ms = spent.duration_ms.saturating_add(delta.duration_ms);
    let workers = spent
        .workers
        .saturating_add(delta.workers)
        .min(budget.max_workers);
    let waves = spent
        .waves
        .saturating_add(delta.waves)
        .min(budget.max_waves);
    let next = BlitzCost {
        tokens,
        cost_microunits,
        duration_ms,
        workers,
        waves,
    };
    if next.tokens > budget.max_tokens
        || next.cost_microunits > budget.max_cost_microunits
        || next.duration_ms > budget.max_duration_ms
        || next.workers > budget.max_workers
        || next.waves > budget.max_waves
    {
        return Err(PlaybookError::BudgetExhausted);
    }
    Ok(next)
}

/// True once any budget dimension has reached its ceiling. After this the
/// race buys nothing further; only the defensive exits remain.
pub fn exhausted(spent: &BlitzCost, budget: &BlitzBudget) -> bool {
    spent.tokens >= budget.max_tokens
        || spent.cost_microunits >= budget.max_cost_microunits
        || spent.duration_ms >= budget.max_duration_ms
        || spent.workers >= budget.max_workers
        || spent.waves >= budget.max_waves
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> BlitzBudget {
        BlitzBudget {
            max_tokens: 4_000,
            max_cost_microunits: 400_000,
            max_duration_ms: 10_000,
            max_workers: 8,
            max_waves: 3,
        }
    }

    #[test]
    fn consumption_stops_at_the_envelope() {
        let budget = envelope();
        let first = consume(
            BlitzCost::zero(),
            &budget,
            &BlitzCost {
                tokens: 3_000,
                cost_microunits: 300_000,
                duration_ms: 8_000,
                workers: 4,
                waves: 1,
            },
        )
        .unwrap();
        assert_eq!(first.tokens, 3_000);
        assert!(!exhausted(&first, &budget));
        let error = consume(
            first,
            &budget,
            &BlitzCost {
                tokens: 2_000,
                cost_microunits: 0,
                duration_ms: 0,
                workers: 0,
                waves: 0,
            },
        )
        .unwrap_err();
        assert_eq!(error, PlaybookError::BudgetExhausted);
        let exact = consume(
            first,
            &budget,
            &BlitzCost {
                tokens: 1_000,
                cost_microunits: 100_000,
                duration_ms: 2_000,
                workers: 4,
                waves: 2,
            },
        )
        .unwrap();
        assert_eq!(exact.tokens, 4_000);
        assert_eq!(exact.workers, 8);
        assert_eq!(exact.waves, 3);
        assert!(exhausted(&exact, &budget));
    }

    #[test]
    fn workers_and_waves_never_pass_the_ceiling() {
        let budget = envelope();
        let spent = consume(
            BlitzCost::zero(),
            &budget,
            &BlitzCost {
                tokens: 0,
                cost_microunits: 0,
                duration_ms: 0,
                workers: 99,
                waves: 99,
            },
        )
        .unwrap();
        assert_eq!(spent.workers, 8);
        assert_eq!(spent.waves, 3);
        assert!(exhausted(&spent, &budget));
    }
}
