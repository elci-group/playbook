//! Live personnel identity.
//!
//! Each live identity represents a stable empirical unit. A change of
//! provider, model, major profile, or materially different tool envelope
//! produces a distinct identity, because folding
//! `provider-A/model-X` and `provider-B/model-X` into one player would
//! compare incomparable performance histories.

use serde::{Deserialize, Serialize};

use playbook_core::{PersonnelId, PlaybookError};

/// The stable empirical identity of one live worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveIdentity {
    pub provider: String,
    pub model: String,
    /// The major system-profile version. A materially different profile is a
    /// different worker even on the same provider and model.
    pub profile_version: String,
    /// The tool envelope this worker operates with.
    pub tool_envelope: String,
}

fn slug(part: &str) -> Result<String, PlaybookError> {
    let cleaned: String = part
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let collapsed = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if collapsed.is_empty() {
        return Err(PlaybookError::InvalidId);
    }
    Ok(collapsed)
}

impl LiveIdentity {
    pub fn new(
        provider: &str,
        model: &str,
        profile_version: &str,
        tool_envelope: &str,
    ) -> Result<Self, PlaybookError> {
        for part in [provider, model, profile_version, tool_envelope] {
            if part.trim().is_empty() {
                return Err(PlaybookError::InvalidId);
            }
        }
        Ok(Self {
            provider: provider.into(),
            model: model.into(),
            profile_version: profile_version.into(),
            tool_envelope: tool_envelope.into(),
        })
    }

    /// The deterministic personnel id. The same identity always maps to the
    /// same id, so empirical records accumulate on the right player.
    pub fn personnel_id(&self) -> Result<PersonnelId, PlaybookError> {
        let id = PersonnelId::new(format!(
            "live-{}-{}-{}-{}",
            slug(&self.provider)?,
            slug(&self.model)?,
            slug(&self.profile_version)?,
            slug(&self.tool_envelope)?
        ))?;
        Ok(id)
    }

    /// True when the two identities are empirically comparable: same
    /// provider, same model, same profile, same envelope.
    pub fn comparable(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.model == other.model
            && self.profile_version == other.profile_version
            && self.tool_envelope == other.tool_envelope
    }
}

/// Whether a worker may take new work. A worker with an unresolved
/// invocation is busy: reassignment is prohibited by default because the
/// effect of the outstanding invocation is still unknown. Reconciliation
/// comes first, always.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occupancy {
    /// The worker has an invocation whose outcome is not yet reconciled.
    pub has_unresolved_invocation: bool,
    /// The capability explicitly supports safe concurrent occupancy.
    pub supports_concurrent_occupancy: bool,
    /// Policy permits reassignment of busy personnel for this play.
    pub policy_permits_reassignment: bool,
}

impl Occupancy {
    /// Default policy: unresolved invocation → player busy → no reassignment.
    pub fn may_take_work(&self) -> bool {
        if !self.has_unresolved_invocation {
            return true;
        }
        self.supports_concurrent_occupancy && self.policy_permits_reassignment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_changes_produce_distinct_players() {
        let base = LiveIdentity::new("alpha", "model-x", "v1", "shell").unwrap();
        let same = LiveIdentity::new("alpha", "model-x", "v1", "shell").unwrap();
        assert_eq!(base.personnel_id().unwrap(), same.personnel_id().unwrap());
        assert!(base.comparable(&same));

        // Same model on another provider is NOT the same player.
        let other_provider = LiveIdentity::new("beta", "model-x", "v1", "shell").unwrap();
        assert_ne!(base.personnel_id().unwrap(), other_provider.personnel_id().unwrap());
        assert!(!base.comparable(&other_provider));

        // A major profile bump is a different worker.
        let new_profile = LiveIdentity::new("alpha", "model-x", "v2", "shell").unwrap();
        assert_ne!(base.personnel_id().unwrap(), new_profile.personnel_id().unwrap());

        // A materially different tool envelope is a different worker.
        let new_tools = LiveIdentity::new("alpha", "model-x", "v1", "shell+network").unwrap();
        assert_ne!(base.personnel_id().unwrap(), new_tools.personnel_id().unwrap());
    }

    #[test]
    fn ids_are_slugified_and_deterministic() {
        let identity = LiveIdentity::new("Provider A", "model.x/v1", "v1", "read only").unwrap();
        let id = identity.personnel_id().unwrap();
        assert_eq!(id.as_str(), "live-provider-a-model-x-v1-v1-read-only");
        assert_eq!(
            id,
            LiveIdentity::new("Provider A", "model.x/v1", "v1", "read only")
                .unwrap()
                .personnel_id()
                .unwrap()
        );
        assert!(LiveIdentity::new("", "m", "v1", "t").is_err());
    }

    #[test]
    fn busy_players_do_not_take_work_unless_concurrency_is_explicit() {
        let free = Occupancy {
            has_unresolved_invocation: false,
            supports_concurrent_occupancy: false,
            policy_permits_reassignment: false,
        };
        assert!(free.may_take_work());

        let busy = Occupancy {
            has_unresolved_invocation: true,
            ..free
        };
        assert!(!busy.may_take_work());

        let concurrent = Occupancy {
            supports_concurrent_occupancy: true,
            policy_permits_reassignment: true,
            ..busy
        };
        assert!(concurrent.may_take_work());

        // Either guard missing keeps the default prohibition.
        assert!(!Occupancy {
            policy_permits_reassignment: false,
            ..concurrent
        }
        .may_take_work());
        assert!(!Occupancy {
            supports_concurrent_occupancy: false,
            ..concurrent
        }
        .may_take_work());
    }
}
