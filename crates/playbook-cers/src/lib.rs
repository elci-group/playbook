//! CERS: cognitive exhaust recovery.
//!
//! Inference is bought with budget. Materially failed inference burns that
//! budget twice unless the failure is harvested: once when the attempt
//! fails, and again when the next drive repeats it blind. This crate makes
//! the second burn optional. Every materially failed play, every closed
//! blitz, and every provider outage yields one structured recovery
//! artefact — measured facts, disproved assumptions, surviving partial
//! work, a reproduction path, ruled-out hypotheses — and never a claim of
//! success. A win is harvested too: what worked is residue.
//!
//! The rules this crate owns:
//!
//! * residue over narrative — every entry in an artefact comes from
//!   measured diagnostics, evidence, or rejected candidates; nothing is
//!   generated prose and nothing pretends the task succeeded;
//! * content addressing — an artefact's id is the digest of its own
//!   canonical bytes and its context digest covers the harvested content;
//!   tampering is detected, never trusted;
//! * scoping at consumption — a packet hands later personnel the residue
//!   without the failed context, bounded per list, and refuses to carry
//!   competitive metadata;
//! * institutional memory — the store keeps recovered patterns sorted,
//!   content-addressed, and matched by hunt hint, and measures how often
//!   the institution actually reuses what it recovered.
//!
//! Harvesting is deterministic: the same failure record always produces
//! the same artefact, byte for byte, id included.

#![forbid(unsafe_code)]

pub mod artefact;
pub mod harvest;
pub mod packet;
pub mod store;

pub use artefact::{RecoveryArtefact, RecoverySource};
pub use harvest::{harvest_blitz, harvest_outage, harvest_play};
pub use packet::packet;
pub use store::{CersStore, ReuseHint};

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_core::{EvidenceId, FailureClass, PlaybookError, PPM_SCALE};

    #[test]
    fn recovered_residue_flows_from_harvest_to_store() {
        let outage = harvest_outage("provider-a", 5_000);
        assert_eq!(outage.failure_class, Some(FailureClass::EnvironmentFailure));
        assert_eq!(
            outage.source,
            RecoverySource::ProviderOutage {
                provider: "provider-a".into()
            }
        );
        assert_eq!(
            outage.confirmed_facts,
            vec!["outage recorded for provider provider-a".to_string()]
        );
        assert_eq!(
            outage.failed_assumptions,
            vec!["provider provider-a was assumed available".to_string()]
        );
        assert!(outage.verify().is_ok());

        let mut store = CersStore::empty();
        store.record(outage.clone()).unwrap();
        assert!(matches!(
            store.record(outage.clone()),
            Err(PlaybookError::Evidence(_))
        ));
        assert_eq!(store.total(), 1);

        assert_eq!(
            store
                .matching(&ReuseHint::Outage {
                    provider: "provider-a".into()
                })
                .len(),
            1
        );
        assert!(store
            .matching(&ReuseHint::Outage {
                provider: "provider-b".into()
            })
            .is_empty());

        let scoped = packet(&outage, 4).unwrap();
        assert_eq!(
            scoped["confirmed_facts"]
                .as_array()
                .map(|items| items.len()),
            Some(1)
        );

        assert!(store.mark_reused(&outage.id));
        assert!(!store.mark_reused(&EvidenceId::new("ev-unknown").unwrap()));
        assert_eq!(store.reuse_rate_ppm(), PPM_SCALE);
    }
}
