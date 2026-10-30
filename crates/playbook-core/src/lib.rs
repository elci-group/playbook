//! Domain primitives for Playbook.
//!
//! Machine identifiers live here. Football words are presentation metadata.
//! No function in this crate calls a model.

#![forbid(unsafe_code)]

pub mod canonical;
pub mod contract;
pub mod domain;
pub mod error;
pub mod ids;
pub mod priors;
pub mod units;

pub use canonical::{canonical_bytes, canonical_value, sha256_hex, to_canonical_bytes};
pub use contract::*;
pub use domain::*;
pub use error::PlaybookError;
pub use ids::*;
pub use priors::{export_priors, import_priors, HistoricalPriorTable, PriorBundle, PriorRow};
pub use units::*;
