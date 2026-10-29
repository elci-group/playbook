//! Huddle budget accounting.
//!
//! Budget exhaustion terminates discussion and forces synthesis from the
//! accumulated reports. No huddle may extend itself.

use playbook_core::{HuddleBudget, PlaybookError};

use crate::domain::HuddleCost;

/// Add one consumption delta to the running spend. Any dimension that would
/// pass its ceiling is refused; the caller must synthesize instead.
pub fn consume(
    spent: HuddleCost,
    budget: &HuddleBudget,
    delta: &HuddleCost,
) -> Result<HuddleCost, PlaybookError> {
    let tokens = spent
        .tokens
        .checked_add(delta.tokens)
        .ok_or(PlaybookError::BudgetExhausted)?;
    let cost_microunits = spent
        .cost_microunits
        .checked_add(delta.cost_microunits)
        .ok_or(PlaybookError::BudgetExhausted)?;
    let duration_ms = spent
        .duration_ms
        .checked_add(delta.duration_ms)
        .ok_or(PlaybookError::BudgetExhausted)?;
    let rounds = spent
        .rounds
        .saturating_add(delta.rounds)
        .min(budget.max_rounds);
    let next = HuddleCost {
        tokens,
        cost_microunits,
        duration_ms,
        rounds,
    };
    if next.tokens > budget.max_tokens
        || next.cost_microunits > budget.max_cost_microunits
        || next.duration_ms > budget.max_duration_ms
        || next.rounds > budget.max_rounds
    {
        return Err(PlaybookError::BudgetExhausted);
    }
    Ok(next)
}

/// True once any budget dimension has reached its ceiling. After this only
/// forced synthesis is admissible.
pub fn exhausted(spent: &HuddleCost, budget: &HuddleBudget) -> bool {
    spent.tokens >= budget.max_tokens
        || spent.cost_microunits >= budget.max_cost_microunits
        || spent.duration_ms >= budget.max_duration_ms
        || spent.rounds >= budget.max_rounds
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> HuddleBudget {
        HuddleBudget {
            max_tokens: 2_000,
            max_cost_microunits: 200_000,
            max_duration_ms: 5_000,
            max_rounds: 2,
            max_participants: 3,
        }
    }

    #[test]
    fn consumption_stops_at_the_envelope() {
        let budget = envelope();
        let first = consume(
            HuddleCost::zero(),
            &budget,
            &HuddleCost {
                tokens: 1_500,
                cost_microunits: 150_000,
                duration_ms: 4_000,
                rounds: 1,
            },
        )
        .unwrap();
        assert_eq!(first.tokens, 1_500);
        assert!(!exhausted(&first, &budget));
        let error = consume(
            first,
            &budget,
            &HuddleCost {
                tokens: 1_000,
                cost_microunits: 0,
                duration_ms: 0,
                rounds: 0,
            },
        )
        .unwrap_err();
        assert_eq!(error, PlaybookError::BudgetExhausted);
        let exact = consume(
            first,
            &budget,
            &HuddleCost {
                tokens: 500,
                cost_microunits: 50_000,
                duration_ms: 1_000,
                rounds: 1,
            },
        )
        .unwrap();
        assert!(exhausted(&exact, &budget));
    }

    #[test]
    fn rounds_never_pass_the_ceiling() {
        let budget = envelope();
        let spent = consume(
            HuddleCost::zero(),
            &budget,
            &HuddleCost {
                tokens: 0,
                cost_microunits: 0,
                duration_ms: 0,
                rounds: 2,
            },
        )
        .unwrap();
        assert_eq!(spent.rounds, 2);
        assert!(exhausted(&spent, &budget));
    }
}
