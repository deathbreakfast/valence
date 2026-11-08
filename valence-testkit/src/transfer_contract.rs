//! Contract for [`TableTransfer`]: a transferred set reads back identically, history rows
//! and edges included, and a transfer that fails partway resumes from its checkpoint.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use valence_core::backend::{
    BackendCapabilities, DatabaseBackend, EdgeRow, TableTransfer, TransferCheckpoint,
};
use valence_core::compiled_query::CompiledQuery;
use valence_core::record_id::RecordId;
use valence_core::storage_layout::StorageLayout;
use valence_core::{Error, Result};

const POST: &str = "valence_xfer_post";
const HISTORY: &str = "valence_xfer_post_history";
const TAG: &str = "valence_xfer_tag";
/// Not in the transferred set: its rows and outgoing edges stay behind.
const OTHER: &str = "valence_xfer_other";
const SET: [&str; 3] = [POST, HISTORY, TAG];

const EDGE_TAGGED: &str = "xfer_tagged";
const EDGE_REPLIES: &str = "xfer_replies";
const EDGE_REVISION_OF: &str = "xfer_revision_of";
const EDGE_MENTIONS: &str = "xfer_mentions";

const POST_IDS: [&str; 5] = ["p1", "p2", "p3", "p4", "p5"];
const TAG_IDS: [&str; 3] = ["t1", "t2", "t3"];
const POST_SCHEMA_VERSION: &str = "xfer-v3";

fn post_row(id: &str, n: i64) -> Value {
    json!({"id": id, "title": format!("title {id}"), "views": n * 7})
}

/// History rows follow the `RecordHistory` shape: a `history_id` plus the snapshot.
fn history_rows() -> Vec<Value> {
    let mut out = Vec::new();
    for (i, post) in POST_IDS.iter().enumerate().take(3) {
        for rev in 1..=2 {
            let id = format!("h{}{rev}", i + 1);
            out.push(json!({
                "id": id,
                "history_id": id,
                "post_id": post,
                "revision": rev,
                "recorded_at": format!("2026-10-0{rev}T12:00:00Z"),
                "title": format!("title {post} rev {rev}"),
            }));
        }
    }
    out
}

fn edge(from: (&str, &str), edge_type: &str, to: (&str, &str)) -> EdgeRow {
    EdgeRow {
        from: RecordId::new(from.0, from.1),
        edge_type: edge_type.to_string(),
        to: RecordId::new(to.0, to.1),
    }
}

fn fixture_edges() -> Vec<EdgeRow> {
    vec![
        edge((POST, "p1"), EDGE_TAGGED, (TAG, "t1")),
        edge((POST, "p1"), EDGE_TAGGED, (TAG, "t2")),
        edge((POST, "p2"), EDGE_TAGGED, (TAG, "t1")),
        edge((POST, "p3"), EDGE_REPLIES, (POST, "p1")),
        edge((POST, "p4"), EDGE_REPLIES, (POST, "p3")),
        edge((HISTORY, "h11"), EDGE_REVISION_OF, (POST, "p1")),
        edge((HISTORY, "h21"), EDGE_REVISION_OF, (POST, "p2")),
        edge((OTHER, "o1"), EDGE_MENTIONS, (POST, "p1")),
    ]
}

/// Seed the fixture on `source` and clear any earlier copy from `target`.
async fn setup(source: &dyn DatabaseBackend, target: &dyn DatabaseBackend) -> Result<()> {
    for table in [POST, HISTORY, TAG, OTHER] {
        source.ensure_schemaless_table(table).await?;
    }
    let fixture_ids = |table: &str| -> Vec<String> {
        match table {
            POST => POST_IDS.iter().map(|s| (*s).to_string()).collect(),
            HISTORY => history_rows()
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_string())
                .collect(),
            TAG => TAG_IDS.iter().map(|s| (*s).to_string()).collect(),
            _ => vec!["o1".to_string()],
        }
    };
    for backend in [source, target] {
        for e in fixture_edges() {
            backend.unrelate_edge(&e.from, &e.edge_type, &e.to).await?;
        }
        for table in [POST, HISTORY, TAG, OTHER] {
            // A fresh target has no tables yet; leave it that way so the transfer creates them.
            if backend.inspect_typed_layout(table).await?.is_none() {
                continue;
            }
            for id in fixture_ids(table) {
                backend.delete_record(table, &id).await?;
            }
        }
    }
    for (n, id) in (1..).zip(POST_IDS.iter().rev()) {
        source.create_record(POST, post_row(id, n)).await?;
    }
    for row in history_rows() {
        source.create_record(HISTORY, row).await?;
    }
    for id in TAG_IDS {
        source
            .create_record(TAG, json!({"id": id, "label": format!("tag {id}")}))
            .await?;
    }
    source
        .create_record(OTHER, json!({"id": "o1", "note": "stays behind"}))
        .await?;
    for e in fixture_edges() {
        source.relate_edge(&e.from, &e.edge_type, &e.to).await?;
    }
    source
        .write_schema_version(POST, POST_SCHEMA_VERSION)
        .await?;
    Ok(())
}

pub(crate) async fn scan_rows(backend: &dyn DatabaseBackend, table: &str) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    loop {
        let after = out
            .last()
            .and_then(|r| r.get("id")?.get("id")?.as_str().map(str::to_string));
        let rows = backend.scan_table(table, after.as_deref(), 3).await?;
        let n = rows.len();
        out.extend(rows);
        if n < 3 {
            return Ok(out);
        }
    }
}

/// Fixture edges as `backend` stores them (the backend may hold unrelated edges too).
async fn scan_fixture_edges(backend: &dyn DatabaseBackend) -> Result<Vec<EdgeRow>> {
    let fixture = fixture_edges();
    let mut out = Vec::new();
    let mut after: Option<EdgeRow> = None;
    loop {
        let page = backend.scan_edges(None, after.as_ref(), 50).await?;
        after = page.last().cloned();
        out.extend(page.iter().filter(|e| fixture.contains(e)).cloned());
        if page.len() < 50 {
            return Ok(out);
        }
    }
}

/// Check every set table, the schema stamp and the in-set edges match between backends.
async fn assert_parity(source: &dyn DatabaseBackend, target: &dyn DatabaseBackend) -> Result<()> {
    for table in SET {
        let src = scan_rows(source, table).await?;
        assert!(!src.is_empty(), "source {table} is seeded");
        assert_eq!(scan_rows(target, table).await?, src, "rows of {table}");
    }
    assert_eq!(
        scan_rows(target, HISTORY).await?.len(),
        history_rows().len()
    );
    assert_eq!(
        target.read_schema_version(POST).await?.as_deref(),
        Some(POST_SCHEMA_VERSION),
        "schema stamp"
    );
    assert!(
        target.get_record(OTHER, "o1").await?.is_none(),
        "table outside the set is not copied"
    );
    let expected: Vec<EdgeRow> = scan_fixture_edges(source)
        .await?
        .into_iter()
        .filter(|e| e.from.table() != OTHER)
        .collect();
    assert_eq!(expected.len(), fixture_edges().len() - 1);
    assert_eq!(scan_fixture_edges(target).await?, expected, "edges");
    Ok(())
}

/// Transfer the fixture set in small pages and check it reads back identically.
pub async fn run_transfer_contract(
    source: Arc<dyn DatabaseBackend>,
    target: Arc<dyn DatabaseBackend>,
) -> Result<()> {
    let (source, target) = (source.as_ref(), target.as_ref());
    setup(source, target).await?;

    let mut checkpoint = TransferCheckpoint::default();
    let report = TableTransfer::new(source, target, &SET)
        .page_size(2)
        .run(&mut checkpoint)
        .await?;
    assert!(checkpoint.is_done());
    assert_eq!(
        report.rows(),
        POST_IDS.len() + history_rows().len() + TAG_IDS.len()
    );
    assert!(report.edges >= fixture_edges().len() - 1);
    assert_parity(source, target).await?;

    let mut zero = TransferCheckpoint::default();
    assert!(
        matches!(
            TableTransfer::new(source, target, &SET)
                .page_size(0)
                .run(&mut zero)
                .await,
            Err(Error::Validation(_))
        ),
        "page size 0 is rejected"
    );
    Ok(())
}

/// Fail a transfer partway, resume it, and check the result matches a clean transfer.
///
/// The first run fails partway through rows and the second partway through edges; each
/// resumes from the kept checkpoint. A second full run over the finished target is a
/// no-op for parity.
pub async fn run_transfer_resume_contract(
    source: Arc<dyn DatabaseBackend>,
    target: Arc<dyn DatabaseBackend>,
) -> Result<()> {
    let source = source.as_ref();
    let flaky = FlakyTarget::new(target);
    setup(source, &flaky).await?;
    let transfer = TableTransfer::new(source, &flaky, &SET).page_size(2);
    let mut checkpoint = TransferCheckpoint::default();

    flaky.fail_upserts_after(5);
    assert!(transfer.run(&mut checkpoint).await.is_err(), "row failure");
    assert!(!checkpoint.is_done());
    let after_rows_failure = checkpoint.clone();

    flaky.fail_upserts_after(usize::MAX);
    flaky.fail_relates_after(2);
    assert!(transfer.run(&mut checkpoint).await.is_err(), "edge failure");
    assert!(!checkpoint.is_done());
    assert_ne!(checkpoint, after_rows_failure, "checkpoint moved forward");

    flaky.fail_relates_after(usize::MAX);
    transfer.run(&mut checkpoint).await?;
    assert!(checkpoint.is_done());
    assert_parity(source, &flaky).await?;

    let mut again = TransferCheckpoint::default();
    transfer.run(&mut again).await?;
    assert_parity(source, &flaky).await?;
    Ok(())
}

/// Target wrapper that fails `upsert_record` / `relate_edge` once a write budget runs out.
#[derive(Debug)]
struct FlakyTarget {
    inner: Arc<dyn DatabaseBackend>,
    upserts_left: AtomicUsize,
    relates_left: AtomicUsize,
}

impl FlakyTarget {
    fn new(inner: Arc<dyn DatabaseBackend>) -> Self {
        Self {
            inner,
            upserts_left: AtomicUsize::new(usize::MAX),
            relates_left: AtomicUsize::new(usize::MAX),
        }
    }

    fn fail_upserts_after(&self, n: usize) {
        self.upserts_left.store(n, Ordering::SeqCst);
    }

    fn fail_relates_after(&self, n: usize) {
        self.relates_left.store(n, Ordering::SeqCst);
    }

    /// The contract drives one write at a time, so load-then-store is enough here.
    fn spend(budget: &AtomicUsize, op: &str) -> Result<()> {
        let left = budget.load(Ordering::SeqCst);
        if left == 0 {
            return Err(Error::database(format!("injected {op} failure")));
        }
        budget.store(left - 1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl DatabaseBackend for FlakyTarget {
    fn engine_id(&self) -> &'static str {
        self.inner.engine_id()
    }

    fn capabilities(&self) -> BackendCapabilities {
        self.inner.capabilities()
    }

    async fn execute_compiled_query(&self, compiled: &CompiledQuery) -> Result<Vec<Value>> {
        self.inner.execute_compiled_query(compiled).await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        self.inner.ensure_schemaless_table(table).await
    }

    async fn inspect_typed_layout(&self, table: &str) -> Result<Option<StorageLayout>> {
        self.inner.inspect_typed_layout(table).await
    }

    async fn ensure_typed_table(&self, layout: &StorageLayout) -> Result<()> {
        self.inner.ensure_typed_table(layout).await
    }

    async fn read_schema_version(&self, table: &str) -> Result<Option<String>> {
        self.inner.read_schema_version(table).await
    }

    async fn write_schema_version(&self, table: &str, version: &str) -> Result<()> {
        self.inner.write_schema_version(table, version).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<Value>> {
        self.inner.get_record(table, id).await
    }

    async fn create_record(&self, table: &str, content: Value) -> Result<Value> {
        self.inner.create_record(table, content).await
    }

    async fn update_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        self.inner.update_record(table, id, content).await
    }

    async fn upsert_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        Self::spend(&self.upserts_left, "upsert")?;
        self.inner.upsert_record(table, id, content).await
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        self.inner.delete_record(table, id).await
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        Self::spend(&self.relates_left, "relate")?;
        self.inner.relate_edge(from, edge_table, to).await
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        self.inner.unrelate_edge(from, edge_table, to).await
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        self.inner.get_edge_targets(from, edge_table).await
    }

    async fn scan_table(
        &self,
        table: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Value>> {
        self.inner.scan_table(table, after, limit).await
    }

    async fn scan_edges(
        &self,
        edge_type: Option<&str>,
        after: Option<&EdgeRow>,
        limit: usize,
    ) -> Result<Vec<EdgeRow>> {
        self.inner.scan_edges(edge_type, after, limit).await
    }
}
