//! MongoDB connection configuration and builder.

use valence_core::error::{Error, Result};

/// Environment variable for MongoDB connection URI.
pub const URI_ENV: &str = "VALENCE_MONGODB_URI";

/// Fallback test URI env var.
pub const TEST_URI_ENV: &str = "VALENCE_TEST_MONGODB_URI";

/// Database name env var.
pub const DATABASE_ENV: &str = "VALENCE_MONGODB_DB";

const DEFAULT_DATABASE: &str = "valence";

/// Connection settings for a MongoDB-backed Valence adapter.
#[derive(Debug, Clone)]
pub struct MongoConfig {
    /// MongoDB connection URI.
    pub uri: String,
    /// Database name for Valence collections.
    pub database: String,
}

impl MongoConfig {
    /// Build config from explicit URI and database name.
    pub fn new(uri: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            database: database.into(),
        }
    }
}

/// Builder for [`super::backend::MongoBackend`].
#[derive(Debug, Clone, Default)]
pub struct MongoBackendBuilder {
    uri: Option<String>,
    database: Option<String>,
}

impl MongoBackendBuilder {
    /// Empty builder; set fields or call [`Self::from_env_defaults`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill unset fields from `VALENCE_MONGODB_*` environment variables.
    #[must_use]
    pub fn from_env_defaults(mut self) -> Self {
        if self.uri.is_none() {
            self.uri = mongodb_uri_from_env();
        }
        if self.database.is_none() {
            self.database = std::env::var(DATABASE_ENV)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| Some(DEFAULT_DATABASE.to_string()));
        }
        self
    }

    /// MongoDB connection URI.
    #[must_use]
    pub fn uri(mut self, uri: impl Into<String>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    /// Database name for Valence collections.
    #[must_use]
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// Whether a URI is set explicitly or resolvable via env defaults.
    pub fn has_uri(&self) -> bool {
        self.uri.is_some() || mongodb_uri_from_env().is_some()
    }

    /// Resolve configuration without connecting.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when no MongoDB URI is configured.
    pub fn resolve(self) -> Result<MongoConfig> {
        let builder = self.from_env_defaults();
        let uri = builder.uri.ok_or_else(|| {
            Error::Internal(format!(
                "{URI_ENV} or {TEST_URI_ENV} not set for mongodb adapter"
            ))
        })?;
        Ok(MongoConfig {
            uri,
            database: builder
                .database
                .unwrap_or_else(|| DEFAULT_DATABASE.to_string()),
        })
    }

    /// Connect and return a [`super::backend::MongoBackend`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when config is incomplete, or [`Error::Database`] on connect failure.
    pub async fn build(self) -> Result<super::backend::MongoBackend> {
        let config = self.resolve()?;
        super::backend::MongoBackend::connect_with_config(config).await
    }
}

fn mongodb_uri_from_env() -> Option<String> {
    std::env::var(URI_ENV)
        .ok()
        .or_else(|| std::env::var(TEST_URI_ENV).ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
