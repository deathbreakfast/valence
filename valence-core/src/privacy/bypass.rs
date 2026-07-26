//! Dual-key privacy bypass seam (bench/test only — never enable in production).

use std::sync::Once;

/// Env var requesting privacy bypass (`1` to request).
pub const PRIVACY_BYPASS_ENV: &str = "VALENCE_PRIVACY_BYPASS";

/// Second key required with [`PRIVACY_BYPASS_ENV`] before bypass is honored.
pub const PRIVACY_BYPASS_FORCE_ON_ENV: &str = "VALENCE_PRIVACY_BYPASS_FORCE_ON";

fn env_flag_on(name: &str) -> bool {
    matches!(
        std::env::var(name).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

/// True when privacy evaluation is skipped (both bypass env keys set).
///
/// Requires `VALENCE_PRIVACY_BYPASS=1` **and** `VALENCE_PRIVACY_BYPASS_FORCE_ON=1`.
/// Never set either in production hosts.
#[must_use]
pub fn privacy_bypass_active() -> bool {
    let requested = env_flag_on(PRIVACY_BYPASS_ENV);
    if !requested {
        return false;
    }
    let force = env_flag_on(PRIVACY_BYPASS_FORCE_ON_ENV);
    if force {
        static WARN_ACTIVE: Once = Once::new();
        WARN_ACTIVE.call_once(|| {
            tracing::warn!(
                VALENCE_PRIVACY_BYPASS = "1",
                VALENCE_PRIVACY_BYPASS_FORCE_ON = "1",
                "privacy bypass active — entity privacy checks skipped (bench/test only; never in production)"
            );
        });
        true
    } else {
        static WARN_IGNORED: Once = Once::new();
        WARN_IGNORED.call_once(|| {
            tracing::warn!(
                VALENCE_PRIVACY_BYPASS = "requested",
                VALENCE_PRIVACY_BYPASS_FORCE_ON = "missing",
                "VALENCE_PRIVACY_BYPASS ignored without VALENCE_PRIVACY_BYPASS_FORCE_ON=1; privacy stays enforced"
            );
        });
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypass_requires_both_keys() {
        std::env::remove_var(PRIVACY_BYPASS_ENV);
        std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
        assert!(!privacy_bypass_active());

        std::env::set_var(PRIVACY_BYPASS_ENV, "1");
        std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
        assert!(!privacy_bypass_active());

        std::env::set_var(PRIVACY_BYPASS_ENV, "1");
        std::env::set_var(PRIVACY_BYPASS_FORCE_ON_ENV, "1");
        assert!(privacy_bypass_active());

        std::env::remove_var(PRIVACY_BYPASS_ENV);
        std::env::remove_var(PRIVACY_BYPASS_FORCE_ON_ENV);
    }
}
