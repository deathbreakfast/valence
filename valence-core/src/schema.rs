//! Schema metadata registration hooks for `valence_schema!`.

use crate::schema_api::{Schema, SchemaConnection};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Metadata for a schema definition discovered at runtime.
#[derive(Debug, Clone)]
pub struct SchemaMetadata {
    pub table_name: &'static str,
    pub version: &'static str,
    pub description: Option<&'static str>,
    pub privacy_read: &'static str,
    pub privacy_write: &'static str,
    pub databases: &'static [String],
    pub schema: &'static Schema,
}

/// Alias used by `valence_schema!` codegen.
pub type SchemaMetadataStruct = SchemaMetadata;

impl SchemaMetadata {
    #[must_use]
    pub fn new(
        table_name: &'static str,
        version: &'static str,
        description: Option<&'static str>,
        privacy_read: &'static str,
        privacy_write: &'static str,
        databases: &'static [String],
        schema: &'static Schema,
    ) -> Self {
        Self {
            table_name,
            version,
            description,
            privacy_read,
            privacy_write,
            databases,
            schema,
        }
    }

    #[must_use]
    pub fn from_schema(schema: &'static Schema) -> Self {
        Self {
            table_name: schema.name.as_str(),
            version: schema.version.as_str(),
            description: schema.meta.description.as_deref(),
            privacy_read: schema.privacy.read.as_str(),
            privacy_write: schema.privacy.write.as_str(),
            databases: schema.databases.as_slice(),
            schema,
        }
    }
}

/// Lazy initializer submitted via `inventory::submit!`.
pub struct SchemaMetadataInit(pub fn() -> &'static SchemaMetadata);

inventory::collect!(SchemaMetadataInit);

/// Codegen-submitted trait-merged connections for deletion graph and tooling.
pub struct SchemaConnectionsOverlayInit(pub fn() -> (&'static str, &'static [SchemaConnection]));

inventory::collect!(SchemaConnectionsOverlayInit);

/// Connections for `table_name`, preferring macro-registered schema connections.
pub fn schema_connections_for_table(
    meta: &SchemaMetadata,
) -> std::borrow::Cow<'static, [SchemaConnection]> {
    if !meta.schema.connections.is_empty() {
        return std::borrow::Cow::Borrowed(meta.schema.connections.as_slice());
    }
    for init in inventory::iter::<SchemaConnectionsOverlayInit> {
        let (table, conns) = (init.0)();
        if table == meta.table_name {
            return std::borrow::Cow::Borrowed(conns);
        }
    }
    std::borrow::Cow::Borrowed(&[])
}

/// Owned registry for schema metadata.
#[derive(Debug)]
pub struct SchemaRegistry {
    inner: HashMap<String, &'static SchemaMetadata>,
}

impl SchemaRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    #[must_use]
    pub fn auto_discover() -> Self {
        let mut registry = Self::new();
        for init in inventory::iter::<SchemaMetadataInit> {
            let metadata = (init.0)();
            registry.register(metadata);
        }
        registry
    }

    /// # Panics
    ///
    /// Panics if the global registry has already been initialized.
    pub fn set_global(registry: SchemaRegistry) {
        assert!(
            GLOBAL_REGISTRY.set(registry).is_ok(),
            "SchemaRegistry::set_global called more than once"
        );
    }

    pub fn global() -> &'static SchemaRegistry {
        GLOBAL_REGISTRY.get_or_init(SchemaRegistry::auto_discover)
    }

    /// Insert schema metadata for `table_name`.
    ///
    /// # Panics
    ///
    /// Panics when `table_name` is already registered. Duplicate inventory
    /// submissions must not silently pick a winner (policy strength is not
    /// correlated with field/connection count). Prefer a single
    /// [`SchemaMetadataInit`] per table and
    /// [`SchemaConnectionsOverlayInit`] for trait-merged connections.
    pub fn register(&mut self, metadata: &'static SchemaMetadata) {
        let key = metadata.table_name.to_string();
        assert!(
            !self.inner.contains_key(&key),
            "duplicate SchemaMetadata registration for table `{key}`: \
             multiple inventory submissions were linked. Keep one \
             SchemaMetadataInit per table (use SchemaConnectionsOverlayInit \
             for trait-merged connections)."
        );
        self.inner.insert(key, metadata);
    }

    pub fn register_schema(&mut self, schema: &'static Schema) {
        let metadata = SchemaMetadata::from_schema(schema);
        self.register(Box::leak(Box::new(metadata)));
    }

    pub fn get_schema(&self, table_name: &str) -> Option<&'static SchemaMetadata> {
        self.inner.get(table_name).copied()
    }

    pub fn get_full_schema(&self, table_name: &str) -> Option<&'static Schema> {
        self.get_schema(table_name).map(|meta| meta.schema)
    }

    pub fn list_schemas(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.inner.keys().map(String::as_str).collect();
        keys.sort_unstable();
        keys
    }

    pub fn has_schema(&self, table_name: &str) -> bool {
        self.inner.contains_key(table_name)
    }

    /// True when `edge_table` appears as a ManyToMany `edge_table` on any
    /// registered (or overlay) schema connection.
    #[must_use]
    pub fn has_edge_table(edge_table: &str) -> bool {
        let matches = |meta: &SchemaMetadata| {
            schema_connections_for_table(meta)
                .iter()
                .any(|c| c.edge_table.as_deref() == Some(edge_table))
        };
        for name in Self::global().list_schemas() {
            if let Some(meta) = Self::global().get_schema(name) {
                if matches(meta) {
                    return true;
                }
            }
        }
        SCHEMA_OVERLAY
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|meta| matches(meta))
    }
}

static GLOBAL_REGISTRY: OnceLock<SchemaRegistry> = OnceLock::new();

/// Process-local overlay consulted before [`SchemaRegistry::global`] (tests / dynamic hosts).
static SCHEMA_OVERLAY: std::sync::LazyLock<
    std::sync::RwLock<HashMap<String, &'static SchemaMetadata>>,
> = std::sync::LazyLock::new(|| std::sync::RwLock::new(HashMap::new()));

impl SchemaRegistry {
    /// Insert or replace a schema visible to [`Self::lookup`] (used by defer-to-edge parent hops).
    pub fn register_overlay(metadata: &'static SchemaMetadata) {
        SCHEMA_OVERLAY
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(metadata.table_name.to_string(), metadata);
    }

    /// Resolve a table: overlay first, then the process-global registry.
    pub fn lookup(table_name: &str) -> Option<&'static SchemaMetadata> {
        if let Some(meta) = SCHEMA_OVERLAY
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(table_name)
            .copied()
        {
            return Some(meta);
        }
        Self::global().get_schema(table_name)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::DEFAULT_IN_MEMORY;
    use crate::schema_api::{SchemaConnection, SchemaField, SchemaMeta, SchemaPrivacy};

    fn build_schema(name: &str) -> &'static Schema {
        Box::leak(Box::new(Schema {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            databases: vec!["default".to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "service".to_string(),
            },
            policies: None,
            fields: vec![SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: None,
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            }],
            edges: Vec::new(),
            connections: Vec::new(),
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,

                repository: "https://github.com/unified-field-dev/valence".to_string(),
            },
        }))
    }

    #[test]
    fn register_and_list() {
        let mut registry = SchemaRegistry::new();
        let schema = build_schema("fixture");
        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(schema))));
        assert!(registry.has_schema("fixture"));
        assert_eq!(registry.list_schemas(), vec!["fixture"]);
    }

    #[test]
    #[should_panic(expected = "duplicate SchemaMetadata registration for table `fixture`")]
    fn register_duplicate_table_panics() {
        let mut registry = SchemaRegistry::new();
        let schema_a = build_schema("fixture");
        let schema_b = build_schema("fixture");
        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(schema_a))));
        registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(schema_b))));
    }

    #[test]
    fn has_edge_table_from_connection() {
        let schema = Box::leak(Box::new(Schema {
            name: "edge_probe_parent".to_string(),
            version: "1.0.0".to_string(),
            databases: vec!["default".to_string()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "service".to_string(),
            },
            policies: None,
            fields: vec![SchemaField {
                name: "id".to_string(),
                field_type: "string".to_string(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: Vec::new(),
                policies: None,
                encrypted: false,
                enum_variants: Vec::new(),
                enum_type: None,
                model_path: None,
            }],
            edges: Vec::new(),
            connections: vec![SchemaConnection {
                name: "peers".to_string(),
                from_table: "edge_probe_parent".to_string(),
                from_field: "id".to_string(),
                to_table: "edge_probe_parent".to_string(),
                cardinality: "ManyToMany".to_string(),
                required: false,
                on_delete: "Cascade".to_string(),
                label: "peers".to_string(),
                model_path: None,
                reverse_field: None,
                edge_table: Some("edge_probe_rel".to_string()),
                target_trait: None,
            }],
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,

                repository: "https://github.com/unified-field-dev/valence".to_string(),
            },
        }));
        SchemaRegistry::register_overlay(Box::leak(Box::new(SchemaMetadata::from_schema(schema))));
        assert!(SchemaRegistry::has_edge_table("edge_probe_rel"));
        assert!(!SchemaRegistry::has_edge_table("missing_edge_rel"));
    }
}
