//! Boot helpers: ensure / sync typed tables from the schema registry.

use crate::error::Result;
use crate::schema::SchemaRegistry;
use crate::Valence;

use super::StorageLayout;

/// Ensure typed physical tables exist for every registered schema.
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

/// Additive-sync typed tables for every registered schema.
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

/// Ensure one registered table.
pub async fn ensure_typed_table_for(valence: &Valence, table: &str) -> Result<()> {
    let layout = StorageLayout::from_registry_table(table)?;
    let backend = valence.backend_for_table(table)?;
    tracing::debug!(
        target: "valence_storage",
        table = layout.table.as_str(),
        engine_id = backend.engine_id(),
        field_count = layout.fields.len(),
        "valence.storage.ensure_typed_table"
    );
    backend.ensure_typed_table(&layout).await
}

/// Sync one registered table (additive).
pub async fn sync_typed_table_for(valence: &Valence, table: &str) -> Result<()> {
    let layout = StorageLayout::from_registry_table(table)?;
    let backend = valence.backend_for_table(table)?;
    backend.sync_typed_table(&layout).await
}
