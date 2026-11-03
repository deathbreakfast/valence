//! Schema opt-in for periodic Spectra partition export: cadence enum and resolved config.
//!
//! # Owns
//!
//! - [`SpectraExportCadence`] and the platform-owned cron string for each variant
//! - [`SpectraExportConfig`], carried on `Schema.spectra_export`
//!
//! # Does not own
//!
//! Chronon job registration and the export executor. Valence has no Spectra or Chronon
//! dependency; those crates read this metadata from the schema registry.
//!
//! # Concern → API
//!
//! | Concern | API |
//! |---------|-----|
//! | Opt a table in | `spectra_export: { cadence, job_name?, target_table? }` in `valence_schema!` |
//! | Read the config | `Schema.spectra_export` ([`SpectraExportConfig`]) |
//! | Resolve the schedule | [`SpectraExportCadence::cron`] |

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Fixed export cadence. Schema authors choose a variant; they never write cron strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpectraExportCadence {
    /// Previous hour bucket, every hour.
    Hourly,
    /// Previous 3-hour bucket, every 3 hours.
    ThreeHours,
    /// Previous 6-hour bucket, every 6 hours.
    SixHours,
    /// Previous 12-hour bucket, every 12 hours.
    TwelveHours,
    /// Previous calendar day, once per day.
    Daily,
    /// Previous calendar week, once per week.
    Weekly,
}

impl SpectraExportCadence {
    /// Every variant, in order of increasing period.
    pub const ALL: [Self; 6] = [
        Self::Hourly,
        Self::ThreeHours,
        Self::SixHours,
        Self::TwelveHours,
        Self::Daily,
        Self::Weekly,
    ];

    /// Platform-owned Chronon cron for this cadence (UTC). Operators cannot override it per schema.
    ///
    /// `Weekly` fires Sunday 00:00 UTC.
    #[must_use]
    pub const fn cron(self) -> &'static str {
        match self {
            Self::Hourly => "0 * * * *",
            Self::ThreeHours => "0 */3 * * *",
            Self::SixHours => "0 */6 * * *",
            Self::TwelveHours => "0 */12 * * *",
            Self::Daily => "0 0 * * *",
            Self::Weekly => "0 0 * * 0",
        }
    }

    /// Variant name as written in the DSL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hourly => "Hourly",
            Self::ThreeHours => "ThreeHours",
            Self::SixHours => "SixHours",
            Self::TwelveHours => "TwelveHours",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
        }
    }
}

impl fmt::Display for SpectraExportCadence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SpectraExportCadence {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == s)
            .ok_or_else(|| Error::Validation(format!("unknown spectra_export cadence: {s:?}")))
    }
}

/// Resolved `spectra_export` opt-in for one schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpectraExportConfig {
    pub cadence: SpectraExportCadence,
    /// Chronon job name; defaults to `{table}_{cadence}_export`.
    pub job_name: String,
    /// Spectra table receiving partitions; defaults to `{table}_export`.
    pub target_table: String,
}

impl SpectraExportConfig {
    /// Platform cron derived from [`Self::cadence`].
    #[must_use]
    pub const fn cron(&self) -> &'static str {
        self.cadence.cron()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_mapping_is_platform_owned() {
        let got: Vec<_> = SpectraExportCadence::ALL.iter().map(|c| c.cron()).collect();
        assert_eq!(
            got,
            [
                "0 * * * *",
                "0 */3 * * *",
                "0 */6 * * *",
                "0 */12 * * *",
                "0 0 * * *",
                "0 0 * * 0"
            ]
        );
    }

    #[test]
    fn name_round_trip_and_unknown() {
        for c in SpectraExportCadence::ALL {
            assert_eq!(c.as_str().parse::<SpectraExportCadence>().unwrap(), c);
        }
        assert!("0 0 * * *".parse::<SpectraExportCadence>().is_err());
        assert!("daily".parse::<SpectraExportCadence>().is_err());
    }

    #[test]
    fn config_serde_round_trip() {
        let cfg = SpectraExportConfig {
            cadence: SpectraExportCadence::SixHours,
            job_name: "j".into(),
            target_table: "t".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert_eq!(
            serde_json::from_str::<SpectraExportConfig>(&json).unwrap(),
            cfg
        );
        assert_eq!(cfg.cron(), "0 */6 * * *");
    }
}
