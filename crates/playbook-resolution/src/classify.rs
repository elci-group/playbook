//! The typed failure taxonomy (§5), the classification engine (§6) and
//! diagnosis confidence (§7). Classification precedence is deterministic
//! first: explicit machine state, then provider termination metadata, then
//! tool status, then evidence output, then structure, then history. Model
//! assistance is last and marks the diagnosis as inferred.

use serde::{Deserialize, Serialize};

/// The refusal taxonomy. The five families — execution, capability,
/// work-product, evidence, governance — must never be collapsed (§5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCause {
    // Execution failure — the worker could not properly attempt the task.
    ProviderFailure,
    TransportFailure,
    RateLimited,
    CapabilityUnavailable,
    Timeout,
    // Envelope / capability failure — conditions, not competence.
    EnvelopeExhaustion,
    OutputTruncation,
    ContextExhaustion,
    BriefRigidity,
    BriefAmbiguity,
    SchemaMismatch,
    FormatMismatch,
    // Work-product failure — an attempt was made and is wrong/incomplete.
    WorkUnitTooLarge,
    IncompleteDecomposition,
    ArtefactMissing,
    ArtefactIncomplete,
    ArtefactCorrupt,
    OutputOwnershipCollision,
    ToolInvocationFailure,
    AnchorViolation,
    // Evidence failure — the work may be fine; the proof is not.
    EvidenceMissing,
    EvidenceStale,
    EvidenceWrongRevision,
    // Governance failure — the authority chain itself cannot verdict.
    VersionControlFailure,
    Unknown,
}

/// Which family a cause belongs to. Diagnoses in different families are
/// never treated as equivalent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalFamily {
    Execution,
    Capability,
    WorkProduct,
    Evidence,
    Governance,
}

impl RefusalCause {
    pub fn family(self) -> RefusalFamily {
        match self {
            Self::ProviderFailure
            | Self::TransportFailure
            | Self::RateLimited
            | Self::CapabilityUnavailable
            | Self::Timeout => RefusalFamily::Execution,
            Self::EnvelopeExhaustion
            | Self::OutputTruncation
            | Self::ContextExhaustion
            | Self::BriefRigidity
            | Self::BriefAmbiguity
            | Self::SchemaMismatch
            | Self::FormatMismatch => RefusalFamily::Capability,
            Self::WorkUnitTooLarge
            | Self::IncompleteDecomposition
            | Self::ArtefactMissing
            | Self::ArtefactIncomplete
            | Self::ArtefactCorrupt
            | Self::OutputOwnershipCollision
            | Self::ToolInvocationFailure
            | Self::AnchorViolation => RefusalFamily::WorkProduct,
            Self::EvidenceMissing | Self::EvidenceStale | Self::EvidenceWrongRevision => {
                RefusalFamily::Evidence
            }
            Self::VersionControlFailure | Self::Unknown => RefusalFamily::Governance,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::ProviderFailure => "provider_failure",
            Self::TransportFailure => "transport_failure",
            Self::RateLimited => "rate_limited",
            Self::CapabilityUnavailable => "capability_unavailable",
            Self::Timeout => "timeout",
            Self::EnvelopeExhaustion => "envelope_exhaustion",
            Self::OutputTruncation => "output_truncation",
            Self::ContextExhaustion => "context_exhaustion",
            Self::BriefRigidity => "brief_rigidity",
            Self::BriefAmbiguity => "brief_ambiguity",
            Self::SchemaMismatch => "schema_mismatch",
            Self::FormatMismatch => "format_mismatch",
            Self::WorkUnitTooLarge => "work_unit_too_large",
            Self::IncompleteDecomposition => "incomplete_decomposition",
            Self::ArtefactMissing => "artefact_missing",
            Self::ArtefactIncomplete => "artefact_incomplete",
            Self::ArtefactCorrupt => "artefact_corrupt",
            Self::OutputOwnershipCollision => "output_ownership_collision",
            Self::ToolInvocationFailure => "tool_invocation_failure",
            Self::AnchorViolation => "anchor_violation",
            Self::EvidenceMissing => "evidence_missing",
            Self::EvidenceStale => "evidence_stale",
            Self::EvidenceWrongRevision => "evidence_wrong_revision",
            Self::VersionControlFailure => "version_control_failure",
            Self::Unknown => "unknown",
        }
    }
}

/// Classification confidence (§7). Automatic remediation requires the
/// policy's minimum; deterministic machine-state evidence yields Certain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Unknown,
    Low,
    Moderate,
    High,
    Certain,
}

/// The observation a classifier reads: deterministic signals first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefusalObservation {
    /// Provider termination metadata, e.g. `"length"` (§6: token
    /// starvation is derivable without inference when present).
    pub finish_reason: Option<String>,
    /// HTTP/provider status or transport error text.
    pub provider_status: Option<String>,
    /// The tool/worker exit code, when one exists.
    pub exit_code: Option<i32>,
    /// Validation failures, verbatim from the gate.
    pub gate_failures: Vec<String>,
    /// Whether the produced artefact is structurally complete.
    pub output_structurally_complete: bool,
    /// An artefact expected at a known path was absent.
    pub artefact_expected_but_missing: bool,
    /// Two stages claimed the same output path.
    pub output_path_contended: bool,
    /// The version authority failed or wedged.
    pub version_control_failed: bool,
    /// How many prior attempts saw the same gate failures.
    pub prior_attempts_with_same_failures: u32,
    /// After brief normalisation (fences stripped, casing folded), did the
    /// content meet the brief's semantic requirements? `Some(true)` with
    /// a literal gate failure is the signature of brief rigidity.
    pub semantic_requirements_met_after_normalisation: Option<bool>,
    /// Structural inspection found the FIRST required element absent.
    pub first_required_item_missing: Option<bool>,
}

/// The diagnosis (§7): primary cause, confidence, and whether it was
/// derived deterministically or inferred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnosis {
    pub primary: RefusalCause,
    pub confidence: Confidence,
    pub deterministic: bool,
    pub rationale: String,
}

/// The classification engine (§6): deterministic evidence before
/// inference, highest-precedence signal wins, and the confidence follows
/// the evidence class — machine state is Certain, structure is High,
/// history is Moderate, and nothing here ever invents certainty.
pub fn classify(observation: &RefusalObservation) -> Diagnosis {
    // 1. Explicit machine state: version-control authority failure.
    if observation.version_control_failed {
        return Diagnosis {
            primary: RefusalCause::VersionControlFailure,
            confidence: Confidence::Certain,
            deterministic: true,
            rationale: "the version authority failed or wedged (explicit machine state)".into(),
        };
    }
    // 2. Provider termination metadata.
    if observation.finish_reason.as_deref() == Some("length")
        && !observation.output_structurally_complete
    {
        return Diagnosis {
            primary: RefusalCause::OutputTruncation,
            confidence: Confidence::Certain,
            deterministic: true,
            rationale: "finish_reason 'length' with structurally incomplete output (§6)".into(),
        };
    }
    // 3. Provider/API status with no substantive completion.
    if let Some(status) = &observation.provider_status {
        if observation.output_structurally_complete {
            return Diagnosis {
                primary: RefusalCause::BriefRigidity,
                confidence: Confidence::High,
                deterministic: true,
                rationale: "provider returned content but the gate rejected it — the brief or the validator owns the mismatch".into(),
            };
        }
        let rate_limited = status.contains("429") || status.to_lowercase().contains("rate");
        return Diagnosis {
            primary: if rate_limited {
                RefusalCause::RateLimited
            } else {
                RefusalCause::ProviderFailure
            },
            confidence: Confidence::Certain,
            deterministic: true,
            rationale: format!("provider status {status} without substantive completion"),
        };
    }
    // 4. Tool exit status.
    if let Some(code) = observation.exit_code {
        if code != 0 {
            return Diagnosis {
                primary: RefusalCause::ToolInvocationFailure,
                confidence: Confidence::High,
                deterministic: true,
                rationale: format!("tool exited {code}"),
            };
        }
    }
    // 5. Ownership contention is an explicit structural signal.
    if observation.output_path_contended {
        return Diagnosis {
            primary: RefusalCause::OutputOwnershipCollision,
            confidence: Confidence::Certain,
            deterministic: true,
            rationale: "two stages claimed one output path".into(),
        };
    }
    // 6. Evidence/gate output — structural inspection.
    if observation.artefact_expected_but_missing {
        return Diagnosis {
            primary: RefusalCause::ArtefactMissing,
            confidence: Confidence::High,
            deterministic: true,
            rationale: "expected artefact absent after invocation".into(),
        };
    }
    // Structural inspection: the first required element is the experiment's
    // known signature — anchors are imposed, not the work re-briefed.
    if observation.first_required_item_missing == Some(true) {
        return Diagnosis {
            primary: RefusalCause::AnchorViolation,
            confidence: Confidence::High,
            deterministic: true,
            rationale: "structural inspection: the first required element is absent (§12)".into(),
        };
    }
    if !observation.gate_failures.is_empty() {
        // A complete output whose content meets the brief after
        // normalisation was refused by wrapper rules: brief rigidity,
        // proven by the normalisation itself (Certain — it was fixed by
        // re-reading, not by re-working).
        if observation.semantic_requirements_met_after_normalisation == Some(true)
            && observation.output_structurally_complete
        {
            return Diagnosis {
                primary: RefusalCause::BriefRigidity,
                confidence: Confidence::Certain,
                deterministic: true,
                rationale:
                    "content meets the brief once normalised — the wrapper rules own the mismatch"
                        .into(),
            };
        }
        // Repeated identical failures across attempts indicate the work
        // unit, not the attempt (§41: retry is not remediation).
        let repeated = observation.prior_attempts_with_same_failures > 0;
        return Diagnosis {
            primary: if repeated {
                RefusalCause::WorkUnitTooLarge
            } else {
                RefusalCause::ArtefactIncomplete
            },
            confidence: if repeated {
                Confidence::High
            } else {
                Confidence::Moderate
            },
            deterministic: true,
            rationale: if repeated {
                "identical gate failures recurred across attempts — the work unit exceeds one response".into()
            } else {
                "gate reported unmet requirements on a completed attempt".into()
            },
        };
    }
    Diagnosis {
        primary: RefusalCause::Unknown,
        confidence: Confidence::Unknown,
        deterministic: false,
        rationale: "no deterministic signal present; escalation to specialist diagnosis".into(),
    }
}
