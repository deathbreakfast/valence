//! Generic string-keyed registry (inventory + HashMap, no quark dependency).

use std::collections::HashMap;

/// Keyed registry for metadata discovered at runtime.
#[derive(Debug, Clone)]
pub struct Registry<T> {
    entries: HashMap<String, T>,
}

pub trait Registrable {
    fn registry_key(&self) -> &str;
}

impl<T: Registrable> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Registrable> Registry<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn register(&mut self, item: T) {
        self.entries.insert(item.registry_key().to_string(), item);
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.entries.get(key)
    }

    pub fn list(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.entries.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.values()
    }
}
