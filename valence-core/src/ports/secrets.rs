//! Secret lookup port — KMS/Vault-style adapters live in separate host crates.

use crate::error::{Error, Result};

/// Resolve secret material by key at runtime.
///
/// Implement this in a host crate (Vault, cloud KMS, …). Never embed product-specific
/// secret clients inside `valence-core`.
///
/// Wire with [`crate::ValenceBuilder::secret_provider`].
///
/// # Examples
///
/// ```
/// use valence_core::{EnvSecretProvider, SecretProvider};
///
/// let provider = EnvSecretProvider;
/// // Looks up `std::env::var("MY_API_KEY")` when present.
/// let _ = provider.get_secret("MY_API_KEY");
/// ```
pub trait SecretProvider: Send + Sync {
    /// Return `Ok(None)` when the key is absent.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    fn get_secret(&self, key: &str) -> Result<Option<String>>;
}

/// Always returns `Ok(None)` — default when no provider is configured.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpSecretProvider;

impl SecretProvider for NoOpSecretProvider {
    fn get_secret(&self, _key: &str) -> Result<Option<String>> {
        Ok(None)
    }
}

/// Read secrets from process environment variables (`key` = env var name).
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvSecretProvider;

impl SecretProvider for EnvSecretProvider {
    fn get_secret(&self, key: &str) -> Result<Option<String>> {
        match std::env::var(key) {
            Ok(v) => Ok(Some(v)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(e) => Err(Error::Internal(format!("env var {key}: {e}"))),
        }
    }
}
