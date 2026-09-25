//! Purpose newtype carried into declared Model / Query APIs.

/// Markdown purpose string plus call-site `file!` / `line!` from `use_!`.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub struct DataUsePurpose {
    purpose: &'static str,
    file: &'static str,
    line: u32,
}

impl DataUsePurpose {
    /// Build a purpose from a static markdown string and call-site location.
    ///
    /// Prefer `use_!` instead of calling this directly.
    #[doc(hidden)]
    pub const fn new(purpose: &'static str, file: &'static str, line: u32) -> Self {
        Self {
            purpose,
            file,
            line,
        }
    }

    /// Nested framework load when the public entry already declared a purpose.
    ///
    /// Not a product catalog row — used only inside Valence / generated Model bodies.
    #[doc(hidden)]
    pub const fn framework_nested() -> Self {
        Self::new(
            "Valence framework nested access; purpose was declared at the public entry point.",
            "<valence-framework>",
            0,
        )
    }

    /// End-user trust copy (markdown).
    #[must_use]
    pub const fn purpose(self) -> &'static str {
        self.purpose
    }

    /// Source file path from `file!()`.
    #[must_use]
    pub const fn file(self) -> &'static str {
        self.file
    }

    /// Source line from `line!()`.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }
}
