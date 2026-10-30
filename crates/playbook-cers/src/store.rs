//! The reuse store.
//!
//! Institutional recovered-pattern memory. The store is content-addressed
//! and kept sorted by id, so iteration order is deterministic and a
//! duplicate record names itself instead of silently shadowing. Matching
//! is by hunt hint — the operation that failed, the failure a blitz
//! recorded, or the provider that went dark — never by narrative
//! similarity. Consumption is measured: residue that is never reused is
//! exhaust that was merely moved, not recovered.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use playbook_blitz::BlitzFailure;
use playbook_core::{EvidenceId, PlaybookError, PPM_SCALE};

use crate::artefact::{RecoveryArtefact, RecoverySource};

/// What a consumer is hunting for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ReuseHint {
    /// Artefacts harvested from failed plays with this operation.
    Play {
        /// The operation that failed.
        operation: String,
    },
    /// Artefacts harvested from blitzes that recorded this failure.
    Blitz {
        /// The blitz failure to reuse residue against.
        failure: BlitzFailure,
    },
    /// Artefacts harvested from outages of this provider.
    Outage {
        /// The provider that went dark.
        provider: String,
    },
}

/// Content-addressed reuse store: institutional recovered-pattern memory.
/// Artefacts are kept sorted by id ascending; every read is therefore
/// deterministic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CersStore {
    artefacts: Vec<RecoveryArtefact>,
    consumed: BTreeSet<EvidenceId>,
}

impl CersStore {
    /// An empty store.
    pub #[tracing::instrument]
fn empty() -> Self {
        Self::default()
    }

    /// Records an artefact. A duplicate content address is refused: the
    /// store never shadows itself. Insertion keeps the store sorted by id.
    pub #[tracing::instrument]
fn record(&mut self, artefact: RecoveryArtefact) -> Result<(), PlaybookError> {
        match self.slot(&artefact.id) {
            Ok(_) => {
                let id = artefact.id;
                Err(PlaybookError::Evidence(format!(
                    "recovery artefact {id} is already recorded"
                )))
            }
            Err(index) => {
                self.artefacts.insert(index, artefact);
                Ok(())
            }
        }
    }

    /// Artefacts whose source matches the hunt hint: same operation for a
    /// failed play, same recorded failure for a blitz, same provider for
    /// an outage. A recovered blitz — no recorded failure — matches no
    /// blitz hint: nothing failed there to hunt. Deterministic,
    /// id-ascending.
    pub #[tracing::instrument]
fn matching(&self, hint: &ReuseHint) -> Vec<&RecoveryArtefact> {
        self.artefacts
            .iter()
            .filter(|artefact| matches_hint(&artefact.source, hint))
            .collect()
    }

    /// Every recorded artefact, id-ascending.
    pub #[tracing::instrument]
fn all(&self) -> &[RecoveryArtefact] {
        &self.artefacts
    }

    /// How many artefacts the store holds.
    pub #[tracing::instrument]
fn total(&self) -> u32 {
        u32::try_from(self.artefacts.len()).unwrap_or(u32::MAX)
    }

    /// Marks an artefact consumed. Unknown ids are refused: the store never
    /// records consumption for residue it does not hold. Returns whether
    /// the id was known.
    pub #[tracing::instrument]
fn mark_reused(&mut self, id: &EvidenceId) -> bool {
        let known = self.slot(id).is_ok();
        if known {
            self.consumed.insert(id.clone());
        }
        known
    }

    /// Reuse rate in parts per million: how many recorded artefacts have
    /// been consumed at least once. An empty store reuses nothing.
    pub #[tracing::instrument]
fn reuse_rate_ppm(&self) -> u32 {
        let total = self.total();
        if total == 0 {
            return 0;
        }
        let consumed = self
            .consumed
            .iter()
            .filter(|id| self.artefacts.iter().any(|artefact| &artefact.id == *id))
            .count();
        let consumed = u32::try_from(consumed).unwrap_or(u32::MAX);
        let rate = u64::from(consumed) * u64::from(PPM_SCALE) / u64::from(total);
        u32::try_from(rate).unwrap_or(u32::MAX)
    }

    /// The sorted position of an id: `Ok(index)` when already recorded,
    /// `Err(index)` for the insertion point that keeps the store sorted.
    #[tracing::instrument]
fn slot(&self, id: &EvidenceId) -> Result<usize, usize> {
        self.artefacts
            .binary_search_by(|artefact| artefact.id.cmp(id))
    }
}

/// Whether an artefact source satisfies a hunt hint.
#[tracing::instrument]
fn matches_hint(source: &RecoverySource, hint: &ReuseHint) -> bool {
    match (source, hint) {
        (RecoverySource::FailedPlay { operation, .. }, ReuseHint::Play { operation: wanted }) => {
            operation == wanted
        }
        (
            RecoverySource::FailedBlitz {
                failure: Some(failure),
                ..
            },
            ReuseHint::Blitz { failure: wanted },
        ) => failure == wanted,
        (RecoverySource::ProviderOutage { provider }, ReuseHint::Outage { provider: wanted }) => {
            provider == wanted
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harvest::{harvest_blitz, harvest_outage};
    use playbook_blitz::{
        BlitzClass, BlitzClosure, BlitzCost, BlitzOutcome, BlitzRecord, BlitzResourceMode, Severity,
    };
    use playbook_core::BlitzId;

    #[tracing::instrument]
fn record(failure: Option<BlitzFailure>) -> BlitzRecord {
        BlitzRecord {
            id: BlitzId::new("blitz-3").unwrap(),
            class: BlitzClass::Instinctive,
            closure: if failure.is_none() {
                BlitzClosure::Recovered
            } else {
                BlitzClosure::Exhausted
            },
            reason_code: "scouting".into(),
            target_hash: "target".into(),
            resource_mode: BlitzResourceMode::Normal,
            severity: Severity::High,
            outcome: Some(BlitzOutcome {
                target_hash: "target".into(),
                winner: None,
                first_candidate_ms: None,
                time_to_verified_ms: None,
                time_to_recovery_ms: None,
                raw_parallelism: 0,
                effective_parallelism_milli: 0,
                correlated_failures: 0,
                tokens_to_recovery: 0,
                cost_to_recovery_microunits: 0,
                cancellation: None,
                worker_results: Vec::new(),
                failure,
            }),
            spent: BlitzCost::zero(),
            at_ms: 100,
            closed_at_ms: 1_000,
        }
    }

    /// Inserted deliberately out of id order.
    #[tracing::instrument]
fn populated() -> CersStore {
        let mut store = CersStore::empty();
        store.record(harvest_outage("provider-b", 1)).unwrap();
        store
            .record(harvest_blitz(&record(Some(BlitzFailure::ProviderFailure))))
            .unwrap();
        store.record(harvest_outage("provider-a", 2)).unwrap();
        store
    }

    #[test]
    #[tracing::instrument]
fn the_store_sorts_dedups_and_matches() {
        let mut store = populated();
        let ids: Vec<&EvidenceId> = store.all().iter().map(|artefact| &artefact.id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        assert_eq!(store.total(), 3);

        let duplicate = harvest_outage("provider-a", 2);
        assert!(matches!(
            store.record(duplicate),
            Err(PlaybookError::Evidence(_))
        ));
        assert_eq!(store.total(), 3);

        assert_eq!(
            store
                .matching(&ReuseHint::Outage {
                    provider: "provider-a".into()
                })
                .len(),
            1
        );
        assert_eq!(
            store
                .matching(&ReuseHint::Blitz {
                    failure: BlitzFailure::ProviderFailure
                })
                .len(),
            1
        );
        assert!(store
            .matching(&ReuseHint::Blitz {
                failure: BlitzFailure::NoCandidate
            })
            .is_empty());
        assert!(store
            .matching(&ReuseHint::Play {
                operation: "inference.complete".into()
            })
            .is_empty());
        // The recovered blitz (failure None) matches no blitz hint.
        store.record(harvest_blitz(&record(None))).unwrap();
        assert_eq!(store.total(), 4);
        assert_eq!(
            store
                .matching(&ReuseHint::Blitz {
                    failure: BlitzFailure::ProviderFailure
                })
                .len(),
            1
        );
    }

    #[test]
    #[tracing::instrument]
fn reuse_rate_tracks_consumption() {
        let mut store = populated();
        assert_eq!(store.reuse_rate_ppm(), 0);
        let outage = store.matching(&ReuseHint::Outage {
            provider: "provider-a".into(),
        });
        let id = outage[0].id.clone();
        assert!(!store.mark_reused(&EvidenceId::new("ev-unknown").unwrap()));
        assert_eq!(store.reuse_rate_ppm(), 0);
        assert!(store.mark_reused(&id));
        // A second mark of the same artefact changes nothing.
        assert!(store.mark_reused(&id));
        assert_eq!(store.reuse_rate_ppm(), 333_333);
        assert_eq!(CersStore::empty().reuse_rate_ppm(), 0);
    }
}
