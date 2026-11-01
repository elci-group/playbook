//! Artefact ownership (§13): no two stages silently claim one output
//! path. The deliver-generator/model collision from the experiment is the
//! canonical regression case.

use serde::{Deserialize, Serialize};

use playbook_core::sha256_hex;

/// Who may write an artefact, and how (§13).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    /// No writer may replace the artefact once it exists.
    Forbidden,
    /// The registered owner may replace its own artefact.
    ReplaceOwned,
    /// Writers append; existing bytes are immutable.
    Append,
    /// Writers merge under a declared contract.
    Merge,
    /// Only artefacts derived from this one may be written elsewhere.
    DerivedOnly,
    /// Replacement requires an explicit, recorded ownership transfer.
    ExplicitTransfer,
}

/// The ownership record of one output path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtefactOwnership {
    pub path: String,
    pub owner: String,
    pub producer: String,
    pub permitted_mutators: Vec<String>,
    pub overwrite_policy: OverwritePolicy,
    /// The producer stage that generated the artefact.
    pub generated_by: String,
    pub revision: u32,
}

/// The verdict of a write attempt against the ownership record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnershipVerdict {
    Allowed,
    /// Denied: the artefact is immutable and the writer is not the owner.
    ForbiddenByPolicy,
    /// Denied: the writer is not a permitted mutator.
    NotAMutator,
    /// Denied: replacement requires a recorded transfer first.
    TransferRequired,
}

/// Decide a write attempt. `exists` is whether the artefact is already on
/// disk. Silent replacement of model-authored artefacts is impossible:
/// every denial is a named verdict, never a quiet overwrite (§13).
pub fn check_write(
    record: &ArtefactOwnership,
    writer: &str,
    exists: bool,
    transfer_recorded: bool,
) -> OwnershipVerdict {
    if !exists {
        return OwnershipVerdict::Allowed;
    }
    match record.overwrite_policy {
        OverwritePolicy::Forbidden => OwnershipVerdict::ForbiddenByPolicy,
        OverwritePolicy::ReplaceOwned | OverwritePolicy::Append => {
            if writer == record.owner || record.permitted_mutators.iter().any(|m| m == writer) {
                OwnershipVerdict::Allowed
            } else {
                OwnershipVerdict::NotAMutator
            }
        }
        OverwritePolicy::Merge => OwnershipVerdict::Allowed,
        OverwritePolicy::DerivedOnly => OwnershipVerdict::ForbiddenByPolicy,
        OverwritePolicy::ExplicitTransfer => {
            if transfer_recorded {
                OwnershipVerdict::Allowed
            } else {
                OwnershipVerdict::TransferRequired
            }
        }
    }
}

/// The content digest of an artefact revision — evidence binds to bytes,
/// not paths.
pub fn revision_digest(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}
