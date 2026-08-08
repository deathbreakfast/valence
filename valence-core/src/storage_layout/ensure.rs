//! Boot helpers: ensure / sync typed tables from the schema registry (version-gated).

use crate::error::Result;
use crate::schema::SchemaRegistry;
use crate::Valence;

use super::version_meta::version_stamp_matches;
use super::StorageLayout;

/// Ensure typed physical tables exist for every registered schema.
///
/// Stamps each table's DSL version after ensure.
///
/// # Errors
///
/// Returns the first backend or layout error.
pub async fn ensure_typed_tables_from_registry(valence: &Valence) -> Result<()> {
    for table in SchemaRegistry::global().list_schemas() {
        ensure_typed_table_for(valence, table).await?;
    }
    Ok(())
}

/// Version-gated additive sync for every registered schema.
///
/// When the physical stamp equals [`SchemaMetadata::version`](crate::schema::SchemaMetadata),
/// inspect/DDL are skipped. Otherwise layout is synced and the stamp is updated.
///
/// # Errors
///
/// Returns the first backend or layout error.
pub async fn sync_typed_tables_from_registry(valence: &Valence) -> Result<()> {
    for table in SchemaRegistry::global().list_schemas() {
        sync_typed_table_for(valence, table).await?;
    }
    Ok(())
}

/// Ensure one registered table and stamp its schema version.
pub async fn ensure_typed_table_for(valence: &Valence, table: &str) -> Result<()> {
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .ok_or_else(|| crate::error::Error::Internal(format!("SchemaRegistry missing {table}")))?;
    let layout = StorageLayout::from_schema(meta.schema)?;
    let backend = valence.backend_for_table(table)?;
    tracing::debug!(
        target: "valence_storage",
        table = layout.table.as_str(),
        engine_id = backend.engine_id(),
        field_count = layout.fields.len(),
        version = meta.version,
        "valence.storage.ensure_typed_table"
    );
    backend.ensure_typed_table(&layout).await?;
    backend.write_schema_version(table, meta.version).await?;
    Ok(())
}

/// Sync one registered table when the version stamp differs from the registry.
pub async fn sync_typed_table_for(valence: &Valence, table: &str) -> Result<()> {
    let meta = SchemaRegistry::global()
        .get_schema(table)
        .ok_or_else(|| crate::error::Error::Internal(format!("SchemaRegistry missing {table}")))?;
    let backend = valence.backend_for_table(table)?;
    let stamp = backend.read_schema_version(table).await?;
    if version_stamp_matches(stamp.as_deref(), meta.version) {
        tracing::debug!(
            target: "valence_storage",
            table,
            version = meta.version,
            engine_id = backend.engine_id(),
            "valence.storage.boot_sync_skip"
        );
        return Ok(());
    }
    let layout = StorageLayout::from_schema(meta.schema)?;
    let from_version = stamp.as_deref().unwrap_or("");
    tracing::info!(
        target: "valence_storage",
        table,
        from_version,
        to_version = meta.version,
        engine_id = backend.engine_id(),
        "valence.storage.boot_sync"
    );
    backend.sync_typed_table(&layout).await?;
    backend.write_schema_version(table, meta.version).await?;
    Ok(())
}
