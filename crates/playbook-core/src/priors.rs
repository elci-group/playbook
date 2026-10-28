//! The historical prior table.
//!
//! Phase 8 learns it from measured outcomes. Phase 9 makes it historical:
//! the table is a core noun so an event log can carry it across drives, and
//! a versioned bundle can persist it between runs. The table holds no model
//! identity, no self-assessed confidence, and no competitive rankings.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::PlaybookError;
use crate::ids::PersonnelId;
use crate::units::{accept_schema, SCHEMA_VERSION};

/// Measured history for one personnel id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorRow {
    pub samples: u32,
    pub realized_reduction_sum_milli: u64,
}

/// Phase 8. The table learns only from measured outcomes: a play's realized
/// reduction is the evidence-backed yards-to-go drop recorded while it ran.
/// Self-reported `own_ytg_milli` never enters the table. The table stays
/// empty until deterministic ground truth exists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalPriorTable {
    rows: BTreeMap<String, PriorRow>,
}

impl HistoricalPriorTable {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Total recorded outcomes across all personnel.
    pub fn samples(&self) -> u32 {
        self.rows.values().map(|row| row.samples).sum()
    }

    /// Record one measured outcome. Failed plays record zero: a failure is
    /// ground truth about the play, not a gap in the table.
    pub fn record(&mut self, personnel: &PersonnelId, realized_reduction_milli: u32) {
        let row = self.rows.entry(personnel.to_string()).or_default();
        row.samples = row.samples.saturating_add(1);
        row.realized_reduction_sum_milli += u64::from(realized_reduction_milli);
    }

    pub fn row(&self, personnel: &PersonnelId) -> Option<&PriorRow> {
        self.rows.get(personnel.as_str())
    }

    /// Deterministic average realized reduction for a personnel id. `None`
    /// until the row exists and holds at least `min_samples` outcomes.
    pub fn prior_milli(&self, personnel: &PersonnelId, min_samples: u32) -> Option<u32> {
        let row = self.row(personnel)?;
        if row.samples < min_samples.max(1) {
            return None;
        }
        Some((row.realized_reduction_sum_milli / u64::from(row.samples)) as u32)
    }

    pub fn rows(&self) -> impl Iterator<Item = (&String, &PriorRow)> {
        self.rows.iter()
    }
}

/// A versioned, persistable prior table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriorBundle {
    pub schema_version: u32,
    pub priors: HistoricalPriorTable,
}

/// Serialize a prior table as a schema-versioned bundle.
pub fn export_priors(priors: &HistoricalPriorTable) -> Result<String, PlaybookError> {
    let bundle = PriorBundle {
        schema_version: SCHEMA_VERSION,
        priors: priors.clone(),
    };
    serde_json::to_string_pretty(&bundle).map_err(PlaybookError::json)
}

/// Parse and schema-check a persisted prior bundle.
pub fn import_priors(json: &str) -> Result<HistoricalPriorTable, PlaybookError> {
    let bundle: PriorBundle = serde_json::from_str(json).map_err(PlaybookError::json)?;
    accept_schema(bundle.schema_version)?;
    Ok(bundle.priors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prior_bundle_round_trips_through_json() {
        let mut priors = HistoricalPriorTable::empty();
        let worker = PersonnelId::new("worker").unwrap();
        priors.record(&worker, 4_000);
        priors.record(&worker, 2_000);
        let json = export_priors(&priors).unwrap();
        let loaded = import_priors(&json).unwrap();
        assert_eq!(loaded, priors);
        assert_eq!(loaded.prior_milli(&worker, 2), Some(3_000));
    }

    #[test]
    fn an_unknown_bundle_schema_is_rejected() {
        let json = r#"{"schema_version": 99, "priors": {"rows": {}}}"#;
        assert!(import_priors(json).is_err());
    }

    #[test]
    fn a_bundle_with_unknown_fields_is_rejected() {
        let json = r#"{
            "schema_version": 1,
            "priors": {"rows": {}, "rankings": {"worker": 1}}
        }"#;
        assert!(import_priors(json).is_err());
    }
}
