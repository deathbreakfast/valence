//! Purpose newtype carried into `*_used` APIs.

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
