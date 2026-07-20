//! Redis connection configuration and builder.

use valence_core::error::{Error, Result};

/// Environment variable for Redis connection URL.
pub const URL_ENV: &str = "VALENCE_REDIS_URL";

/// Fallback test URL env var.
pub const TEST_URL_ENV: &str = "VALENCE_TEST_REDIS_URL";

/// Key prefix env var.
pub const KEY_PREFIX_ENV: &str = "VALENCE_REDIS_KEY_PREFIX";

/// Comma-separated fleet URLs env var.
pub const URLS_ENV: &str = "VALENCE_REDIS_URLS";

const DEFAULT_KEY_PREFIX: &str = "valence";

/// Connection settings for a Redis-backed Valence adapter.
#[derive(Debug, Clone)]
pub struct RedisConfig {
    /// Redis connection URL (`redis://…`).
    pub url: String,
    /// Key prefix for Valence namespacing.
    pub key_prefix: String,
}

impl RedisConfig {
    /// Build config from explicit URL and prefix.
    pub fn new(url: impl Into<String>, key_prefix: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            key_prefix: key_prefix.into(),
        }
    }
}

/// Builder for [`super::backend::RedisBackend`].
#[derive(Debug, Clone, Default)]
pub struct RedisBackendBuilder {
    url: Option<String>,
    key_prefix: Option<String>,
}

impl RedisBackendBuilder {
    /// Empty builder; set fields or call [`Self::from_env_defaults`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill unset fields from `VALENCE_REDIS_*` environment variables.
    #[must_use]
    pub fn from_env_defaults(mut self) -> Self {
        if self.url.is_none() {
            self.url = redis_url_from_env();
        }
        if self.key_prefix.is_none() {
            self.key_prefix = std::env::var(KEY_PREFIX_ENV)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| Some(DEFAULT_KEY_PREFIX.to_string()));
        }
        self
    }

    /// Redis connection URL.
    #[must_use]
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// Key prefix for Valence namespacing.
    #[must_use]
    pub fn key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(prefix.into());
        self
    }

    /// Whether a URL is set explicitly or resolvable via env defaults.
    pub fn has_url(&self) -> bool {
        self.url.is_some() || redis_url_from_env().is_some()
    }

    /// Resolve configuration without connecting.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when no Redis URL is configured.
    pub fn resolve(self) -> Result<RedisConfig> {
        let builder = self.from_env_defaults();
        let url = builder.url.ok_or_else(|| {
            Error::Internal(format!(
                "{URL_ENV} or {TEST_URL_ENV} not set for redis adapter"
            ))
        })?;
        Ok(RedisConfig {
            url,
            key_prefix: builder
                .key_prefix
                .unwrap_or_else(|| DEFAULT_KEY_PREFIX.to_string()),
        })
    }

    /// Connect and return a [`super::backend::RedisBackend`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when config is incomplete, or [`Error::Database`] on connect failure.
    pub async fn build(self) -> Result<super::backend::RedisBackend> {
        let config = self.resolve()?;
        super::backend::RedisBackend::connect_with_config(config).await
    }
}

/// Builder for [`super::fleet::FleetRedisBackend`].
#[derive(Debug, Clone, Default)]
pub struct FleetRedisBackendBuilder {
    urls: Option<Vec<String>>,
    key_prefix: Option<String>,
}

impl FleetRedisBackendBuilder {
    /// Empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill unset fields from `VALENCE_REDIS_URLS` / `VALENCE_REDIS_KEY_PREFIX`.
    #[must_use]
    pub fn from_env_defaults(mut self) -> Self {
        if self.urls.is_none() {
            self.urls = fleet_urls_from_env().ok();
        }
        if self.key_prefix.is_none() {
            self.key_prefix = std::env::var(KEY_PREFIX_ENV)
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| Some(DEFAULT_KEY_PREFIX.to_string()));
        }
        self
    }

    /// Standalone Redis node URLs.
    #[must_use]
    pub fn urls(mut self, urls: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.urls = Some(urls.into_iter().map(|u| u.into()).collect());
        self
    }

    /// Shared key prefix for all fleet nodes.
    #[must_use]
    pub fn key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.key_prefix = Some(prefix.into());
        self
    }

    /// Whether fleet URLs are configured.
    pub fn has_urls(&self) -> bool {
        self.urls.as_ref().is_some_and(|u| !u.is_empty()) || fleet_urls_from_env().is_ok()
    }

    /// Resolve fleet settings.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when fleet URLs are missing or empty.
    pub fn resolve(self) -> Result<(Vec<String>, String)> {
        let builder = self.from_env_defaults();
        let urls = builder.urls.filter(|u| !u.is_empty()).ok_or_else(|| {
            Error::Internal(format!("{URLS_ENV} not set for redis fleet adapter"))
        })?;
        Ok((
            urls,
            builder
                .key_prefix
                .unwrap_or_else(|| DEFAULT_KEY_PREFIX.to_string()),
        ))
    }

    /// Connect fleet backend.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when URLs are missing, or [`Error::Database`] on connect failure.
    pub async fn build(self) -> Result<super::fleet::FleetRedisBackend> {
        let (urls, prefix) = self.resolve()?;
        super::fleet::FleetRedisBackend::connect_with_urls(urls, prefix).await
    }
}

/// Test/bench URL helper (explicit default, not env-first).
pub fn test_url() -> String {
    redis_url_from_env().unwrap_or_else(|| "redis://127.0.0.1:6379".into())
}

fn redis_url_from_env() -> Option<String> {
    std::env::var(URL_ENV)
        .ok()
        .or_else(|| std::env::var(TEST_URL_ENV).ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn fleet_urls_from_env() -> Result<Vec<String>> {
    let raw =
        std::env::var(URLS_ENV).map_err(|_| Error::Internal(format!("{URLS_ENV} not set")))?;
    let urls: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if urls.is_empty() {
        return Err(Error::Internal(format!("{URLS_ENV} is empty")));
    }
    Ok(urls)
}
