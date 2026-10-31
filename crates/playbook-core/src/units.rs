use serde::{Deserialize, Serialize};

use crate::error::PlaybookError;

/// Parts per million. `1_000_000` is certainty, matching Lastdown probabilities.
pub const PPM_SCALE: u32 = 1_000_000;

/// Nominal full-field yards-to-go, in milli-yards. Matches Lastdown's reference.
pub const YTG_REFERENCE_MILLI: u32 = 10_000;

pub const SCHEMA_VERSION: u32 = 1;
pub const SCHEMA_MAGIC: &str = "PLAYBOOK/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ppm(u32);

impl Ppm {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(PPM_SCALE);

    pub fn new(value: u32) -> Result<Self, PlaybookError> {
        if value > PPM_SCALE {
            return Err(PlaybookError::Probability(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u32 {
        self.0
    }
}

/// Lastdown's scale: zero stays zero, every other product is at least one.
pub fn scale_by_probability(ppm: u32, value: u64) -> u64 {
    if ppm == 0 || value == 0 {
        0
    } else {
        (u64::from(ppm) * value / u64::from(PPM_SCALE)).max(1)
    }
}

pub fn accept_schema(version: u32) -> Result<(), PlaybookError> {
    if version == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(PlaybookError::Schema(version))
    }
}

pub fn ppm_percent(ppm: u32) -> String {
    let ppm = ppm.min(PPM_SCALE);
    let whole = ppm / 10_000;
    let frac = (ppm % 10_000) / 100;
    if ppm % 10_000 == 0 {
        format!("{whole}%")
    } else {
        format!("{whole}.{frac:02}%")
    }
}
