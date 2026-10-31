//! §15 version provenance: the record of the instrument.
//!
//! Replay is science only when the instrument is recorded. A drive that
//! journals its plays but not the toolchain that produced them is an
//! anecdote; with the toolchain recorded, it becomes an experiment another
//! team can reproduce. Every Franchise game therefore stamps itself with
//! the version — and, where available, the commit — of every integrated
//! system: Playbook, Mesut, Hybreed, Emulsify, Theosis, Dreamseq, and
//! Reece.
//!
//! The constitutional line, enforced by the types: provenance is
//! descriptive evidence and never authority. A [`ToolchainProvenance`]
//! records what ran; it cannot change what runs. The recorded systems are
//! fixed in [`KNOWN_SYSTEMS`] order, and a game without a version for
//! every one of them is refused — replay meaning collapses when a system
//! is anonymous.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use playbook_core::PlaybookError;

/// The integrated systems whose versions are recorded, in the fixed order
/// provenance is always written and digested.
pub const KNOWN_SYSTEMS: &[&str] = &[
    "playbook", "mesut", "hybreed", "emulsify", "theosis", "dreamseq", "reece",
];

/// The owned wire form of a [`SystemVersion`]. The public record keeps its
/// `system` name as a genuine `&'static str` (matched against the
/// [`KNOWN_SYSTEMS`] table, never allocated), so deserialisation goes
/// through this owned shadow and converts — refusing unknown fields and
/// unknown system names on the way.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SystemVersionRecord {
    system: String,
    version: String,
    commit: Option<String>,
}

/// The recorded version of one integrated system. The version string is
/// mandatory; the commit identifier is recorded when the toolchain can
/// supply one. Deserialisation refuses unknown fields and a system name
/// outside [`KNOWN_SYSTEMS`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SystemVersion {
    pub system: &'static str,
    pub version: String,
    pub commit: Option<String>,
}

impl<'de> Deserialize<'de> for SystemVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let record = SystemVersionRecord::deserialize(deserializer)?;
        let system = KNOWN_SYSTEMS
            .iter()
            .copied()
            .find(|known| *known == record.system.as_str())
            .ok_or_else(|| {
                serde::de::Error::custom(format!("unknown system: {}", record.system))
            })?;
        Ok(Self {
            system,
            version: record.version,
            commit: record.commit,
        })
    }
}

/// The full toolchain record for one Franchise game: exactly the seven
/// known systems, always in [`KNOWN_SYSTEMS`] order. Two games produced by
/// the same toolchain carry the same [`ToolchainProvenance::digest`]; any
/// version or commit difference separates them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainProvenance {
    pub systems: Vec<SystemVersion>,
}

impl ToolchainProvenance {
    /// Build from version strings (commits optional). The systems vec is
    /// always the full [`KNOWN_SYSTEMS`] order; a missing version is an
    /// error ("provenance requires a version for every known system") —
    /// replay meaning collapses when a system is anonymous. A version
    /// supplied for a system outside the known set is likewise refused:
    /// provenance silently dropping a record would be an anonymous
    /// instrument by another name.
    pub fn new(
        versions: &BTreeMap<&'static str, (String, Option<String>)>,
    ) -> Result<Self, PlaybookError> {
        for system in versions.keys() {
            if !KNOWN_SYSTEMS.contains(system) {
                return Err(PlaybookError::Evidence(format!(
                    "provenance names an unknown system: {system}"
                )));
            }
        }
        let mut systems = Vec::with_capacity(KNOWN_SYSTEMS.len());
        for &system in KNOWN_SYSTEMS {
            let Some((version, commit)) = versions.get(&system) else {
                return Err(PlaybookError::Evidence(format!(
                    "provenance requires a version for every known system: {system} is missing"
                )));
            };
            systems.push(SystemVersion {
                system,
                version: version.clone(),
                commit: commit.clone(),
            });
        }
        Ok(Self { systems })
    }

    /// The provenance digest: sha256 over the canonical serialisation
    /// (`playbook_core::to_canonical_bytes` + `sha256_hex`). Deterministic:
    /// the same toolchain always yields the same digest, and any version or
    /// commit difference changes it.
    pub fn digest(&self) -> Result<String, PlaybookError> {
        let bytes = playbook_core::to_canonical_bytes(self)?;
        Ok(playbook_core::sha256_hex(&bytes))
    }

    /// The version recorded for one system, by name.
    pub fn version_of(&self, system: &str) -> Option<&str> {
        self.systems
            .iter()
            .find(|record| record.system == system)
            .map(|record| record.version.as_str())
    }

    /// True when every commit field is `Some` — full commit-level
    /// provenance, the strongest form of instrument record.
    pub fn commit_complete(&self) -> bool {
        self.systems.iter().all(|record| record.commit.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_versions() -> BTreeMap<&'static str, (String, Option<String>)> {
        let mut versions = BTreeMap::new();
        versions.insert("playbook", ("2.1.0".into(), Some("9f1c02ab".into())));
        versions.insert("mesut", ("0.7.3".into(), Some("c4d5e6f7".into())));
        versions.insert("hybreed", ("1.4.0".into(), None));
        versions.insert("emulsify", ("0.9.1".into(), Some("1122334a".into())));
        versions.insert("theosis", ("0.5.0".into(), None));
        versions.insert("dreamseq", ("1.0.2".into(), Some("5566778b".into())));
        versions.insert("reece", ("0.3.9".into(), Some("ddeeff9c".into())));
        versions
    }

    #[test]
    fn full_provenance_is_recorded_in_known_systems_order() {
        let provenance = ToolchainProvenance::new(&full_versions()).unwrap();
        assert_eq!(provenance.systems.len(), KNOWN_SYSTEMS.len());
        for (record, name) in provenance.systems.iter().zip(KNOWN_SYSTEMS) {
            assert_eq!(record.system, *name);
        }
        assert_eq!(provenance.version_of("playbook"), Some("2.1.0"));
        assert_eq!(provenance.version_of("reece"), Some("0.3.9"));
        assert_eq!(provenance.version_of("griddle"), None);
    }

    #[test]
    fn a_missing_system_version_is_refused() {
        let mut versions = full_versions();
        versions.remove("dreamseq");
        let error = ToolchainProvenance::new(&versions).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("provenance requires a version for every known system"),
            "unexpected error: {error}"
        );
        let mut unknown = full_versions();
        unknown.insert("griddle", ("1.0.0".into(), None));
        assert!(ToolchainProvenance::new(&unknown).is_err());
    }

    #[test]
    fn the_digest_tracks_versions_and_commits() {
        let baseline = ToolchainProvenance::new(&full_versions()).unwrap();
        assert_eq!(baseline.digest().unwrap(), baseline.digest().unwrap());

        let mut version_bumped = full_versions();
        version_bumped.insert("reece", ("0.4.0".into(), Some("ddeeff9c".into())));
        assert_ne!(
            baseline.digest().unwrap(),
            ToolchainProvenance::new(&version_bumped)
                .unwrap()
                .digest()
                .unwrap()
        );

        let mut commit_bumped = full_versions();
        commit_bumped.insert("reece", ("0.3.9".into(), Some("000000ff".into())));
        assert_ne!(
            baseline.digest().unwrap(),
            ToolchainProvenance::new(&commit_bumped)
                .unwrap()
                .digest()
                .unwrap()
        );
    }

    #[test]
    fn commit_completeness_requires_every_commit() {
        let partial = ToolchainProvenance::new(&full_versions()).unwrap();
        assert!(!partial.commit_complete());
        let mut versions = full_versions();
        versions.insert("hybreed", ("1.4.0".into(), Some("aabb0011".into())));
        versions.insert("theosis", ("0.5.0".into(), Some("ccdd0022".into())));
        assert!(ToolchainProvenance::new(&versions)
            .unwrap()
            .commit_complete());
    }

    #[test]
    fn provenance_round_trips_and_refuses_unknown_fields() {
        let provenance = ToolchainProvenance::new(&full_versions()).unwrap();
        let json = serde_json::to_string(&provenance).unwrap();
        let round: ToolchainProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(round, provenance);

        let tampered =
            r#"{"systems":[{"system":"playbook","version":"2.1.0","commit":null,"spoofed":true}]}"#;
        assert!(serde_json::from_str::<ToolchainProvenance>(tampered).is_err());
        assert!(serde_json::from_str::<SystemVersion>(
            r#"{"system":"reece","version":"0.3.9","extra":1}"#
        )
        .is_err());
        assert!(serde_json::from_str::<SystemVersion>(
            r#"{"system":"griddle","version":"1.0.0","commit":null}"#
        )
        .is_err());
    }
}
