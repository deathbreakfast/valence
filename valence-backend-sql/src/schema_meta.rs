//! `valence_schema_meta` — last-applied DSL schema version per table.

use valence_core::error::{Error, Result};
use valence_core::safe_ident::assert_safe_ident;
use valence_core::storage_layout::SCHEMA_META_TABLE;

use super::sqlite_ops::assert_safe_table;

fn validate_version_stamp(version: &str) -> Result<()> {
    if version.is_empty() || version.contains('\0') || version.len() > 128 {
        return Err(Error::Validation(
            "schema version stamp must be non-empty and at most 128 chars".into(),
        ));
    }
    // Bound characters: semver-like; bind as parameter so injection is not the risk —
    // still reject control / quote shapes for defense in depth.
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        return Err(Error::Validation(format!(
            "unsafe schema version stamp: {version:?}"
        )));
    }
    Ok(())
}

const ENSURE_META_SQLITE: &str = concat!(
    "CREATE TABLE IF NOT EXISTS valence_schema_meta (",
    "table_name TEXT PRIMARY KEY NOT NULL, ",
    "version TEXT NOT NULL)"
);

const ENSURE_META_POSTGRES: &str = concat!(
    "CREATE TABLE IF NOT EXISTS valence_schema_meta (",
    "table_name TEXT PRIMARY KEY NOT NULL, ",
    "version TEXT NOT NULL)"
);

/// Ensure the meta catalog table exists (SQLite).
pub async fn ensure_schema_meta_sqlite(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(ENSURE_META_SQLITE)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

/// Ensure the meta catalog table exists (Postgres).
pub async fn ensure_schema_meta_postgres(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query(ENSURE_META_POSTGRES)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

/// Read stamped schema version for `table` (SQLite).
pub async fn read_schema_version_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
) -> Result<Option<String>> {
    assert_safe_table(table)?;
    ensure_schema_meta_sqlite(pool).await?;
    let row = sqlx::query_scalar::<_, String>(&format!(
        "SELECT version FROM {SCHEMA_META_TABLE} WHERE table_name = ?"
    ))
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(row)
}

/// Upsert stamped schema version (SQLite).
pub async fn write_schema_version_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    version: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    validate_version_stamp(version)?;
    ensure_schema_meta_sqlite(pool).await?;
    sqlx::query(&format!(
        "INSERT INTO {SCHEMA_META_TABLE} (table_name, version) VALUES (?, ?) \
         ON CONFLICT(table_name) DO UPDATE SET version = excluded.version"
    ))
    .bind(table)
    .bind(version)
    .execute(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

/// Read stamped schema version for `table` (Postgres).
pub async fn read_schema_version_postgres(
    pool: &sqlx::PgPool,
    table: &str,
) -> Result<Option<String>> {
    assert_safe_ident(table)?;
    ensure_schema_meta_postgres(pool).await?;
    let row = sqlx::query_scalar::<_, String>(&format!(
        "SELECT version FROM {SCHEMA_META_TABLE} WHERE table_name = $1"
    ))
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(row)
}

/// Upsert stamped schema version (Postgres).
pub async fn write_schema_version_postgres(
    pool: &sqlx::PgPool,
    table: &str,
    version: &str,
) -> Result<()> {
    assert_safe_ident(table)?;
    validate_version_stamp(version)?;
    ensure_schema_meta_postgres(pool).await?;
    sqlx::query(&format!(
        "INSERT INTO {SCHEMA_META_TABLE} (table_name, version) VALUES ($1, $2) \
         ON CONFLICT (table_name) DO UPDATE SET version = EXCLUDED.version"
    ))
    .bind(table)
    .bind(version)
    .execute(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}
