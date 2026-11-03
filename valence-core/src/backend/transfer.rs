//! Copy a set of tables and their edges from one backend to another.
//!
//! [`TableTransfer`] pages through the source with [`DatabaseBackend::scan_table`] and
//! [`DatabaseBackend::scan_edges`] and writes each page to the target with
//! [`DatabaseBackend::upsert_record`] and [`DatabaseBackend::relate_edge`]. Row ids are
//! kept as they are, so history tables (rows shaped by the `RecordHistory` trait schema)
//! come across with the rest of the set as long as the caller lists them.
//!
//! A transfer is resumable rather than atomic: the port has no cross-call transaction.
//! [`TransferCheckpoint`] moves forward only after a whole page is written. When
//! [`TableTransfer::run`] fails, keep the checkpoint and call `run` again; both writes are
//! idempotent, so a page that was half written goes in a second time without duplicates.
//!
//! This works below privacy, like the rest of the port. There is no actor: the caller
//! holds both backend handles and owns that decision.
//!
//! ```no_run
//! # async fn demo(
//! #     source: &dyn valence_core::DatabaseBackend,
//! #     target: &dyn valence_core::DatabaseBackend,
//! # ) -> valence_core::Result<()> {
//! use valence_core::backend::{TableTransfer, TransferCheckpoint};
//!
//! let transfer = TableTransfer::new(source, target, &["post", "post_history"]);
//! let mut checkpoint = TransferCheckpoint::default();
//! let report = transfer.run(&mut checkpoint).await?;
//! println!("{} rows, {} edges", report.rows(), report.edges);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::port::{DatabaseBackend, EdgeRow};
use crate::error::{Error, Result};

/// Rows or edges per scan page when [`TableTransfer::page_size`] is not set.
pub const DEFAULT_PAGE_SIZE: usize = 500;

/// Where a transfer has got to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferPhase {
    /// Copying rows of `tables[table_index]`, after primary key `after`.
    Rows {
        /// Index into the transfer's table list.
        table_index: usize,
        /// Last id written for this table; `None` before the first page.
        after: Option<String>,
    },
    /// Copying edges, after `after` in scan order.
    Edges {
        /// Last edge scanned; `None` before the first page.
        after: Option<EdgeRow>,
    },
    /// Every table and edge has been written.
    Done,
}

/// Resume point for [`TableTransfer::run`]. `Default` starts from the beginning.
///
/// Serializable so a caller can persist it between process runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferCheckpoint {
    /// Current phase.
    pub phase: TransferPhase,
}

impl Default for TransferCheckpoint {
    fn default() -> Self {
        Self {
            phase: TransferPhase::Rows {
                table_index: 0,
                after: None,
            },
        }
    }
}

impl TransferCheckpoint {
    /// `true` once the transfer has finished.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.phase == TransferPhase::Done
    }
}

/// What one call to [`TableTransfer::run`] wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferReport {
    /// Rows written per table, in table order (tables this call skipped are absent).
    pub rows_per_table: Vec<(String, usize)>,
    /// Edges written.
    pub edges: usize,
}

impl TransferReport {
    /// Total rows written across tables.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows_per_table.iter().map(|(_, n)| n).sum()
    }

    fn add_rows(&mut self, table: &str, n: usize) {
        match self.rows_per_table.last_mut() {
            Some((t, count)) if t == table => *count += n,
            _ => self.rows_per_table.push((table.to_string(), n)),
        }
    }
}

/// Copies a set of tables, then the edges that start in them, from `source` to `target`.
///
/// Edges are copied when their `from` table is in the set. Rows already in the target
/// with the same id are overwritten.
#[derive(Debug)]
pub struct TableTransfer<'a> {
    source: &'a dyn DatabaseBackend,
    target: &'a dyn DatabaseBackend,
    tables: Vec<String>,
    page_size: usize,
}

impl<'a> TableTransfer<'a> {
    /// Transfer `tables`, in the order given, from `source` to `target`.
    #[must_use]
    pub fn new(
        source: &'a dyn DatabaseBackend,
        target: &'a dyn DatabaseBackend,
        tables: &[&str],
    ) -> Self {
        Self {
            source,
            target,
            tables: tables.iter().map(|t| (*t).to_string()).collect(),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// Rows or edges read per scan page. `0` makes [`Self::run`] fail.
    #[must_use]
    pub const fn page_size(mut self, page_size: usize) -> Self {
        self.page_size = page_size;
        self
    }

    /// Run from `checkpoint` to the end, advancing it after every page written.
    ///
    /// # Errors
    ///
    /// Returns the first source or target error, with `checkpoint` left at the last
    /// completed page; call again with the same checkpoint to resume. Also fails on a zero
    /// page size, a checkpoint whose table index is past the table list, or a source row
    /// without an `id`.
    pub async fn run(&self, checkpoint: &mut TransferCheckpoint) -> Result<TransferReport> {
        if self.page_size == 0 {
            return Err(Error::Validation(
                "transfer page size must be positive".into(),
            ));
        }
        let mut report = TransferReport::default();
        loop {
            match checkpoint.phase.clone() {
                TransferPhase::Rows { table_index, after } => {
                    let Some(table) = self.tables.get(table_index) else {
                        if table_index == self.tables.len() {
                            checkpoint.phase = TransferPhase::Edges { after: None };
                            continue;
                        }
                        return Err(Error::Validation(format!(
                            "transfer checkpoint table index {table_index} is past {} tables",
                            self.tables.len()
                        )));
                    };
                    if after.is_none() {
                        self.prepare_table(table).await?;
                    }
                    let rows = self
                        .source
                        .scan_table(table, after.as_deref(), self.page_size)
                        .await?;
                    let mut last = after;
                    for row in &rows {
                        let id = row_id(table, row)?;
                        self.target.upsert_record(table, &id, row.clone()).await?;
                        last = Some(id);
                    }
                    report.add_rows(table, rows.len());
                    checkpoint.phase = if rows.len() < self.page_size {
                        TransferPhase::Rows {
                            table_index: table_index + 1,
                            after: None,
                        }
                    } else {
                        TransferPhase::Rows {
                            table_index,
                            after: last,
                        }
                    };
                }
                TransferPhase::Edges { after } => {
                    let edges = self
                        .source
                        .scan_edges(None, after.as_ref(), self.page_size)
                        .await?;
                    for e in &edges {
                        if self.tables.iter().any(|t| t == e.from.table()) {
                            self.target
                                .relate_edge(&e.from, &e.edge_type, &e.to)
                                .await?;
                            report.edges += 1;
                        }
                    }
                    checkpoint.phase = if edges.len() < self.page_size {
                        TransferPhase::Done
                    } else {
                        TransferPhase::Edges {
                            after: edges.last().cloned(),
                        }
                    };
                }
                TransferPhase::Done => return Ok(report),
            }
        }
    }

    /// Create the table on the target with the source's layout when the target lacks it,
    /// and copy the schema version stamp.
    async fn prepare_table(&self, table: &str) -> Result<()> {
        let target_layout = self.target.inspect_typed_layout(table).await?;
        if target_layout.is_none_or(|l| l.fields.is_empty()) {
            if let Some(layout) = self.source.inspect_typed_layout(table).await? {
                if !layout.fields.is_empty() {
                    self.target.ensure_typed_table(&layout).await?;
                }
            }
        }
        if let Some(version) = self.source.read_schema_version(table).await? {
            self.target.write_schema_version(table, &version).await?;
        }
        Ok(())
    }
}

/// Primary key of a scanned row: `{"id": {"table", "id"}}` or a bare string id.
fn row_id(table: &str, row: &Value) -> Result<String> {
    match row.get("id") {
        Some(Value::String(s)) => Ok(crate::row_json::thing_to_id_only(s.clone())),
        Some(Value::Object(o)) => match o.get("id") {
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err(Error::Internal(format!(
                "transfer: row in {table} has an id object without a string id"
            ))),
        },
        _ => Err(Error::Internal(format!(
            "transfer: row in {table} has no id"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record_id::RecordId;
    use serde_json::json;

    #[test]
    fn row_id_reads_object_and_bare_forms() {
        assert_eq!(
            row_id("t", &json!({"id": {"table": "t", "id": "a1"}})).ok(),
            Some("a1".to_string())
        );
        assert_eq!(
            row_id("t", &json!({"id": "t:a2"})).ok(),
            Some("a2".to_string())
        );
        assert_eq!(
            row_id("t", &json!({"id": "a3"})).ok(),
            Some("a3".to_string())
        );
        assert!(row_id("t", &json!({"name": "x"})).is_err());
        assert!(row_id("t", &json!({"id": {"table": "t"}})).is_err());
    }

    #[test]
    fn checkpoint_round_trips_through_json() {
        let cp = TransferCheckpoint {
            phase: TransferPhase::Edges {
                after: Some(EdgeRow {
                    from: RecordId::new("a", "1"),
                    edge_type: "follows".into(),
                    to: RecordId::new("b", "2"),
                }),
            },
        };
        let json = serde_json::to_string(&cp).unwrap_or_default();
        let back: Option<TransferCheckpoint> = serde_json::from_str(&json).ok();
        assert_eq!(back, Some(cp));
        assert!(!TransferCheckpoint::default().is_done());
    }

    #[test]
    fn report_sums_rows_and_merges_pages_of_one_table() {
        let mut r = TransferReport::default();
        r.add_rows("a", 2);
        r.add_rows("a", 1);
        r.add_rows("b", 4);
        assert_eq!(r.rows_per_table, vec![("a".into(), 3), ("b".into(), 4)]);
        assert_eq!(r.rows(), 7);
    }
}
