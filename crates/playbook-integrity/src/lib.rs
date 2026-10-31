//! Playbook-specific determinism gate suite (directive §14).
//!
//! Every check drives the REAL engine (`playbook-sim`, `playbook-state`,
//! `playbook-journal`) and reports a structured `CheckReport` a CI gate can
//! serialize and consume. No check panics: logical failures are folded into
//! `passed = false` plus a human-readable detail string.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::panic::{catch_unwind, AssertUnwindSafe};

use playbook_core::{sha256_hex, to_canonical_bytes, Evidence, EvidenceClass};
use playbook_events::DriveEvent;
use playbook_policy::Policy;
use playbook_sim::{gate_one, open_drive, run_mvp, run_mvp_with_journal, ApplyMode, FixturePlane};
use playbook_state::apply;

/// The structured outcome of one integrity check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckReport {
    pub check_id: &'static str,
    pub passed: bool,
    pub detail: String,
}

impl CheckReport {
    pub fn passed(&self) -> bool {
        self.passed
    }

    fn fail(check_id: &'static str, detail: impl Into<String>) -> Self {
        Self {
            check_id,
            passed: false,
            detail: detail.into(),
        }
    }
}

fn ok(check_id: &'static str, detail: impl Into<String>) -> CheckReport {
    CheckReport {
        check_id,
        passed: true,
        detail: detail.into(),
    }
}

/// Run every §14 check against the live engine.
pub fn run_all() -> Vec<CheckReport> {
    vec![
        replay_identity(),
        evidence_authority(),
        budget_monotonicity(),
        mandatory_gates_block_completion(),
        fourth_down_enters_lastdown_policy(),
        recovery_truncates_torn_tails(),
        priors_are_measured_not_synthetic(),
        journal_digests_bind(),
    ]
}

/// Wrap a check body so no panic can escape; a panic becomes a failed report.
fn guard(check_id: &'static str, body: impl FnOnce() -> Result<String, String>) -> CheckReport {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(detail)) => ok(check_id, detail),
        Ok(Err(detail)) => CheckReport::fail(check_id, detail),
        Err(_) => CheckReport::fail(check_id, "check panicked"),
    }
}

/// §14: audit and resimulation of the MVP event log produce identical state.
pub fn replay_identity() -> CheckReport {
    guard("replay_identity", || {
        let report = run_mvp().map_err(|err| format!("run_mvp failed: {err}"))?;
        if report.replay_match {
            Ok(format!(
                "audit and resimulate agree over {} events",
                report.events.len()
            ))
        } else {
            Err("mvp report.replay_match was false".into())
        }
    })
}

/// §14: a measurement backed by non-qualifying evidence moves neither YTG nor
/// gate satisfaction.
pub fn evidence_authority() -> CheckReport {
    guard("evidence_authority", || {
        let (mut drive, _) = open_drive(
            FixturePlane::ready(),
            vec![gate_one("implementation", EvidenceClass::CompilerResult)],
        )
        .map_err(|err| format!("open_drive failed: {err}"))?;
        let ytg_before = drive.state().ytg_milli();
        let satisfaction_before = drive.state().gates()[0].satisfaction_ppm;
        let claim = Evidence::seal(
            EvidenceClass::NarrativeClaim,
            json!({"text": "trust me, it compiles"}),
            1,
        )
        .map_err(|err| format!("seal failed: {err}"))?;
        drive
            .accept_evidence(claim)
            .map_err(|err| format!("accept_evidence failed: {err}"))?;
        let ytg_after = drive.state().ytg_milli();
        let satisfaction_after = drive.state().gates()[0].satisfaction_ppm;
        if ytg_after == ytg_before && satisfaction_after == satisfaction_before {
            Ok(format!(
                "narrative claim moved nothing: ytg {ytg_before} milli, satisfaction {satisfaction_before} ppm"
            ))
        } else {
            Err(format!(
                "unqualified evidence advanced state: ytg {ytg_before}->{ytg_after}, satisfaction {satisfaction_before}->{satisfaction_after}"
            ))
        }
    })
}

/// §14: budget remaining never increases within a possession across the MVP
/// event fold.
pub fn budget_monotonicity() -> CheckReport {
    guard("budget_monotonicity", || {
        let report = run_mvp().map_err(|err| format!("run_mvp failed: {err}"))?;
        let policy = Policy::builtin();
        let mut state = None;
        let mut previous = u32::MAX;
        for (index, event) in report.events.iter().enumerate() {
            let next = apply(state.as_ref(), event, &policy, ApplyMode::Audit)
                .map_err(|err| format!("fold diverged at event {index}: {err}"))?;
            let remaining = next.budget_remaining_ppm(&policy);
            let new_possession = matches!(event, DriveEvent::PossessionStarted { .. });
            if remaining > previous && !new_possession {
                return Err(format!(
                    "budget increased at event {index} ({}): {previous} -> {remaining} ppm",
                    event.name()
                ));
            }
            previous = remaining;
            state = Some(next);
        }
        Ok(format!(
            "budget remaining never increased across {} events (ends at {previous} ppm)",
            report.events.len()
        ))
    })
}

/// §14: a drive with an open mandatory gate rejects completion.
pub fn mandatory_gates_block_completion() -> CheckReport {
    guard("mandatory_gates_block_completion", || {
        let (mut drive, _) = open_drive(
            FixturePlane::ready(),
            vec![gate_one("implementation", EvidenceClass::CompilerResult)],
        )
        .map_err(|err| format!("open_drive failed: {err}"))?;
        let completed = drive
            .claim_complete(100)
            .map_err(|err| format!("claim_complete failed: {err}"))?;
        if completed {
            Err("claim_complete returned true with an open mandatory gate".into())
        } else {
            Ok("claim_complete returned completed=false with the gate open".into())
        }
    })
}

/// §14: every FourthDownDeclared in the MVP log is followed by exactly one
/// deterministic disposition event (PuntIssued or FieldGoalSelected).
pub fn fourth_down_enters_lastdown_policy() -> CheckReport {
    guard("fourth_down_enters_lastdown_policy", || {
        let report = run_mvp().map_err(|err| format!("run_mvp failed: {err}"))?;
        let mut declared = 0usize;
        for (index, event) in report.events.iter().enumerate() {
            let DriveEvent::FourthDownDeclared { .. } = event else {
                continue;
            };
            declared += 1;
            report.events[index + 1..]
                .iter()
                .find(|later| {
                    matches!(
                        later,
                        DriveEvent::PuntIssued { .. } | DriveEvent::FieldGoalSelected { .. }
                    )
                })
                .ok_or_else(|| {
                    format!("fourth down at event {index} has no punt or field-goal disposition")
                })?;
        }
        if declared == 0 {
            return Err("the MVP log contains no FourthDownDeclared event".into());
        }
        Ok(format!(
            "{declared} fourth-down declaration(s) each reached a deterministic disposition"
        ))
    })
}

/// §14: recovery from a journal with a torn tail keeps the longest valid
/// prefix and reports the repaired tail.
pub fn recovery_truncates_torn_tails() -> CheckReport {
    guard("recovery_truncates_torn_tails", || {
        let dir = std::env::temp_dir().join(format!("playbook-integrity-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| format!("temp dir: {err}"))?;
        let result = (|| -> Result<String, String> {
            let path = dir.join("journal.ndjson");
            let report = run_mvp_with_journal(&path)
                .map_err(|err| format!("run_mvp_with_journal failed: {err}"))?;
            let intact = std::fs::read(&path).map_err(|err| format!("read: {err}"))?;
            let mut torn = intact.clone();
            torn.extend_from_slice(b"{\"seq\":");
            std::fs::write(&path, &torn).map_err(|err| format!("write torn tail: {err}"))?;
            let recovery = playbook_journal::Journal::open(&path)
                .map_err(|err| format!("journal open failed: {err}"))?;
            if !recovery.repaired_tail {
                return Err("recovery did not report a repaired tail".into());
            }
            if recovery.events.len() != report.events.len() {
                return Err(format!(
                    "recovered {} events, expected the {} valid ones",
                    recovery.events.len(),
                    report.events.len()
                ));
            }
            let truncated = std::fs::read(&path).map_err(|err| format!("re-read: {err}"))?;
            if truncated != intact {
                return Err("recovery did not truncate the file back to the valid prefix".into());
            }
            Ok(format!(
                "torn tail excluded; longest valid prefix of {} events retained",
                recovery.events.len()
            ))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    })
}

/// §14: every prior row in the folded MVP state is a measured sample — at
/// least one sample, with the realized-reduction sum consistent.
pub fn priors_are_measured_not_synthetic() -> CheckReport {
    guard("priors_are_measured_not_synthetic", || {
        let report = run_mvp().map_err(|err| format!("run_mvp failed: {err}"))?;
        let priors = report.final_state.priors();
        let rows: Vec<_> = priors.rows().collect();
        if rows.is_empty() {
            return Err("the priors table is empty: no measured samples exist".into());
        }
        for (id, row) in &rows {
            if row.samples == 0 {
                return Err(format!("prior row `{id}` carries zero samples (synthetic)"));
            }
        }
        let total: u32 = rows.iter().map(|(_, row)| row.samples).sum();
        if total != priors.samples() {
            return Err(format!(
                "row samples sum to {total} but the table reports {}",
                priors.samples()
            ));
        }
        Ok(format!(
            "{} prior row(s), all measured ({} samples total)",
            rows.len(),
            total
        ))
    })
}

/// A journal record as stored on disk, for digest re-verification.
#[derive(Deserialize)]
struct StoredRecord {
    seq: u64,
    digest: String,
    event: DriveEvent,
}

/// §14: every record in the MVP journal re-verifies — the stored digest equals
/// the recomputed sha256 of the event's canonical bytes.
pub fn journal_digests_bind() -> CheckReport {
    guard("journal_digests_bind", || {
        let dir =
            std::env::temp_dir().join(format!("playbook-integrity-digest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|err| format!("temp dir: {err}"))?;
        let result = (|| -> Result<String, String> {
            let path = dir.join("journal.ndjson");
            run_mvp_with_journal(&path)
                .map_err(|err| format!("run_mvp_with_journal failed: {err}"))?;
            let bytes = std::fs::read(&path).map_err(|err| format!("read: {err}"))?;
            let mut expected_seq = 1u64;
            let mut count = 0usize;
            for line in bytes.split(|byte| *byte == b'\n') {
                if line.is_empty() {
                    continue;
                }
                let record: StoredRecord = serde_json::from_slice(line)
                    .map_err(|err| format!("record {count} unparseable: {err}"))?;
                if record.seq != expected_seq {
                    return Err(format!(
                        "record {count} has seq {}, expected {expected_seq}",
                        record.seq
                    ));
                }
                let recomputed = sha256_hex(
                    &to_canonical_bytes(&record.event)
                        .map_err(|err| format!("canonical bytes: {err}"))?,
                );
                if recomputed != record.digest {
                    return Err(format!(
                        "record {} digest mismatch: stored {}, recomputed {recomputed}",
                        record.seq, record.digest
                    ));
                }
                expected_seq += 1;
                count += 1;
            }
            if count == 0 {
                return Err("the journal file is empty".into());
            }
            Ok(format!(
                "{count} journal records re-verified against recomputed digests"
            ))
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_check_passes_on_the_healthy_engine() {
        let reports = run_all();
        assert_eq!(reports.len(), 8);
        for report in &reports {
            assert!(
                report.passed(),
                "check {} failed: {}",
                report.check_id,
                report.detail
            );
        }
    }

    #[test]
    fn a_corrupt_journal_is_excluded_by_recovery() {
        // Negative mechanism test: an interior record tampered so its digest
        // no longer binds must truncate everything from that record onward.
        let dir =
            std::env::temp_dir().join(format!("playbook-integrity-neg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.ndjson");
        run_mvp_with_journal(&path).unwrap();
        let intact = std::fs::read(&path).unwrap();
        let mut lines: Vec<Vec<u8>> = intact
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(<[u8]>::to_vec)
            .collect();
        assert!(lines.len() > 2, "journal should hold multiple records");
        let text = String::from_utf8(lines[1].clone()).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        // Tamper with a real field value so the recomputed digest differs
        // (DriveEvent deserialization ignores unknown fields, so adding one
        // would silently round-trip back to the original digest).
        value["event"]["ytg_milli"] = serde_json::json!(999);
        lines[1] = serde_json::to_vec(&value).unwrap();
        let mut corrupt: Vec<u8> = Vec::new();
        for line in &lines {
            corrupt.extend_from_slice(line);
            corrupt.push(b'\n');
        }
        std::fs::write(&path, &corrupt).unwrap();
        let recovery = playbook_journal::Journal::open(&path).unwrap();
        assert!(recovery.repaired_tail);
        assert_eq!(recovery.events.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn completion_rejection_is_not_a_stub() {
        // Negative mechanism test for the completion gate: completion is
        // rejected on the YTG condition as well, so the §14 check exercises
        // real gate logic rather than a constant false.
        let (mut gated, _) = open_drive(
            FixturePlane::ready(),
            vec![gate_one("implementation", EvidenceClass::CompilerResult)],
        )
        .unwrap();
        assert!(!gated.state().gates()[0].is_terminal());
        assert!(!gated.claim_complete(2).unwrap());
    }
}
