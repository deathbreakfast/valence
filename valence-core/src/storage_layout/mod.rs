//! Schema-driven physical storage layout and dialect DDL export.
//!
//! Application [`crate::Schema`] / [`crate::FieldType`] strings map to engine-native
//! fields (SQL columns, Surreal `DEFINE FIELD`, Redis Hash fields, Indra properties).
//! Prefer typed `{Model}Schema::full()` over string registry lookups when the model
//! is known at compile time.

mod diff;
mod encode;
pub mod ensure;
mod export;
mod sql_types;

pub use diff::{additive_ops, AdditiveOp, LayoutDiff};
pub use ensure::{
    ensure_typed_table_for, ensure_typed_tables_from_registry, sync_typed_table_for,
    sync_typed_tables_from_registry,
};
pub use encode::{
    coerce_for_storage, decode_sql_cell, field_by_name, field_names_excluding_id,
    fields_from_content, row_from_columns, split_record_fields, sql_bind_text, validate_write_types,
};
pub use export::{
    postgres_add_column, sqlite_add_column, surreal_add_field, to_ddl, to_layout_json, DdlDialect,
};
pub use sql_types::{logical_type_to_storage, FieldStorage, SqlColumnType, SurrealFieldType};

use crate::error::{Error, Result};
use crate::safe_ident::assert_safe_ident;
use crate::schema::SchemaRegistry;
use crate::schema_api::Schema;
use crate::ttl::EXPIRE_AT_FIELD;

/// One physical field in a typed table layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutField {
    /// Column / Hash field / property name.
    pub name: String,
    /// Engine-agnostic storage kind.
    pub storage: FieldStorage,
    /// Primary key (always `id` for Valence models).
    pub primary_key: bool,
    /// Whether NULL is allowed.
    pub nullable: bool,
    /// Unique index requested.
    pub unique: bool,
    /// Non-unique index requested.
    pub indexed: bool,
}

/// Physical layout for one table, derived from schema metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLayout {
    /// Table / collection name.
    pub table: String,
    /// Ordered fields (includes `id` when present on the schema).
    pub fields: Vec<LayoutField>,
}

impl StorageLayout {
    /// Build layout from a full [`Schema`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] for unsafe identifiers or unknown field types.
    pub fn from_schema(schema: &Schema) -> Result<Self> {
        assert_safe_ident(&schema.name)?;
        let mut fields = Vec::with_capacity(schema.fields.len().saturating_add(1));
        let mut saw_id = false;
        for f in &schema.fields {
            assert_safe_ident(&f.name)?;
            let storage = logical_type_to_storage(&f.field_type)?;
            if f.name == "id" {
                saw_id = true;
            }
            fields.push(LayoutField {
                name: f.name.clone(),
                storage,
                primary_key: f.primary || f.name == "id",
                nullable: f.nullable && !f.primary && f.name != "id",
                unique: f.unique,
                indexed: f.indexed,
            });
        }
        if !saw_id {
            fields.insert(
                0,
                LayoutField {
                    name: "id".into(),
                    storage: FieldStorage::String,
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    indexed: false,
                },
            );
        }
        // Deferred TTL stamp column when schema declares TTL.
        if schema.ttl.is_some()
            && !fields.iter().any(|f| f.name == EXPIRE_AT_FIELD)
            && assert_safe_ident(EXPIRE_AT_FIELD).is_ok()
        {
            // Deferred TTL stamps use RFC3339 strings today (Mongo/native DateTime paths).
            fields.push(LayoutField {
                name: EXPIRE_AT_FIELD.into(),
                storage: FieldStorage::String,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: true,
            });
        }
        Ok(Self {
            table: schema.name.clone(),
            fields,
        })
    }

    /// Build layout from the global registry by table name.
    ///
    /// Prefer `{Model}Schema::full()` + [`Self::from_schema`] when the type is known.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when the table is not registered.
    pub fn from_registry_table(table: &str) -> Result<Self> {
        let schema = SchemaRegistry::global()
            .get_full_schema(table)
            .ok_or_else(|| Error::Internal(format!("SchemaRegistry missing entry for {table}")))?;
        Self::from_schema(schema)
    }

    /// Best-effort layout: registry schema when present, else dynamic fields from `content`.
    ///
    /// Used by adapters on first write for non-schema (contract) tables.
    ///
    /// # Errors
    ///
    /// Propagates identifier / type mapping failures.
    pub fn resolve_for_write(table: &str, content: &serde_json::Value) -> Result<Self> {
        if let Some(schema) = SchemaRegistry::global().get_full_schema(table) {
            let mut layout = Self::from_schema(schema)?;
            // Allow TTL / extra keys present on the wire to become columns.
            layout.merge_content_fields(content)?;
            return Ok(layout);
        }
        Self::from_content_keys(table, content)
    }

    /// Layout with `id` plus one field per top-level JSON key (JSON cell storage).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] for unsafe identifiers.
    pub fn from_content_keys(table: &str, content: &serde_json::Value) -> Result<Self> {
        assert_safe_ident(table)?;
        let mut fields = vec![LayoutField {
            name: "id".into(),
            storage: FieldStorage::String,
            primary_key: true,
            nullable: false,
            unique: true,
            indexed: false,
        }];
        if let Some(obj) = content.as_object() {
            for key in obj.keys() {
                if key == "id" {
                    continue;
                }
                assert_safe_ident(key)?;
                let storage = storage_from_json_value(&obj[key]);
                fields.push(LayoutField {
                    name: key.clone(),
                    storage,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: false,
                });
            }
        }
        Ok(Self {
            table: table.to_string(),
            fields,
        })
    }

    fn merge_content_fields(&mut self, content: &serde_json::Value) -> Result<()> {
        let Some(obj) = content.as_object() else {
            return Ok(());
        };
        for key in obj.keys() {
            if key == "id" || self.fields.iter().any(|f| f.name == *key) {
                continue;
            }
            assert_safe_ident(key)?;
            self.fields.push(LayoutField {
                name: key.clone(),
                storage: storage_from_json_value(&obj[key]),
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
            });
        }
        Ok(())
    }

    /// Field names in layout order (including `id`).
    #[must_use]
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.name.as_str()).collect()
    }

    /// Non-primary data fields.
    #[must_use]
    pub fn data_fields(&self) -> impl Iterator<Item = &LayoutField> {
        self.fields.iter().filter(|f| !f.primary_key)
    }

    /// Render create-table DDL (or structured JSON) for an engine id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] for unknown engines or unsafe idents.
    pub fn to_ddl(&self, engine_id: &str) -> Result<String> {
        export::to_ddl(self, engine_id)
    }

    /// Structured JSON description of the layout (Redis/Mongo/Indra export).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Serialization`] on encode failure.
    pub fn to_layout_json(&self) -> Result<serde_json::Value> {
        export::to_layout_json(self)
    }
}

fn storage_from_json_value(v: &serde_json::Value) -> FieldStorage {
    match v {
        serde_json::Value::Bool(_) => FieldStorage::Boolean,
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => FieldStorage::Integer,
        serde_json::Value::Number(_) => FieldStorage::Decimal,
        serde_json::Value::String(_) => FieldStorage::String,
        _ => FieldStorage::Json,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluator::DEFAULT_IN_MEMORY;
    use crate::schema_api::{SchemaField, SchemaMeta, SchemaPrivacy};
    use crate::KnownEngines;

    fn stub_schema(fields: Vec<SchemaField>) -> Schema {
        Schema {
            name: "account_email".into(),
            version: "0.1.0".into(),
            databases: vec!["default".into()],
            database_evaluator: &DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".into(),
                write: "public".into(),
            },
            policies: None,
            fields,
            edges: vec![],
            connections: vec![],
            side_effects: vec![],
            iters: vec![],
            composite_key: vec![],
            traits: vec![],
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".into(),
                row_count: 0,
                owner: "system".into(),
                description: None,
            },
        }
    }

    #[test]
    fn layout_maps_integer_and_string() {
        let schema = stub_schema(vec![
            SchemaField {
                name: "id".into(),
                field_type: "string".into(),
                primary: true,
                nullable: false,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: vec![],
                policies: None,
                encrypted: false,
                enum_variants: vec![],
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "address".into(),
                field_type: "string".into(),
                primary: false,
                nullable: false,
                indexed: false,
                unique: true,
                default: None,
                fk: None,
                validations: vec![],
                policies: None,
                encrypted: false,
                enum_variants: vec![],
                enum_type: None,
                model_path: None,
            },
            SchemaField {
                name: "value".into(),
                field_type: "integer".into(),
                primary: false,
                nullable: true,
                indexed: false,
                unique: false,
                default: None,
                fk: None,
                validations: vec![],
                policies: None,
                encrypted: false,
                enum_variants: vec![],
                enum_type: None,
                model_path: None,
            },
        ]);
        let layout = StorageLayout::from_schema(&schema).expect("layout");
        assert_eq!(layout.table, "account_email");
        let addr = layout.fields.iter().find(|f| f.name == "address").unwrap();
        assert!(addr.unique);
        assert_eq!(addr.storage, FieldStorage::String);
        let val = layout.fields.iter().find(|f| f.name == "value").unwrap();
        assert_eq!(val.storage, FieldStorage::Integer);
        let ddl = layout.to_ddl(KnownEngines::SQLITE).expect("ddl");
        assert!(ddl.contains("address TEXT"));
        assert!(ddl.contains("value INTEGER"));
        assert!(!ddl.contains("body"));
    }

    #[test]
    fn rejects_unsafe_table() {
        let mut schema = stub_schema(vec![]);
        schema.name = "bad;drop".into();
        assert!(StorageLayout::from_schema(&schema).is_err());
    }

    #[test]
    fn additive_diff_adds_missing_field() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![
                LayoutField {
                    name: "id".into(),
                    storage: FieldStorage::String,
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    indexed: false,
                },
                LayoutField {
                    name: "a".into(),
                    storage: FieldStorage::String,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: false,
                },
                LayoutField {
                    name: "b".into(),
                    storage: FieldStorage::Integer,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: false,
                },
            ],
        };
        let live = StorageLayout {
            table: "t".into(),
            fields: desired.fields[..2].to_vec(),
        };
        let diff = additive_ops(&desired, &live).expect("diff");
        assert_eq!(diff.ops.len(), 1);
        match &diff.ops[0] {
            AdditiveOp::AddField(f) => assert_eq!(f.name, "b"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn additive_diff_refuses_destructive() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![LayoutField {
                name: "id".into(),
                storage: FieldStorage::String,
                primary_key: true,
                nullable: false,
                unique: true,
                indexed: false,
            }],
        };
        let live = StorageLayout {
            table: "t".into(),
            fields: vec![
                desired.fields[0].clone(),
                LayoutField {
                    name: "orphan".into(),
                    storage: FieldStorage::String,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: false,
                },
            ],
        };
        let err = additive_ops(&desired, &live).expect_err("destructive");
        assert!(matches!(err, Error::Validation(_)));
    }

}
