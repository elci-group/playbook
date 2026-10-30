use serde::{Deserialize, Serialize};

use crate::error::PlaybookError;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, PlaybookError> {
                let value = value.into();
                if value.is_empty() || value.len() > 200 {
                    return Err(PlaybookError::InvalidId);
                }
                let ok = value.chars().all(|c| {
                    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ':'
                });
                if !ok {
                    return Err(PlaybookError::InvalidId);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(DriveId);
id_type!(FranchiseId);
id_type!(ObjectiveId);
id_type!(GateId);
id_type!(PlayId);
id_type!(CapabilityId);
id_type!(PersonnelId);
id_type!(EvidenceId);
id_type!(ArtefactId);
id_type!(HuddleId);
id_type!(GameId);
id_type!(WinConditionId);
id_type!(DirectiveId);
id_type!(BlitzId);
id_type!(WorkerId);
id_type!(CandidateId);
id_type!(EventId);
