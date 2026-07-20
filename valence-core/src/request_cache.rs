//! Request-scoped permission check cache (shared via [`Arc`] on cloned [`Valence`](crate::Valence) handles).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Memoizes permission name lookups for one HTTP/server-fn request.
#[derive(Clone, Default)]
pub struct RequestPermissionCache {
    inner: Arc<Mutex<HashMap<String, bool>>>,
}

impl RequestPermissionCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Return a cached allow/deny decision for `key`, if present.
    pub fn get(&self, key: &str) -> Option<bool> {
        self.inner.lock().ok()?.get(key).copied()
    }

    /// Store an allow/deny decision for `key`.
    pub fn set(&self, key: &str, allowed: bool) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(key.to_string(), allowed);
        }
    }
}
