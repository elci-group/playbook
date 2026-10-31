//! `CI_EVIDENCE/1` — the canonical machine-readable evidence contract for
//! Playbook's evidence-gated CI campaign.
//!
//! Constitutional rule: no claim of correctness, quality, completion or
//! release readiness derives solely from the component making that claim.
//! Specialist authorities observe; this contract records; policy decides;
//! Kaptaind admits (§36). The verdict is mechanically derivable from the
//! evidence object — no prose interpretation overrides it (§16).
//!
//! Failure semantics are closed (§30): `ERROR` never equals `PASS`,
//! `INCONCLUSIVE` never equals `PASS`, and a required gate that cannot
//! execute fails closed unless a recorded waiver says otherwise.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// The schema identity of every envelope.
pub const SCHEMA: &str = "CI_EVIDENCE/1";

/// The four authority classes of a gate (§4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateClassification {
    /// Failure makes the revision inadmissible.
    Blocking,
    /// Failure blocks only when applicable policy declares it required.
    Conditional,
    /// Evidence is recorded; it never independently prevents admission.
    Advisory,
    /// Measurement only — never admission-relevant (§4).
    Observational,
}

/// The six failure semantics (§30). There is no silent collapse between
/// them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateOutcome {
    Pass,
    Fail,
    Error,
    SkippedNotApplicable,
    SkippedAuthorised,
    Inconclusive,
}

impl GateOutcome {
    /// Whether this outcome satisfies a gate that must run (§30: fail
    /// closed — only an explicit pass counts).
    pub fn satisfies(self) -> bool {
        matches!(self, Self::Pass)
    }

    /// Whether this outcome counts as executed at all.
    pub fn executed(self) -> bool {
        matches!(self, Self::Pass | Self::Fail | Self::Error)
    }
}

/// The frozen identity of the revision under verification (§5). Evidence
/// generated against revision A never authorises revision B.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionFreeze {
    pub commit: String,
    pub parent: String,
    pub branch: String,
    /// Digest over the full diff.
    pub diff_digest: String,
    /// Digest over Cargo.lock.
    pub lock_digest: String,
    pub toolchain: String,
    pub policy_version: String,
    pub implementation_version: String,
    pub config_hashes: std::collections::BTreeMap<String, String>,
    pub frozen_at_ms: u64,
}

/// One finding inside a gate record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub code: String,
    pub severity: String,
    pub message: String,
}

/// One gate's evidence record (§16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateRecord {
    pub gate_id: String,
    pub authority: String,
    pub authority_version: String,
    pub policy_version: String,
    pub classification: GateClassification,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    /// Digest over the gate's exact inputs.
    pub input_hash: String,
    pub outcome: GateOutcome,
    pub exit_code: Option<i32>,
    pub findings: Vec<Finding>,
    pub artifact_hashes: Vec<String>,
    pub duration_ms: u64,
}

/// An explicit, attributable, time-bounded waiver (§29). Waivers are
/// themselves evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    pub gate_id: String,
    pub reason: String,
    pub author: String,
    pub expires_at_ms: Option<u64>,
    /// The revision this waiver was recorded against.
    pub recorded_at_revision: String,
}

/// The mechanically derived verdict (§16).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Admitted,
    Rejected,
}

/// The canonical envelope (§16).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CiEvidence {
    pub schema: String,
    pub revision: RevisionFreeze,
    pub gates: Vec<GateRecord>,
    pub waivers: Vec<Waiver>,
}

impl CiEvidence {
    /// Open an envelope for a frozen revision.
    pub fn new(revision: RevisionFreeze) -> Self {
        Self {
            schema: SCHEMA.to_string(),
            revision,
            gates: Vec::new(),
            waivers: Vec::new(),
        }
    }

    pub fn record(&mut self, gate: GateRecord) {
        self.gates.push(gate);
    }

    /// The waiver state of one gate: unwaivered, authorised (a valid,
    /// unexpired waiver exists), or present-but-invalid.
    fn waiver_for(&self, gate_id: &str, now_ms: u64) -> WaiverState {
        let mut state = WaiverState::None;
        for waiver in &self.waivers {
            if waiver.gate_id != gate_id {
                continue;
            }
            if waiver
                .expires_at_ms
                .map(|expiry| expiry <= now_ms)
                .unwrap_or(false)
            {
                return WaiverState::Expired;
            }
            if waiver.reason.trim().is_empty() || waiver.author.trim().is_empty() {
                return WaiverState::Invalid;
            }
            state = WaiverState::Authorised;
        }
        state
    }

    /// Derive the verdict mechanically (§16). Blocking gates must pass,
    /// unless an authorised waiver covers them (§29) — and a waived
    /// blocking gate is still recorded as failed-with-waiver, never
    /// rewritten into a pass.
    pub fn derive_verdict(&self, now_ms: u64) -> Verdict {
        for gate in &self.gates {
            let required = matches!(gate.classification, GateClassification::Blocking)
                || matches!(gate.outcome, GateOutcome::Error | GateOutcome::Inconclusive);
            if !required {
                continue;
            }
            if gate.outcome.satisfies() {
                continue;
            }
            // A blocking gate that did not pass: admissible only under an
            // authorised waiver.
            if matches!(
                self.waiver_for(&gate.gate_id, now_ms),
                WaiverState::Authorised
            ) {
                continue;
            }
            return Verdict::Rejected;
        }
        Verdict::Admitted
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaiverState {
    None,
    Authorised,
    Expired,
    Invalid,
}

/// Independent admission verification (§17): Kaptaind's question — does
/// this exact revision possess the evidence required by the current
/// policy? Checks revision identity, schema identity, mandatory gate
/// presence, and re-derives the verdict; it never trusts a recorded
/// verdict field.
pub fn verify_admission(
    evidence: &CiEvidence,
    expected_commit: &str,
    expected_policy_version: &str,
    required_gates: &[&str],
    now_ms: u64,
) -> Result<Verdict, String> {
    if evidence.schema != SCHEMA {
        return Err(format!("evidence schema is not {SCHEMA}"));
    }
    if evidence.revision.commit != expected_commit {
        return Err(format!(
            "evidence was generated against revision {} — it does not authorise {expected_commit}",
            evidence.revision.commit
        ));
    }
    if evidence.revision.policy_version != expected_policy_version {
        return Err(format!(
            "evidence policy {} does not match admission policy {expected_policy_version}",
            evidence.revision.policy_version
        ));
    }
    for required in required_gates {
        let present = evidence
            .gates
            .iter()
            .any(|gate| gate.gate_id == *required && gate.outcome.executed());
        if !present {
            return Err(format!("mandatory gate {required} did not execute"));
        }
    }
    Ok(evidence.derive_verdict(now_ms))
}
