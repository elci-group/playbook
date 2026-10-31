//! Causal penalty accounting.
//!
//! A penalty is the field geometry becoming less or more favourable — never
//! a mere failed play. A failed implementation attempt is an execution
//! event; a penalty means the *known field geometry itself* changed: newly
//! evidenced obligations hardened it (offensive penalty), or newly evidenced
//! simplification eased it (defensive penalty).
//!
//! Every adjustment carries provenance sufficient to prevent gaming: cause,
//! evidence, foreseeability class, and the affected gates. The four-way
//! foreseeability split keeps the attribution honest — exogenous change is
//! nobody's fault, discovery is new evidence, estimation error belongs to
//! the Opposition Kicker's decomposition quality, and execution-created
//! scope is the offence's own doing. Personnel priors distinguish earned
//! execution yards from topology movement: nobody receives execution credit
//! because the field became easier, and nobody is penalised because a user
//! expanded scope.

use serde::{Deserialize, Serialize};

use crate::domain::{field_refusal, FieldAdjustment, Foreseeability, MovementAttribution};

/// The append-only log of field adjustments. Every entry is validated at
/// declaration; duplicate ids are refused so replay cannot double-count a
/// movement.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustmentLog {
    records: Vec<FieldAdjustment>,
}

impl AdjustmentLog {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Declare one adjustment: validated, then appended. Duplicate
    /// adjustment ids are refused ("duplicate_adjustment_id").
    pub fn declare(
        &mut self,
        adjustment: FieldAdjustment,
    ) -> Result<(), playbook_core::PlaybookError> {
        adjustment.validate()?;
        if self
            .records
            .iter()
            .any(|record| record.adjustment_id == adjustment.adjustment_id)
        {
            return Err(field_refusal("duplicate_adjustment_id"));
        }
        self.records.push(adjustment);
        Ok(())
    }

    pub fn records(&self) -> &[FieldAdjustment] {
        &self.records
    }

    /// All records of one foreseeability class.
    pub fn by_foreseeability(&self, foreseeability: Foreseeability) -> Vec<&FieldAdjustment> {
        self.records
            .iter()
            .filter(|record| record.foreseeability == foreseeability)
            .collect()
    }

    /// Net signed movement in yards: harder adjustments add, easier
    /// adjustments subtract.
    pub fn net_yards(&self) -> i64 {
        self.records
            .iter()
            .map(|record| match record.direction {
                crate::domain::AdjustmentDirection::Harder => i64::from(record.magnitude_yards),
                crate::domain::AdjustmentDirection::Easier => -i64::from(record.magnitude_yards),
            })
            .sum()
    }
}

/// The per-direction and per-foreseeability totals over one log. These feed
/// the Kicker accuracy metric (estimation error) and the §15 attribution
/// split (execution versus topology movement).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PenaltyTotals {
    pub offensive_penalty_count: u32,
    pub defensive_penalty_count: u32,
    pub offensive_penalty_yards: u64,
    pub defensive_penalty_yards: u64,
    pub estimation_error_yards: u64,
    pub execution_created_yards: u64,
    pub exogenous_yards: u64,
    pub discovery_yards: u64,
}

/// The §11 decomposition over one log. Deterministic.
pub fn totals(log: &AdjustmentLog) -> PenaltyTotals {
    let mut t = PenaltyTotals::default();
    for record in log.records() {
        let yards = u64::from(record.magnitude_yards);
        match record.direction {
            crate::domain::AdjustmentDirection::Harder => {
                t.offensive_penalty_count += 1;
                t.offensive_penalty_yards += yards;
            }
            crate::domain::AdjustmentDirection::Easier => {
                t.defensive_penalty_count += 1;
                t.defensive_penalty_yards += yards;
            }
        }
        match record.foreseeability {
            Foreseeability::EstimationError => t.estimation_error_yards += yards,
            Foreseeability::ExecutionCreated => t.execution_created_yards += yards,
            Foreseeability::Exogenous => t.exogenous_yards += yards,
            Foreseeability::Discovery => t.discovery_yards += yards,
        }
    }
    t
}

/// §15 attribution: classify one adjustment into the movement class the
/// personnel priors use. Harder + exogenous is the external environment;
/// harder + discovery or estimation error is an offensive penalty; harder +
/// execution created is a self-created regression; easier in any form is a
/// defensive penalty (the offence cleaning up its own regression is credited
/// as cleanup, not as earned yards).
pub fn attribution(adjustment: &FieldAdjustment) -> MovementAttribution {
    use crate::domain::AdjustmentDirection as Direction;
    use Foreseeability as Why;
    match (adjustment.direction, adjustment.foreseeability) {
        (Direction::Easier, _) => MovementAttribution::DefensivePenalty,
        (Direction::Harder, Why::Exogenous) => MovementAttribution::ExternalEnvironmentAdjustment,
        (Direction::Harder, Why::Discovery) | (Direction::Harder, Why::EstimationError) => {
            MovementAttribution::OffensivePenalty
        }
        (Direction::Harder, Why::ExecutionCreated) => MovementAttribution::ExecutionRegression,
    }
}

/// §15 rule: execution credit requires ExecutionProgress attribution.
/// Personnel SHALL NOT receive execution credit because the field became
/// easier, and SHALL NOT be penalised because a user expanded scope.
pub fn earns_execution_credit(attribution: MovementAttribution) -> bool {
    attribution == MovementAttribution::ExecutionProgress
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::AdjustmentDirection as Direction;
    use playbook_core::{EvidenceId, GateId};

    fn adjustment(
        id: &str,
        direction: Direction,
        yards: u32,
        foreseeability: Foreseeability,
    ) -> FieldAdjustment {
        FieldAdjustment {
            adjustment_id: id.into(),
            direction,
            magnitude_yards: yards,
            cause: format!("{direction:?} {foreseeability:?}"),
            evidence_ref: Some(EvidenceId::new("ev-1").unwrap()),
            foreseeability,
            affected_gates: vec![GateId::new("build").unwrap()],
            at_ms: 0,
        }
    }

    #[test]
    fn declare_appends_and_refuses_duplicates_and_invalid_records() {
        let mut log = AdjustmentLog::empty();
        log.declare(adjustment(
            "a-1",
            Direction::Harder,
            6,
            Foreseeability::Discovery,
        ))
        .unwrap();
        assert_eq!(log.records().len(), 1);
        // Duplicate id refused.
        assert_eq!(
            log.declare(adjustment(
                "a-1",
                Direction::Easier,
                3,
                Foreseeability::Discovery
            ))
            .unwrap_err(),
            field_refusal("duplicate_adjustment_id")
        );
        // Unvalidated declaration refused at the gate.
        let mut unattributed = adjustment("a-2", Direction::Harder, 2, Foreseeability::Discovery);
        unattributed.evidence_ref = None;
        assert_eq!(
            log.declare(unattributed).unwrap_err(),
            field_refusal(
                "adjustment requires establishing evidence — topology movement without evidence is unverifiable"
            )
        );
        assert_eq!(log.records().len(), 1);
    }

    #[test]
    fn net_yards_is_the_signed_sum() {
        let mut log = AdjustmentLog::empty();
        log.declare(adjustment(
            "a-1",
            Direction::Harder,
            6,
            Foreseeability::Discovery,
        ))
        .unwrap();
        log.declare(adjustment(
            "a-2",
            Direction::Easier,
            4,
            Foreseeability::Discovery,
        ))
        .unwrap();
        assert_eq!(log.net_yards(), 2);
    }

    #[test]
    fn by_foreseeability_filters_exactly() {
        let mut log = AdjustmentLog::empty();
        log.declare(adjustment(
            "a-1",
            Direction::Harder,
            3,
            Foreseeability::Exogenous,
        ))
        .unwrap();
        log.declare(adjustment(
            "a-2",
            Direction::Harder,
            4,
            Foreseeability::EstimationError,
        ))
        .unwrap();
        let exogenous = log.by_foreseeability(Foreseeability::Exogenous);
        assert_eq!(exogenous.len(), 1);
        assert_eq!(exogenous[0].adjustment_id, "a-1");
    }

    #[test]
    fn totals_split_by_direction_and_foreseeability() {
        let mut log = AdjustmentLog::empty();
        log.declare(adjustment(
            "a-1",
            Direction::Harder,
            6,
            Foreseeability::EstimationError,
        ))
        .unwrap();
        log.declare(adjustment(
            "a-2",
            Direction::Harder,
            4,
            Foreseeability::ExecutionCreated,
        ))
        .unwrap();
        log.declare(adjustment(
            "a-3",
            Direction::Easier,
            5,
            Foreseeability::Discovery,
        ))
        .unwrap();
        log.declare(adjustment(
            "a-4",
            Direction::Easier,
            2,
            Foreseeability::Exogenous,
        ))
        .unwrap();
        let t = totals(&log);
        assert_eq!(t.offensive_penalty_count, 2);
        assert_eq!(t.offensive_penalty_yards, 10);
        assert_eq!(t.defensive_penalty_count, 2);
        assert_eq!(t.defensive_penalty_yards, 7);
        assert_eq!(t.estimation_error_yards, 6);
        assert_eq!(t.execution_created_yards, 4);
        assert_eq!(t.exogenous_yards, 2);
        assert_eq!(t.discovery_yards, 5);
    }

    #[test]
    fn the_attribution_matrix_covers_every_direction_foreseeability_pair() {
        use Foreseeability as Why;
        for direction in [Direction::Harder, Direction::Easier] {
            for why in [
                Why::Exogenous,
                Why::Discovery,
                Why::EstimationError,
                Why::ExecutionCreated,
            ] {
                let adjustment = adjustment("a-x", direction, 1, why);
                let class = attribution(&adjustment);
                // The earned-yard class is never produced by a penalty.
                assert_ne!(
                    class,
                    MovementAttribution::ExecutionProgress,
                    "penalties never earn execution credit"
                );
            }
        }
        // Spot pins from the doctrine.
        let harder_exogenous = adjustment("x", Direction::Harder, 1, Why::Exogenous);
        assert_eq!(
            attribution(&harder_exogenous),
            MovementAttribution::ExternalEnvironmentAdjustment
        );
        let harder_created = adjustment("y", Direction::Harder, 1, Why::ExecutionCreated);
        assert_eq!(
            attribution(&harder_created),
            MovementAttribution::ExecutionRegression
        );
    }

    #[test]
    fn only_execution_progress_earns_credit() {
        use MovementAttribution as Class;
        assert!(earns_execution_credit(Class::ExecutionProgress));
        for class in [
            Class::ExecutionRegression,
            Class::OffensivePenalty,
            Class::DefensivePenalty,
            Class::ValidationAdjustment,
            Class::ExternalEnvironmentAdjustment,
        ] {
            assert!(!earns_execution_credit(class), "{class:?} is not earned");
        }
    }
}
