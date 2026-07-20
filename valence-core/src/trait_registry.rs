//! Runtime trait metadata registry.

use crate::trait_schema::{TraitDefinition, TraitDefinitionInit, TraitImplementor};
use std::collections::HashMap;
use std::sync::OnceLock;

pub use crate::trait_schema::{TraitFieldDef, TraitPolicies, TraitPolicyRules};

#[derive(Debug)]
pub struct TraitRegistry {
    inner: HashMap<String, &'static TraitDefinition>,
    implementors: HashMap<String, Vec<String>>,
}

impl TraitRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            implementors: HashMap::new(),
        }
    }

    #[must_use]
    pub fn auto_discover() -> Self {
        let mut registry = Self::new();
        for init in inventory::iter::<TraitDefinitionInit> {
            let def = (init.0)();
            registry.inner.insert(def.name.to_string(), def);
        }
        for imp in inventory::iter::<TraitImplementor> {
            registry
                .implementors
                .entry(imp.trait_name.to_string())
                .or_default()
                .push(imp.table_name.to_string());
        }
        registry
    }

    pub fn global() -> &'static TraitRegistry {
        GLOBAL_TRAIT_REGISTRY.get_or_init(TraitRegistry::auto_discover)
    }

    /// # Panics
    ///
    /// Panics if the global registry has already been initialized.
    pub fn set_global(registry: TraitRegistry) {
        assert!(
            GLOBAL_TRAIT_REGISTRY.set(registry).is_ok(),
            "TraitRegistry::set_global called more than once"
        );
    }

    pub fn get_definition(&self, trait_name: &str) -> Option<&'static TraitDefinition> {
        self.inner.get(trait_name).copied()
    }

    pub fn tables_for_trait(&self, trait_name: &str) -> Vec<&str> {
        self.implementors
            .get(trait_name)
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }

    pub fn list_traits(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.inner.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static TraitDefinition> + '_ {
        self.inner.values().copied()
    }
}

static GLOBAL_TRAIT_REGISTRY: OnceLock<TraitRegistry> = OnceLock::new();

impl Default for TraitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_tables() {
        let mut registry = TraitRegistry::new();
        let def: &'static TraitDefinition = Box::leak(Box::new(TraitDefinition {
            name: "Named",
            fields: &[],
            connection_names: &[],
            policies: None,
        }));
        registry.inner.insert(def.name.to_string(), def);
        registry
            .implementors
            .entry("Named".into())
            .or_default()
            .push("user".into());
        assert_eq!(registry.tables_for_trait("Named"), vec!["user"]);
    }
}
