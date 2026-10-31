//! The §13→§14 bridge: decision snapshots derived from a real Playbook
//! journal. Folding the journal stepwise and snapshotting at each
//! personnel decision yields the frozen decision record the lab
//! reconstructs counterfactuals from — the subject's own journal becomes
//! the experimental instrument, read by the lab's rules.

use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_state::{apply, ApplyMode, DriveState};
use serde::Serialize;

use crate::frozen::{DecisionSnapshot, PersonnelOption};

/// The lab's utility proxy for an option at decision time: the measured
/// reliability ppm of the personnel — never a self-report, and exactly
/// what the frozen state carried.
fn option_of(personnel: &playbook_core::Personnel) -> PersonnelOption {
    PersonnelOption {
        name: personnel.id.as_str().to_string(),
        predicted_utility: i64::from(personnel.reliability_ppm),
        cost_micros: personnel.cost_tokens,
        latency_ms: personnel.latency_ms,
        reliability_ppm: personnel.reliability_ppm,
    }
}

fn digest_of(value: &impl Serialize) -> String {
    playbook_core::sha256_hex(serde_json::to_string(value).unwrap_or_default().as_bytes())
}

/// Derive the decision snapshots of a journaled drive. At every
/// personnel-selection moment the lab freezes: YTG, the gate digest, the
/// assessed options with their measured scores, the selection and the
/// named alternatives. Pure: the same journal always derives the same
/// snapshots.
pub fn decision_snapshots(
    events: &[DriveEvent],
    policy: &Policy,
) -> Result<Vec<DecisionSnapshot>, playbook_core::PlaybookError> {
    let mut state: Option<DriveState> = None;
    let mut snapshots = Vec::new();
    for event in events {
        state = Some(apply(state.as_ref(), event, policy, ApplyMode::Audit)?);
        let state = state.as_ref().expect("just applied");
        let decisive = matches!(event, DriveEvent::PersonnelChanged { .. });
        if !decisive {
            continue;
        }
        let mut options: Vec<PersonnelOption> = state.assessments().iter().map(option_of).collect();
        if let Some(active) = state.personnel() {
            let option = option_of(active);
            if !options.iter().any(|existing| existing.name == option.name) {
                options.push(option);
            }
        }
        if options.is_empty() {
            continue;
        }
        options.sort_by_key(|option| std::cmp::Reverse(option.predicted_utility));
        let selected = state
            .personnel()
            .map(|p| p.id.as_str().to_string())
            .unwrap_or_else(|| options[0].name.clone());
        let rejected: Vec<String> = options
            .iter()
            .filter(|option| option.name != selected)
            .map(|option| option.name.clone())
            .collect();
        snapshots.push(DecisionSnapshot {
            snap_hash: String::new(),
            at_ms: state.budget_consumed().wall_clock_ms,
            ytg_milli: state.ytg_milli(),
            gate_digest: digest_of(
                &state
                    .gates()
                    .iter()
                    .map(|gate| (gate.id.as_str(), gate.satisfaction_ppm))
                    .collect::<Vec<_>>(),
            ),
            options,
            selected,
            rejected,
        });
    }
    Ok(snapshots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_sim::run_mvp_with_journal;
    use std::path::Path;

    #[test]
    fn a_real_journal_derives_frozen_decisions() {
        let path = std::env::temp_dir().join("value-lab-derive.jsonl");
        let _ = std::fs::remove_file(&path);
        let report = run_mvp_with_journal(&path).unwrap();
        assert!(report.replay_match);
        let events = report.events;
        let snapshots = decision_snapshots(&events, &Policy::builtin()).unwrap();
        // The MVP drive changes personnel at least once; every decision
        // carries its frozen options and the selection.
        assert!(!snapshots.is_empty(), "a decisive moment was frozen");
        for snapshot in &snapshots {
            assert!(!snapshot.options.is_empty());
            assert!(
                snapshot
                    .options
                    .iter()
                    .any(|option| option.name == snapshot.selected),
                "the selection is among the frozen options"
            );
        }
        // The derivation is deterministic.
        assert_eq!(
            decision_snapshots(&events, &Policy::builtin()).unwrap(),
            snapshots
        );
        // Journal file cleanup for repeat runs.
        let _ = std::fs::remove_file(Path::new(&path));
    }
}
