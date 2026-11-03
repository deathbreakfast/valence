//! Shared enumeration contract for backends that implement `scan_table` / `scan_edges`.

use std::sync::Arc;

use serde_json::{json, Value};
use valence_core::backend::{DatabaseBackend, EdgeRow};
use valence_core::record_id::RecordId;
use valence_core::Result;

const TABLE_A: &str = "valence_scan_a";
const TABLE_B: &str = "valence_scan_b";
const MISSING_TABLE: &str = "valence_scan_missing";
const EDGE_FOLLOWS: &str = "scan_follows";
const EDGE_OWNS: &str = "scan_owns";

/// Ids chosen so byte order and locale order agree (lowercase ASCII and digits).
const IDS_A: [&str; 6] = ["a1", "a2", "a3", "a4", "a5", "a6"];
const IDS_B: [&str; 3] = ["b1", "b2", "b3"];

fn edge(from: (&str, &str), edge_type: &str, to: (&str, &str)) -> EdgeRow {
    EdgeRow {
        from: RecordId::new(from.0, from.1),
        edge_type: edge_type.to_string(),
        to: RecordId::new(to.0, to.1),
    }
}

/// Edges in primary-key order: `(from_table, from_id, edge_type, to_table, to_id)`.
fn expected_edges() -> Vec<EdgeRow> {
    vec![
        edge((TABLE_A, "a1"), EDGE_FOLLOWS, (TABLE_A, "a2")),
        edge((TABLE_A, "a1"), EDGE_FOLLOWS, (TABLE_B, "b1")),
        edge((TABLE_A, "a1"), EDGE_OWNS, (TABLE_B, "b2")),
        edge((TABLE_A, "a2"), EDGE_FOLLOWS, (TABLE_A, "a3")),
        edge((TABLE_B, "b1"), EDGE_OWNS, (TABLE_A, "a4")),
    ]
}

/// Seed a fixed fixture, then check that paging through it returns the same rows and edges
/// for every page size. Re-seeds idempotently so shared databases can be reused.
///
/// # Errors
///
/// Returns the backend error from seeding or scanning; assertion failures panic.
pub async fn run_scan_contract(backend: Arc<dyn DatabaseBackend>) -> Result<()> {
    let backend = backend.as_ref();
    seed(backend).await?;

    let expect_a: Vec<Value> = IDS_A.iter().map(|id| row_a(id)).collect();
    let expect_b: Vec<Value> = IDS_B.iter().map(|id| row_b(id)).collect();
    for page in [1, 2, 4, 100] {
        assert_eq!(
            scan_all(backend, TABLE_A, page).await?,
            expect_a,
            "page {page}"
        );
        assert_eq!(
            scan_all(backend, TABLE_B, page).await?,
            expect_b,
            "page {page}"
        );
    }
    assert!(
        backend
            .scan_table(MISSING_TABLE, None, 10)
            .await?
            .is_empty(),
        "missing table scans empty"
    );
    assert!(
        backend
            .scan_table(TABLE_A, Some("a6"), 10)
            .await?
            .is_empty(),
        "cursor past the last row scans empty"
    );

    let all = expected_edges();
    for page in [1, 2, 3, 100] {
        assert_eq!(
            scan_all_edges(backend, None, page).await?,
            all,
            "page {page}"
        );
    }
    let owns: Vec<EdgeRow> = all
        .iter()
        .filter(|e| e.edge_type == EDGE_OWNS)
        .cloned()
        .collect();
    assert_eq!(scan_all_edges(backend, Some(EDGE_OWNS), 1).await?, owns);
    assert_eq!(
        scan_all_edges(backend, Some("scan_no_such_edge"), 5).await?,
        Vec::<EdgeRow>::new()
    );
    Ok(())
}

fn row_a(id: &str) -> Value {
    json!({"id": {"table": TABLE_A, "id": id}, "name": format!("name-{id}"), "score": score(id)})
}

fn row_b(id: &str) -> Value {
    json!({"id": {"table": TABLE_B, "id": id}, "label": format!("label-{id}")})
}

fn score(id: &str) -> i64 {
    id.trim_start_matches('a').parse::<i64>().unwrap_or(0) * 10
}

async fn seed(backend: &dyn DatabaseBackend) -> Result<()> {
    backend.ensure_schemaless_table(TABLE_A).await?;
    backend.ensure_schemaless_table(TABLE_B).await?;
    for e in expected_edges() {
        backend.unrelate_edge(&e.from, &e.edge_type, &e.to).await?;
    }
    for id in IDS_A {
        backend.delete_record(TABLE_A, id).await?;
    }
    for id in IDS_B {
        backend.delete_record(TABLE_B, id).await?;
    }
    // Insert out of order so a scan that returns insertion order fails.
    for id in IDS_A.iter().rev() {
        backend
            .create_record(
                TABLE_A,
                json!({"id": id, "name": format!("name-{id}"), "score": score(id)}),
            )
            .await?;
    }
    for id in IDS_B.iter().rev() {
        backend
            .create_record(TABLE_B, json!({"id": id, "label": format!("label-{id}")}))
            .await?;
    }
    for e in expected_edges().iter().rev() {
        backend.relate_edge(&e.from, &e.edge_type, &e.to).await?;
    }
    Ok(())
}

async fn scan_all(backend: &dyn DatabaseBackend, table: &str, page: usize) -> Result<Vec<Value>> {
    let mut out: Vec<Value> = Vec::new();
    loop {
        let after = out.last().and_then(row_id);
        let rows = backend.scan_table(table, after.as_deref(), page).await?;
        let n = rows.len();
        out.extend(rows);
        if n < page {
            return Ok(out);
        }
    }
}

fn row_id(row: &Value) -> Option<String> {
    row.get("id")?.get("id")?.as_str().map(str::to_string)
}

async fn scan_all_edges(
    backend: &dyn DatabaseBackend,
    edge_type: Option<&str>,
    page: usize,
) -> Result<Vec<EdgeRow>> {
    let mut out: Vec<EdgeRow> = Vec::new();
    loop {
        let rows = backend.scan_edges(edge_type, out.last(), page).await?;
        let n = rows.len();
        out.extend(rows);
        if n < page {
            return Ok(out);
        }
    }
}
