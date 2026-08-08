//! Bulk row insertion before query benchmarks.

use std::sync::Arc;

use anyhow::Result;
use valence_core::DatabaseBackend;

const BATCH: usize = 1000;

/// Populate `count` rows in `table` via the adapter (typed columns from content keys).
///
/// Uses upsert so the workload is idempotent across shared wire stores — the hybrid
/// adapter and the standalone SQL adapter target the same physical table, and a plain
/// insert would collide on the primary key when both prefill the same rows.
pub async fn prefill_table(
    backend: Arc<dyn DatabaseBackend>,
    table: &str,
    count: usize,
) -> Result<usize> {
    backend.ensure_schemaless_table(table).await?;
    let mut inserted = 0usize;
    while inserted < count {
        let end = (inserted + BATCH).min(count);
        for i in inserted..end {
            let id = format!("prefill-{i}");
            backend
                .upsert_record(
                    table,
                    &id,
                    serde_json::json!({
                        "id": id,
                        "idx": i,
                        "label": format!("row-{i}"),
                    }),
                )
                .await?;
        }
        inserted = end;
    }
    Ok(count)
}
