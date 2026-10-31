//! Scrawny authority adapter — change-coherence gate (`ci/scrawny`).
//!
//! # Policy (directive §6)
//!
//! There is NO scrawny binary consulted by this gate in CI. It is a
//! deterministic change-coherence heuristic standing in as the initial
//! gate: review-load extremes and top-level concern mixing are measured
//! from the changed-file list alone, so the gate is reproducible and
//! needs no toolchain. Automated splitting (`scrawny split --apply`)
//! stays OUTSIDE CI — CI records the coherence evidence; a human or an
//! out-of-band workflow decides whether to split, and no automatic
//! rewrite of the working tree ever happens inside a gate.
//!
//! Bands:
//!
//! * `lines_changed > 2000` → `Fail` with `extreme_review_load`
//!   (classification is Blocking: extreme load blocks).
//! * more than 6 distinct top-level directories touched → `Fail` with
//!   `severe_cohesion_failure`.
//! * 3–6 distinct top-level directories touched → `Pass` with warning
//!   findings `moderate_concern_mixing`.
//! * otherwise → clean `Pass`.

use std::time::Instant;

use ci_evidence::{Finding, GateClassification, GateOutcome, GateRecord};

use crate::runner::input_hash;
use crate::GateSpec;

/// Extreme review load threshold (directive §6).
pub const EXTREME_LOAD_LINES: u64 = 2000;

/// Distinct top-level directories above which concern mixing is severe.
pub const SEVERE_MIXING_DIRS: usize = 6;

/// Distinct top-level directories at or above which concern mixing is
/// moderate.
pub const MODERATE_MIXING_DIRS: usize = 3;

/// Run the deterministic change-coherence gate over `files_changed`.
pub fn scrawny_gate(
    policy_version: &str,
    files_changed: &[&str],
    lines_changed: u64,
) -> GateRecord {
    let spec = GateSpec::new("ci/scrawny", "scrawny", GateClassification::Blocking);
    let started = Instant::now();
    let started_at_ms = now_ms();

    let dirs = distinct_top_level_dirs(files_changed);
    let (outcome, mut findings) = coherence_verdict(dirs.len(), lines_changed);

    // The load band dominates, but severe mixing should still be
    // recorded as evidence alongside it.
    if lines_changed > EXTREME_LOAD_LINES && dirs.len() > SEVERE_MIXING_DIRS {
        findings.push(Finding {
            code: "severe_cohesion_failure".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "extreme review load also mixes {} top-level concerns (subsumed by extreme_review_load)",
                dirs.len()
            ),
        });
    }

    let mut hash_parts: Vec<String> = files_changed.iter().map(|f| (*f).to_string()).collect();
    hash_parts.push(lines_changed.to_string());
    let hash_refs: Vec<&str> = hash_parts.iter().map(|s| s.as_str()).collect();
    let input_digest = input_hash(policy_version, "scrawny-heuristic", &hash_refs);

    GateRecord {
        gate_id: spec.gate_id,
        authority: spec.authority,
        authority_version: "heuristic/1".to_string(),
        policy_version: policy_version.to_string(),
        classification: spec.classification,
        started_at_ms,
        completed_at_ms: now_ms(),
        input_hash: input_digest,
        outcome,
        exit_code: None,
        findings,
        artifact_hashes: Vec::new(),
        duration_ms: started.elapsed().as_millis() as u64,
    }
}

/// The band verdict for a given directory count and changed-line count.
/// Exposed for tests and future callers.
pub fn coherence_verdict(dirs: usize, lines_changed: u64) -> (GateOutcome, Vec<Finding>) {
    if lines_changed > EXTREME_LOAD_LINES {
        return (
            GateOutcome::Fail,
            vec![Finding {
                code: "extreme_review_load".to_string(),
                severity: "error".to_string(),
                message: format!(
                    "{lines_changed} changed lines exceed the {EXTREME_LOAD_LINES}-line extreme review-load threshold; split before merging"
                ),
            }],
        );
    }
    if dirs > SEVERE_MIXING_DIRS {
        return (
            GateOutcome::Fail,
            vec![Finding {
                code: "severe_cohesion_failure".to_string(),
                severity: "error".to_string(),
                message: format!(
                    "{dirs} distinct top-level directories touched — severe concern mixing"
                ),
            }],
        );
    }
    if dirs >= MODERATE_MIXING_DIRS {
        return (
            GateOutcome::Pass,
            vec![Finding {
                code: "moderate_concern_mixing".to_string(),
                severity: "warning".to_string(),
                message: format!(
                    "{dirs} distinct top-level directories touched — moderate concern mixing"
                ),
            }],
        );
    }
    (GateOutcome::Pass, Vec::new())
}

/// Distinct first path components among the changed files. Files at the
/// repository root (no separator) count as one pseudo-directory "".
fn distinct_top_level_dirs<'a>(files: &[&'a str]) -> Vec<&'a str> {
    let mut seen: Vec<&str> = Vec::new();
    for file in files {
        let top = match file.split('/').next() {
            Some(t) if file.contains('/') => t,
            _ => "",
        };
        if !seen.contains(&top) {
            seen.push(top);
        }
    }
    seen
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_band_is_hit() {
        // Extreme load dominates regardless of directories.
        let (outcome, findings) = coherence_verdict(2, 2001);
        assert_eq!(outcome, GateOutcome::Fail);
        assert_eq!(findings[0].code, "extreme_review_load");

        // Severe mixing: 7+ directories, sub-threshold load.
        let (outcome, findings) = coherence_verdict(7, 100);
        assert_eq!(outcome, GateOutcome::Fail);
        assert_eq!(findings[0].code, "severe_cohesion_failure");

        // Moderate mixing: 3–6 directories, passes with warnings.
        let (outcome, findings) = coherence_verdict(4, 100);
        assert_eq!(outcome, GateOutcome::Pass);
        assert!(findings.iter().all(|f| f.code == "moderate_concern_mixing"));

        // Clean pass.
        let (outcome, findings) = coherence_verdict(2, 100);
        assert_eq!(outcome, GateOutcome::Pass);
        assert!(findings.is_empty());
    }

    #[test]
    fn boundaries() {
        assert_eq!(coherence_verdict(6, 2000).0, GateOutcome::Pass);
        assert_eq!(coherence_verdict(7, 2000).0, GateOutcome::Fail);
    }

    #[test]
    fn record_carries_blocking_classification_and_bands() {
        let files = ["crates/a.rs", "crates/b.rs", "docs/x.md", "src/main.rs"];
        let record = scrawny_gate("test-policy", &files, 50);
        assert_eq!(record.gate_id, "ci/scrawny");
        assert_eq!(record.classification, GateClassification::Blocking);
        assert_eq!(record.outcome, GateOutcome::Pass);
        assert!(record
            .findings
            .iter()
            .any(|f| f.code == "moderate_concern_mixing"));

        let record = scrawny_gate("test-policy", &files, 5000);
        assert_eq!(record.outcome, GateOutcome::Fail);
        assert!(record
            .findings
            .iter()
            .any(|f| f.code == "extreme_review_load"));
    }
}
