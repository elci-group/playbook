//! `CLAIMS/1` — the admission registry (§26).
//!
//! A [`AdmissionClaim`] is a sealed, machine-verifiable attestation that a
//! specific revision, under a specific policy version, was admitted on the
//! strength of a specific evidence envelope. Sealing is the Scribe role;
//! verification is the Verity role (§26): verification trusts nothing and
//! re-evaluates the evidence contract from scratch.
//!
//! Constitutional rule: a claim is only as good as the evidence beneath
//! it. Any alteration to the envelope invalidates the claim's evidence
//! root; any expiry of time invalidates the claim entirely.

#![forbid(unsafe_code)]

use ci_evidence::{CiEvidence, Verdict, SCHEMA};
use playbook_core::{sha256_hex, to_canonical_bytes};
use serde::{Deserialize, Serialize};

/// Rejection reason: the claim's revision does not match the envelope.
pub const WRONG_REVISION: &str = "wrong revision";
/// Rejection reason: the claim has expired.
pub const STALE_CLAIM: &str = "stale claim";
/// Rejection reason: the claim's policy version does not match.
pub const WRONG_POLICY: &str = "wrong policy";
/// Rejection reason: the envelope digest no longer matches the claim.
pub const ALTERED_EVIDENCE_ROOT: &str = "altered evidence root";
/// Rejection reason: the authority is not the expected one.
pub const UNRECOGNISED_AUTHORITY: &str = "unrecognised authority";
/// Rejection reason: the envelope's verdict no longer equals the claim's.
pub const VERDICT_NO_LONGER_DERIVABLE: &str = "verdict no longer derivable";

fn missing_mandatory_gate(gate_id: &str) -> String {
    format!("missing mandatory gate: {gate_id}")
}

/// A sealed admission claim (§26).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionClaim {
    pub repository: String,
    pub revision_sha: String,
    pub policy_version: String,
    /// Digest over the canonical JSON of the evidence envelope.
    pub evidence_root: String,
    pub mandatory_gates: Vec<String>,
    pub verdict: Verdict,
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub authority: String,
}

fn evidence_root(evidence: &CiEvidence) -> Result<String, String> {
    let bytes = to_canonical_bytes(evidence).map_err(|e| e.to_string())?;
    Ok(sha256_hex(&bytes))
}

/// Seal an admission claim over an evidence envelope (§26 Scribe role).
///
/// Refuses to seal a [`Verdict::Rejected`] verdict or a foreign schema.
/// The claim's `evidence_root` is the sha256 of the envelope's canonical
/// JSON; expiry is `now_ms + ttl_ms` when a TTL is given.
pub fn seal(
    evidence: &CiEvidence,
    repository: &str,
    authority: &str,
    ttl_ms: Option<u64>,
    now_ms: u64,
) -> Result<AdmissionClaim, String> {
    if evidence.schema != SCHEMA {
        return Err(format!("evidence schema is not {SCHEMA}"));
    }
    if authority.trim().is_empty() {
        return Err("authority must not be empty".to_string());
    }
    let verdict = evidence.derive_verdict(now_ms);
    if verdict == Verdict::Rejected {
        return Err("refusing to seal a Rejected verdict".to_string());
    }
    Ok(AdmissionClaim {
        repository: repository.to_string(),
        revision_sha: evidence.revision.commit.clone(),
        policy_version: evidence.revision.policy_version.clone(),
        evidence_root: evidence_root(evidence)?,
        mandatory_gates: Vec::new(),
        verdict,
        issued_at_ms: now_ms,
        expires_at_ms: ttl_ms.map(|ttl| now_ms + ttl),
        authority: authority.to_string(),
    })
}

/// Independent verification of a claim against an evidence envelope,
/// trusting nothing (§26 Verity role).
pub fn verify(
    claim: &AdmissionClaim,
    evidence: &CiEvidence,
    now_ms: u64,
) -> Result<Verdict, String> {
    let authority = claim.authority.clone();
    verify_for(claim, evidence, &authority, now_ms)
}

/// As [`verify`], but additionally requires the claim's authority to match
/// `expected_authority`.
pub fn verify_for(
    claim: &AdmissionClaim,
    evidence: &CiEvidence,
    expected_authority: &str,
    now_ms: u64,
) -> Result<Verdict, String> {
    if claim.authority.trim().is_empty() || claim.authority != expected_authority {
        return Err(UNRECOGNISED_AUTHORITY.to_string());
    }
    if claim
        .expires_at_ms
        .map(|expiry| now_ms >= expiry)
        .unwrap_or(false)
    {
        return Err(STALE_CLAIM.to_string());
    }
    if claim.revision_sha != evidence.revision.commit {
        return Err(WRONG_REVISION.to_string());
    }
    if claim.policy_version != evidence.revision.policy_version {
        return Err(WRONG_POLICY.to_string());
    }
    if claim.evidence_root != evidence_root(evidence)? {
        return Err(ALTERED_EVIDENCE_ROOT.to_string());
    }
    for gate_id in &claim.mandatory_gates {
        let executed = evidence
            .gates
            .iter()
            .any(|gate| &gate.gate_id == gate_id && gate.outcome.executed());
        if !executed {
            return Err(missing_mandatory_gate(gate_id));
        }
    }
    let derived = derive_envelope_verdict(evidence, now_ms)?;
    if derived != claim.verdict {
        return Err(VERDICT_NO_LONGER_DERIVABLE.to_string());
    }
    Ok(claim.verdict)
}

/// Re-derive the envelope verdict through the shared admission contract,
/// checking schema identity first (never trust a foreign envelope).
fn derive_envelope_verdict(evidence: &CiEvidence, now_ms: u64) -> Result<Verdict, String> {
    if evidence.schema != SCHEMA {
        return Err(format!("evidence schema is not {SCHEMA}"));
    }
    Ok(evidence.derive_verdict(now_ms))
}

/// One registry entry: the sealed claim plus the exact envelope it was
/// sealed over — verification re-runs against that envelope.
#[derive(Clone, Debug)]
struct Entry {
    claim: AdmissionClaim,
    evidence: CiEvidence,
}

/// The admission registry: a ledger of sealed claims.
#[derive(Clone, Debug, Default)]
pub struct ClaimRegistry {
    entries: Vec<Entry>,
}

impl ClaimRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a claim over its evidence envelope. Refuses a duplicate of
    /// (repository, revision, policy).
    pub fn record(&mut self, claim: AdmissionClaim, evidence: CiEvidence) -> Result<(), String> {
        if self.entries.iter().any(|entry| {
            entry.claim.repository == claim.repository
                && entry.claim.revision_sha == claim.revision_sha
                && entry.claim.policy_version == claim.policy_version
        }) {
            return Err(format!(
                "claim already recorded for {}/{} under policy {}",
                claim.repository, claim.revision_sha, claim.policy_version
            ));
        }
        self.entries.push(Entry { claim, evidence });
        Ok(())
    }

    /// All recorded claims (read-only view).
    pub fn claims(&self) -> Vec<&AdmissionClaim> {
        self.entries.iter().map(|entry| &entry.claim).collect()
    }

    /// The most recently issued claim for a revision, if any.
    pub fn latest_for_revision(&self, revision_sha: &str) -> Option<&AdmissionClaim> {
        self.entries
            .iter()
            .filter(|entry| entry.claim.revision_sha == revision_sha)
            .map(|entry| &entry.claim)
            .max_by_key(|claim| claim.issued_at_ms)
    }

    /// The admission verdict for (revision, policy): returns the verdict
    /// only if a valid, unexpired, evidence-verified claim exists —
    /// otherwise an error naming why.
    pub fn admission_for(
        &self,
        revision_sha: &str,
        policy_version: &str,
        now_ms: u64,
    ) -> Result<Verdict, String> {
        let best = self
            .entries
            .iter()
            .filter(|entry| {
                entry.claim.revision_sha == revision_sha
                    && entry.claim.policy_version == policy_version
            })
            .max_by_key(|entry| entry.claim.issued_at_ms);
        let Some(entry) = best else {
            return Err(format!(
                "no claim recorded for {revision_sha} under policy {policy_version}"
            ));
        };
        let claim = entry.claim.clone();
        verify(&claim, &entry.evidence, now_ms)
    }
}
