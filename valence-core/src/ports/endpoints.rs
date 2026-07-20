//! Bootstrap-only physical URL resolution — not per-query schema routing.
//!
//! Schema `database:` evaluators ([`crate::DatabaseFromEngine`]) select a **router key**.
//! [`DatabaseEndpointResolver`] resolves a **physical URL** when a host bootstraps a
//! remote engine. Do not conflate the two.

use std::collections::HashMap;

use crate::error::Result;

/// Resolve a physical connection URL for a logical database name at bootstrap.
///
/// Wire with [`crate::ValenceBuilder::endpoint_resolver`].
///
/// # Examples
///
/// ```
/// use valence_core::{DatabaseEndpointResolver, StaticEndpointResolver};
///
/// let resolver = StaticEndpointResolver::from_pairs(&[
///     ("billing", "postgres://db.example/billing".to_string()),
/// ]);
/// assert_eq!(
///     resolver.resolve_url("billing").unwrap().as_deref(),
///     Some("postgres://db.example/billing")
/// );
/// ```
pub trait DatabaseEndpointResolver: Send + Sync {
    /// Return `Ok(None)` when the logical name has no mapped URL.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    fn resolve_url(&self, logical_name: &str) -> Result<Option<String>>;
}

/// Always returns `Ok(None)`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopEndpointResolver;

impl DatabaseEndpointResolver for NoopEndpointResolver {
    fn resolve_url(&self, _logical_name: &str) -> Result<Option<String>> {
        Ok(None)
    }
}

/// In-memory map of logical name → URL.
#[derive(Debug, Clone)]
pub struct StaticEndpointResolver {
    urls: HashMap<String, String>,
}

impl StaticEndpointResolver {
    /// Build from `(logical_name, url)` pairs with `'static` string slices.
    #[must_use]
    pub fn new(urls: Vec<(&'static str, &'static str)>) -> Self {
        let pairs: Vec<(&str, String)> = urls
            .into_iter()
            .map(|(logical, url)| (logical, url.to_string()))
            .collect();
        Self::from_pairs(&pairs)
    }

    /// Build from `(logical_name, url)` pairs.
    #[must_use]
    pub fn from_pairs(pairs: &[(&str, String)]) -> Self {
        let mut urls = HashMap::new();
        for (logical, url) in pairs {
            urls.insert((*logical).to_string(), url.clone());
        }
        Self { urls }
    }
}

impl DatabaseEndpointResolver for StaticEndpointResolver {
    fn resolve_url(&self, logical_name: &str) -> Result<Option<String>> {
        Ok(self.urls.get(logical_name).cloned())
    }
}

fn parse_env_endpoints() -> HashMap<String, String> {
    let mut out = HashMap::new();

    if let Ok(json) = std::env::var("VALENCE_ENDPOINTS_JSON") {
        if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&json) {
            out.extend(map);
        }
    }

    for (key, value) in std::env::vars() {
        let Some(rest) = key.strip_prefix("VALENCE_ENDPOINT_") else {
            continue;
        };
        if rest.is_empty() {
            continue;
        }
        let logical = rest.to_ascii_lowercase();
        if !value.trim().is_empty() {
            out.insert(logical, value);
        }
    }

    out
}

/// Resolve physical database URLs from env at bootstrap.
///
/// Supported:
/// - `VALENCE_ENDPOINTS_JSON='{"default":"http://127.0.0.1:8000"}'`
/// - `VALENCE_ENDPOINT_<LOGICAL>=url` (logical name lowercased)
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvEndpointResolver;

impl DatabaseEndpointResolver for EnvEndpointResolver {
    fn resolve_url(&self, logical_name: &str) -> Result<Option<String>> {
        let key = logical_name.to_ascii_lowercase();
        Ok(parse_env_endpoints().get(&key).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn static_resolver_from_pairs() {
        let resolver = StaticEndpointResolver::from_pairs(&[(
            "billing",
            "http://db.example/billing".to_string(),
        )]);
        assert_eq!(
            resolver.resolve_url("billing").unwrap(),
            Some("http://db.example/billing".to_string())
        );
    }

    #[test]
    fn static_resolver_new_honors_args() {
        let resolver = StaticEndpointResolver::new(vec![
            ("default", "postgres://localhost/valence"),
            ("billing", "postgres://localhost/billing"),
        ]);
        assert_eq!(
            resolver.resolve_url("default").unwrap().as_deref(),
            Some("postgres://localhost/valence")
        );
        assert_eq!(
            resolver.resolve_url("billing").unwrap().as_deref(),
            Some("postgres://localhost/billing")
        );
        assert_eq!(resolver.resolve_url("missing").unwrap(), None);
    }

    #[test]
    fn env_resolver_reads_prefixed_vars() {
        let _guard = env_test_lock();
        std::env::set_var("VALENCE_ENDPOINT_DEFAULT", "http://127.0.0.1:8000");
        let resolver = EnvEndpointResolver;
        assert_eq!(
            resolver.resolve_url("default").unwrap(),
            Some("http://127.0.0.1:8000".to_string())
        );
        std::env::remove_var("VALENCE_ENDPOINT_DEFAULT");
    }
}
