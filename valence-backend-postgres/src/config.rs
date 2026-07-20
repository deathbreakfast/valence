//! Postgres connection configuration and builder.

use valence_core::error::{Error, Result};

/// Environment variable for Postgres connection URL.
pub const URL_ENV: &str = "DATABASE_URL";

/// Builder for [`super::backend::PostgresBackend`].
#[derive(Debug, Clone, Default)]
pub struct PostgresBackendBuilder {
    url: Option<String>,
}

impl PostgresBackendBuilder {
    /// Empty builder; set fields or call [`Self::from_env_defaults`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill unset URL from `DATABASE_URL`.
    #[must_use]
    pub fn from_env_defaults(mut self) -> Self {
        if self.url.is_none() {
            self.url = postgres_url_from_env();
        }
        self
    }

    /// Postgres connection URL.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Whether a URL is set explicitly or resolvable via env defaults.
    pub fn has_url(&self) -> bool {
        self.url.is_some() || postgres_url_from_env().is_some()
    }

    /// Resolve URL without connecting.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when no URL is set and `DATABASE_URL` is unset or empty.
    pub fn resolve(self) -> Result<String> {
        let builder = self.from_env_defaults();
        builder
            .url
            .ok_or_else(|| Error::Internal(format!("{URL_ENV} not set for postgres adapter")))
    }

    /// Connect and return a [`super::backend::PostgresBackend`].
    ///
    /// # Errors
    ///
    /// Returns an error if URL resolution or the database connection fails.
    pub async fn build(self) -> Result<super::backend::PostgresBackend> {
        let url = self.resolve()?;
        super::backend::PostgresBackend::connect(&url).await
    }
}

fn postgres_url_from_env() -> Option<String> {
    std::env::var(URL_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
