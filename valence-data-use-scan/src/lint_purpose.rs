//! Purpose-string quality lint for Valence `use_!` trust copy.
//!
//! Tier-aware checks: ban migration templates ([`TEMPLATE_PHRASES`]) and hedges
//! ([`HEDGE_PHRASES`]), catalog duplication, thin/jargon purposes, and require
//! `**Test:**` (or `### Test`) for harness paths.
//!
//! Template and hedge phrases are matched after [`phrase_text`] normalization:
//! lowercase, markdown `*` dropped, curly apostrophes straightened, whitespace
//! collapsed. Every other check sees the same text it always did.

use std::fmt;

/// Sensitivity / audience class for a purpose string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurposeTier {
    /// Personal, financial, or secrets — most detail expected.
    S3,
    /// User-adjacent / collaboration.
    S2,
    /// System / control-plane.
    S1,
    /// Tests, e2e, bench, testkit, lab fixtures.
    S0,
}

impl PurposeTier {
    /// Infer a default tier from a source path (best-effort).
    #[must_use]
    pub fn from_path(path: &str) -> Self {
        let p = path.replace('\\', "/").to_ascii_lowercase();
        if p.contains("/tests/")
            || p.contains("_test.rs")
            || p.contains("-e2e/")
            || p.contains("-testkit/")
            || p.contains("-bench/")
            || p.contains("testkit")
            || p.contains("/bench/")
            || p.contains("campaign-ops")
            || p.contains("-lab/")
        {
            // Teaching prod fixtures under tests/fixtures/prod* stay S3.
            if p.contains("fixtures/prod") {
                return Self::S3;
            }
            return Self::S0;
        }
        if p.contains("lepton-auth")
            || p.contains("lepton-identity")
            || p.contains("lepton-app")
            || p.contains("/totp/")
            || p.contains("neutrino")
            || p.contains("/finance/")
            || p.contains("uf-notifications")
            || p.contains("uf-ocr")
            || p.contains("sealed_store")
            || p.contains("/profile")
        {
            return Self::S3;
        }
        if p.contains("polaron") || p.contains("/gauge/") || p.contains("meson") {
            return Self::S2;
        }
        Self::S1
    }
}

/// Migration-template sentence fragments, matched after [`phrase_text`] normalization.
///
/// A 2026 migration stamped these generic sentences onto production `use_!`
/// call sites. They say nothing about what the code does with the data.
pub const TEMPLATE_PHRASES: &[&str] = &[
    "valence persistence for this feature path",
    "mutable handle for in-place update",
    "decide what to do next in this workflow",
    "show or process the matching set for this workflow",
    "so the next step in that feature can continue with the latest values",
    "so later steps see the latest values for this workflow",
    "is not shown as a standalone end-user page by itself",
    "so saved changes apply on the next read",
];

/// Hedges that say the author did not check how the data is used.
pub const HEDGE_PHRASES: &[&str] = &["not necessarily", "unless that feature"];

/// Normalize a purpose for template and hedge matching: lowercase, drop `*`,
/// straighten `’` to `'`, collapse whitespace.
#[must_use]
pub fn phrase_text(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .filter(|c| *c != '*')
        .map(|c| if c == '\u{2019}' { '\'' } else { c })
        .collect();
    cleaned
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every template and hedge phrase found in `purpose`, in list order.
#[must_use]
pub fn matching_phrases(purpose: &str) -> Vec<&'static str> {
    let text = phrase_text(purpose);
    TEMPLATE_PHRASES
        .iter()
        .chain(HEDGE_PHRASES)
        .copied()
        .filter(|p| text.contains(p))
        .collect()
}

fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|p| text.contains(p))
}

/// Machine-stable gap codes for inventory / CI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GapCode {
    /// Empty or whitespace-only purpose.
    Empty,
    /// Migration template phrase (see [`TEMPLATE_PHRASES`]).
    ///
    /// The sentence was stamped onto many call sites and says nothing about this
    /// one. Follow the result to its consumer and write what data is read or
    /// written, why, and which product feature serves it, in plain words.
    BanTemplate,
    /// Hedge such as "not necessarily displayed … unless that feature's UI shows it"
    /// (see [`HEDGE_PHRASES`]).
    ///
    /// A hedge means the author did not check how the data is used, and a purpose
    /// that may contradict the code is false trust copy. Find the consumer (server
    /// function, view, task, branch) and state which screen shows the data and to
    /// whom, or which decision it drives.
    Hedge,
    /// `Type in path.rs` / leading get/query/create duplication.
    BanPath,
    /// Ritual “not copied outside Valence” negatives.
    BanRitualNegative,
    /// Session-actor / typed-store jargon from migration.
    BanJargon,
    /// Too short / no stranger story for the tier.
    TooThin,
    /// Code-context tokens without product setup.
    CodeContext,
    /// S0 missing `**Test:**` / `### Test`.
    TestPrefix,
    /// Claims user “sees” without display language — heuristic soft; reserved.
    FakeDisplay,
}

impl GapCode {
    /// Stable snake token for inventory columns.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "EMPTY",
            Self::BanTemplate => "BAN_TEMPLATE",
            Self::Hedge => "HEDGE",
            Self::BanPath => "BAN_PATH",
            Self::BanRitualNegative => "BAN_RITUAL_NEGATIVE",
            Self::BanJargon => "BAN_JARGON",
            Self::TooThin => "TOO_THIN",
            Self::CodeContext => "CODE_CONTEXT",
            Self::TestPrefix => "TEST_PREFIX",
            Self::FakeDisplay => "FAKE_DISPLAY",
        }
    }
}

impl GapCode {
    /// `BanTemplate` or `Hedge`: the codes the purpose rewrite counts.
    #[must_use]
    pub const fn is_template_or_hedge(self) -> bool {
        matches!(self, Self::BanTemplate | Self::Hedge)
    }
}

impl fmt::Display for GapCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lint a purpose string for the given tier. Empty `gaps` means Pass.
#[must_use]
pub fn lint_purpose(purpose: &str, tier: PurposeTier) -> Vec<GapCode> {
    let mut gaps = Vec::new();
    let trimmed = purpose.trim();
    if trimmed.is_empty() {
        gaps.push(GapCode::Empty);
        return gaps;
    }

    let lower = trimmed.to_ascii_lowercase();

    let phrases = phrase_text(trimmed);
    if contains_any(&phrases, TEMPLATE_PHRASES) {
        gaps.push(GapCode::BanTemplate);
    }
    if contains_any(&phrases, HEDGE_PHRASES) {
        gaps.push(GapCode::Hedge);
    }

    if lower.contains("typed store")
        || lower.contains("session actor")
        || lower.contains("session/service path")
        || lower.contains("visible to session actor")
    {
        gaps.push(GapCode::BanJargon);
    }

    if lower.contains("not copied outside valence")
        || lower.contains("not copied elsewhere")
        || (lower.contains("typed store only") && lower.contains("not copied"))
    {
        gaps.push(GapCode::BanRitualNegative);
    }

    // `get Foo in src/...` / `query Bar in path`
    if looks_like_path_op_duplication(trimmed) {
        gaps.push(GapCode::BanPath);
    }

    if contains_code_context(&lower) {
        gaps.push(GapCode::CodeContext);
    }

    match tier {
        PurposeTier::S0 => {
            if !(trimmed.starts_with("**Test:**")
                || trimmed.starts_with("**Test**:")
                || trimmed.starts_with("### Test"))
            {
                gaps.push(GapCode::TestPrefix);
            }
            if word_count(trimmed) < 8 {
                gaps.push(GapCode::TooThin);
            }
        }
        PurposeTier::S1 => {
            if word_count(trimmed) < 18 {
                gaps.push(GapCode::TooThin);
            }
        }
        PurposeTier::S2 => {
            if word_count(trimmed) < 22 {
                gaps.push(GapCode::TooThin);
            }
        }
        PurposeTier::S3 => {
            if word_count(trimmed) < 28 {
                gaps.push(GapCode::TooThin);
            }
        }
    }

    gaps.sort_by_key(|g| g.as_str());
    gaps.dedup();
    gaps
}

/// True when lint finds no gaps.
#[must_use]
pub fn purpose_passes(purpose: &str, tier: PurposeTier) -> bool {
    lint_purpose(purpose, tier).is_empty()
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

fn looks_like_path_op_duplication(s: &str) -> bool {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let ops = [
        "get ",
        "query ",
        "create ",
        "upsert ",
        "update ",
        "delete ",
        "delete_now ",
        "get_mutable ",
    ];
    let starts_op = ops.iter().any(|op| lower.starts_with(op));
    if starts_op && (lower.contains(" in src/") || lower.contains(" in ") && lower.contains(".rs"))
    {
        return true;
    }
    if lower.starts_with("get_mutable via ") {
        return true;
    }
    // `Foo in path/file.rs;` early in string
    if let Some(idx) = lower.find(" in ") {
        if idx < 48 {
            let after = &lower[idx..];
            if after.contains(".rs") || after.contains("src/") {
                return true;
            }
        }
    }
    false
}

fn contains_code_context(lower: &str) -> bool {
    const TOKENS: &[&str] = &[
        "gate enroll",
        "pending factor",
        "pending row",
        "discard handle",
        "lazy-create",
        "vault put",
        "querycore",
        "get_mutable",
        "delete_now",
    ];
    TOKENS.iter().any(|t| lower.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ban_template_sad() {
        let p = "get User in src/totp/api.rs; Valence persistence for this feature path; typed store; visible to session actor / service path.";
        let gaps = lint_purpose(p, PurposeTier::S3);
        assert!(gaps.contains(&GapCode::BanTemplate));
        assert!(gaps.contains(&GapCode::BanPath));
        assert!(gaps.contains(&GapCode::BanJargon));
    }

    #[test]
    fn ritual_negative_sad() {
        let p = "We load your profile for the account page. Not copied outside Valence for this step. You can edit the display name.";
        let gaps = lint_purpose(p, PurposeTier::S3);
        assert!(gaps.contains(&GapCode::BanRitualNegative));
    }

    #[test]
    fn enroll_gold_happy() {
        let p = "Before we begin **enrollment** on setting up your **authenticator**, we first verify the user account **exists** by **loading it with the provided id**. **No other information** is required, so we discard it immediately.";
        assert!(
            purpose_passes(p, PurposeTier::S3),
            "{:?}",
            lint_purpose(p, PurposeTier::S3)
        );
    }

    #[test]
    fn s1_thin_sad() {
        let gaps = lint_purpose("Acme application for reconcile.", PurposeTier::S1);
        assert!(gaps.contains(&GapCode::TooThin));
    }

    #[test]
    fn s1_acme_create_happy() {
        let p = "When an operator **creates an Acme application**, we **save its name, slug, container image, health and load-balancer settings, route prefix, and desired instance count** so **reconcile** can start the right containers and **HAProxy** can route traffic. Acme operators who manage that app use this record on the applications console—it is not end-user profile data.";
        assert!(
            purpose_passes(p, PurposeTier::S1),
            "{:?}",
            lint_purpose(p, PurposeTier::S1)
        );
    }

    #[test]
    fn s0_requires_test_prefix() {
        let gaps = lint_purpose(
            "Builds a minimal user fixture for the account wipe suite and asserts removal.",
            PurposeTier::S0,
        );
        assert!(gaps.contains(&GapCode::TestPrefix));
        let ok = "**Test:** Builds a minimal **user** fixture for lepton-auth `tests/account_wipe`, then asserts wipe removed it. CI and developers running the suite only.";
        assert!(
            purpose_passes(ok, PurposeTier::S0),
            "{:?}",
            lint_purpose(ok, PurposeTier::S0)
        );
    }

    #[test]
    fn code_context_factor_sad() {
        let p = "While you set up your authenticator, we save a pending factor for your account including a sealed copy of the authenticator secret so enrollment can finish after you confirm with a code from the app.";
        let gaps = lint_purpose(p, PurposeTier::S3);
        assert!(gaps.contains(&GapCode::CodeContext));
    }

    #[test]
    fn tier_from_path() {
        assert_eq!(
            PurposeTier::from_path("lepton-auth/src/totp/api.rs"),
            PurposeTier::S3
        );
        assert_eq!(
            PurposeTier::from_path("acme/src/applications/service.rs"),
            PurposeTier::S1
        );
        assert_eq!(
            PurposeTier::from_path("lepton-auth/tests/account_wipe.rs"),
            PurposeTier::S0
        );
    }
}
