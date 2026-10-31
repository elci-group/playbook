//! Verity authority adapter — claim-vs-reality gate (`ci/verity`).
//!
//! Conditional by policy: a verity failure blocks only when the current
//! policy declares it required.
//!
//! # Deterministic baseline (this implementation)
//!
//! This is a deterministic claim-vs-reality *baseline*, not the full
//! verity registry. The complete CLAIMS/1 registry — a machine-readable
//! ledger of documented claims, each bound to the test that proves it —
//! is future work. Until that registry exists, the only mechanically
//! checkable claim is: documentation that changed must be accompanied
//! by at least one changed test. A documentation change with no test
//! reference among `test_refs` yields the advisory finding
//! `unsupported_documentation_change` (the gate still passes; verity is
//! Conditional, and an advisory finding never independently blocks —
//! the registry upgrade will harden this into claim-level verification).

use std::time::Instant;

use ci_evidence::{Finding, GateClassification, GateOutcome, GateRecord};

use crate::runner::input_hash;
use crate::GateSpec;

/// Run the deterministic verity baseline over the changed docs.
pub fn verity_gate(policy_version: &str, docs_changed: &[&str], test_refs: &[&str]) -> GateRecord {
    let spec = GateSpec::new("ci/verity", "verity", GateClassification::Conditional);
    let started = Instant::now();
    let started_at_ms = now_ms();

    let mut outcome = GateOutcome::Pass;
    let mut findings = Vec::new();

    let docs_changed_nonempty = !docs_changed.is_empty();
    let has_test_reference = test_refs.iter().any(|t| looks_like_test(t));
    if docs_changed_nonempty && !has_test_reference {
        findings.push(Finding {
            code: "unsupported_documentation_change".to_string(),
            severity: "advisory".to_string(),
            message: format!(
                "{} documented file(s) changed with no test reference among {} ref(s); claims lack a proving test (full CLAIMS/1 registry is future work)",
                docs_changed.len(),
                test_refs.len()
            ),
        });
    }
    let _ = &mut outcome;

    let mut hash_parts: Vec<String> = docs_changed.iter().map(|f| (*f).to_string()).collect();
    hash_parts.extend(test_refs.iter().map(|t| (*t).to_string()));
    let hash_refs: Vec<&str> = hash_parts.iter().map(|s| s.as_str()).collect();
    let input_digest = input_hash(policy_version, "verity-baseline", &hash_refs);

    GateRecord {
        gate_id: spec.gate_id,
        authority: spec.authority,
        authority_version: "baseline/1".to_string(),
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

/// Whether a reference looks like a test artifact: a path whose file
/// name or a containing directory mentions `test`/`spec`/`benches`, or
/// a bare test-function name.
fn looks_like_test(reference: &str) -> bool {
    let lower = reference.to_lowercase();
    lower.contains("test") || lower.contains("spec") || lower.contains("bench")
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
    fn unsupported_docs_change_is_flagged_advisory() {
        let record = verity_gate("test-policy", &["docs/guide.md"], &[]);
        assert_eq!(record.outcome, GateOutcome::Pass);
        assert_eq!(record.findings.len(), 1);
        assert_eq!(record.findings[0].code, "unsupported_documentation_change");
        assert_eq!(record.findings[0].severity, "advisory");
    }

    #[test]
    fn docs_with_test_reference_pass_clean() {
        let record = verity_gate(
            "test-policy",
            &["docs/guide.md"],
            &["crates/verity/tests/guide.rs"],
        );
        assert_eq!(record.outcome, GateOutcome::Pass);
        assert!(record.findings.is_empty());
    }

    #[test]
    fn no_docs_change_is_clean() {
        let record = verity_gate("test-policy", &[], &[]);
        assert_eq!(record.outcome, GateOutcome::Pass);
        assert!(record.findings.is_empty());
    }
}
