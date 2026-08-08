//! Dialect DDL and JSON layout export.

use super::{FieldStorage, StorageLayout};
use crate::error::{Error, Result};
use crate::safe_ident::assert_safe_ident;
use crate::KnownEngines;

/// Which create-DDL dialect to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdlDialect {
    Sqlite,
    Postgres,
    Surreal,
    /// Structured JSON (Redis / Mongo / Indra / mem).
    LayoutJson,
}

impl DdlDialect {
    /// Resolve from [`DatabaseBackend::engine_id`](crate::DatabaseBackend::engine_id).
    pub fn from_engine_id(engine_id: &str) -> Result<Self> {
        match engine_id {
            KnownEngines::SQLITE => Ok(Self::Sqlite),
            KnownEngines::POSTGRES | KnownEngines::HYBRID_INDRA_SQL => Ok(Self::Postgres),
            KnownEngines::SURREALDB => Ok(Self::Surreal),
            KnownEngines::REDIS
            | KnownEngines::MONGODB
            | KnownEngines::INDRADB
            | KnownEngines::INMEMORY_MEM => Ok(Self::LayoutJson),
            other => Err(Error::Validation(format!(
                "no DDL dialect for engine_id {other:?}"
            ))),
        }
    }
}

/// Render create DDL / layout JSON for `engine_id`.
pub fn to_ddl(layout: &StorageLayout, engine_id: &str) -> Result<String> {
    match DdlDialect::from_engine_id(engine_id)? {
        DdlDialect::Sqlite => sqlite_create(layout),
        DdlDialect::Postgres => postgres_create(layout),
        DdlDialect::Surreal => surreal_create(layout),
        DdlDialect::LayoutJson => Ok(serde_json::to_string_pretty(&to_layout_json(layout)?)
            .map_err(|e| Error::serialization_msg(e.to_string()))?),
    }
}

/// Structured layout export (no row data).
pub fn to_layout_json(layout: &StorageLayout) -> Result<serde_json::Value> {
    assert_safe_ident(&layout.table)?;
    let fields: Vec<serde_json::Value> = layout
        .fields
        .iter()
        .map(|f| {
            serde_json::json!({
                "name": f.name,
                "storage": storage_label(f.storage),
                "primary_key": f.primary_key,
                "nullable": f.nullable,
                "unique": f.unique,
                "indexed": f.indexed,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "table": layout.table,
        "fields": fields,
    }))
}

fn storage_label(s: FieldStorage) -> &'static str {
    match s {
        FieldStorage::String => "string",
        FieldStorage::Integer => "integer",
        FieldStorage::Decimal => "decimal",
        FieldStorage::Boolean => "boolean",
        FieldStorage::Date => "date",
        FieldStorage::Json => "json",
        FieldStorage::Currency => "currency",
    }
}

fn sqlite_create(layout: &StorageLayout) -> Result<String> {
    assert_safe_ident(&layout.table)?;
    let mut cols = Vec::new();
    for f in &layout.fields {
        assert_safe_ident(&f.name)?;
        let null = if f.nullable || !f.primary_key {
            if f.nullable {
                ""
            } else {
                " NOT NULL"
            }
        } else {
            " NOT NULL"
        };
        let pk = if f.primary_key { " PRIMARY KEY" } else { "" };
        // primary key already implies NOT NULL
        let null = if f.primary_key { " NOT NULL" } else { null };
        cols.push(format!(
            "{} {}{}{}",
            f.name,
            f.storage.sqlite_ddl(),
            pk,
            if f.primary_key { "" } else { null }
        ));
    }
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        layout.table,
        cols.join(", ")
    ))
}

fn postgres_create(layout: &StorageLayout) -> Result<String> {
    assert_safe_ident(&layout.table)?;
    let mut cols = Vec::new();
    for f in &layout.fields {
        assert_safe_ident(&f.name)?;
        let pk = if f.primary_key { " PRIMARY KEY" } else { "" };
        let null = if f.primary_key || !f.nullable {
            " NOT NULL"
        } else {
            ""
        };
        let null = if f.primary_key { " NOT NULL" } else { null };
        cols.push(format!(
            "{} {}{}{}",
            f.name,
            f.storage.postgres_ddl(),
            pk,
            if f.primary_key { "" } else { null }
        ));
    }
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        layout.table,
        cols.join(", ")
    ))
}

/// Surreal `TYPE` clause: nullable → `option<T>`; object/json → trailing `FLEXIBLE`.
///
/// Record links stay `string` (canonical `table:id`) so cross-backend hops do not require
/// the target row to exist in the same Surreal database.
fn surreal_type_clause(field: &super::LayoutField) -> String {
    let base = field.storage.surreal_ddl();
    let ty = if field.nullable {
        format!("option<{base}>")
    } else {
        base.to_string()
    };
    // FLEXIBLE is only valid for object (or array) types — not scalars.
    if matches!(field.storage, FieldStorage::Json | FieldStorage::Currency) {
        format!("{ty} FLEXIBLE")
    } else {
        ty
    }
}

fn surreal_create(layout: &StorageLayout) -> Result<String> {
    assert_safe_ident(&layout.table)?;
    let mut stmts = vec![format!(
        "DEFINE TABLE IF NOT EXISTS `{}` SCHEMAFULL",
        layout.table
    )];
    for f in &layout.fields {
        if f.name == "id" {
            continue;
        }
        assert_safe_ident(&f.name)?;
        stmts.push(format!(
            "DEFINE FIELD IF NOT EXISTS `{}` ON `{}` TYPE {}",
            f.name,
            layout.table,
            surreal_type_clause(f)
        ));
    }
    Ok(stmts.join(";\n"))
}

/// SQLite `ALTER TABLE … ADD COLUMN` for one field.
pub fn sqlite_add_column(table: &str, field: &super::LayoutField) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(&field.name)?;
    Ok(format!(
        "ALTER TABLE {table} ADD COLUMN {} {}",
        field.name,
        field.storage.sqlite_ddl()
    ))
}

/// Postgres `ALTER TABLE … ADD COLUMN`.
pub fn postgres_add_column(table: &str, field: &super::LayoutField) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(&field.name)?;
    let null = if field.nullable { "" } else { " NOT NULL" };
    // Additive sync: new NOT NULL columns need a default for existing rows — use NULLABLE add.
    let _ = null;
    Ok(format!(
        "ALTER TABLE {table} ADD COLUMN IF NOT EXISTS {} {}",
        field.name,
        field.storage.postgres_ddl()
    ))
}

/// Postgres `ALTER COLUMN … DROP NOT NULL`.
pub fn postgres_set_nullable(table: &str, field: &str) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(field)?;
    Ok(format!(
        "ALTER TABLE {table} ALTER COLUMN {field} DROP NOT NULL"
    ))
}

/// Postgres `ALTER COLUMN … SET NOT NULL`.
pub fn postgres_set_not_null(table: &str, field: &str) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(field)?;
    Ok(format!(
        "ALTER TABLE {table} ALTER COLUMN {field} SET NOT NULL"
    ))
}

/// Postgres `ALTER COLUMN … SET DEFAULT …` (value already validated as safe stamp-like literal).
pub fn postgres_set_default(table: &str, field: &str, value: &str) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(field)?;
    // Quote as string literal; escape single quotes.
    let escaped = value.replace('\'', "''");
    Ok(format!(
        "ALTER TABLE {table} ALTER COLUMN {field} SET DEFAULT '{escaped}'"
    ))
}

/// Postgres `ALTER COLUMN … DROP DEFAULT`.
pub fn postgres_drop_default(table: &str, field: &str) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(field)?;
    Ok(format!(
        "ALTER TABLE {table} ALTER COLUMN {field} DROP DEFAULT"
    ))
}

/// Surreal additive `DEFINE FIELD`.
pub fn surreal_add_field(table: &str, field: &super::LayoutField) -> Result<String> {
    assert_safe_ident(table)?;
    assert_safe_ident(&field.name)?;
    Ok(format!(
        "DEFINE FIELD IF NOT EXISTS `{}` ON `{table}` TYPE {}",
        field.name,
        surreal_type_clause(field)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_layout::LayoutField;

    #[test]
    fn surreal_nullable_scalar_uses_option_not_flexible() {
        let layout = StorageLayout {
            table: "demo".into(),
            fields: vec![
                LayoutField {
                    name: "id".into(),
                    storage: FieldStorage::String,
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    indexed: false,
                    default: None,
                record_table: None,
                },
                LayoutField {
                    name: "name".into(),
                    storage: FieldStorage::String,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: false,
                    default: None,
                record_table: None,
                },
            ],
        };
        let ddl = surreal_create(&layout).expect("ddl");
        assert!(
            ddl.contains("TYPE option<string>"),
            "expected option<string>, got {ddl}"
        );
        assert!(
            !ddl.contains("FLEXIBLE"),
            "scalar nullable must not use FLEXIBLE: {ddl}"
        );
    }

    #[test]
    fn surreal_json_field_keeps_flexible() {
        let f = LayoutField {
            name: "meta".into(),
            storage: FieldStorage::Json,
            primary_key: false,
            nullable: true,
            unique: false,
            indexed: false,
            default: None,
        record_table: None,
        };
        let clause = surreal_type_clause(&f);
        assert_eq!(clause, "option<object> FLEXIBLE");
    }

    #[test]
    fn surreal_record_link_stays_string_type() {
        let f = LayoutField {
            name: "project".into(),
            storage: FieldStorage::String,
            primary_key: false,
            nullable: false,
            unique: false,
            indexed: false,
            default: None,
            record_table: Some("hop_pair_project".into()),
        };
        // Cross-backend Valence keeps record links as strings even when record_table is set.
        let clause = surreal_type_clause(&f);
        assert_eq!(clause, "string");
    }
}
