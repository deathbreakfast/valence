//! Contract for moving a platform-shaped schema set between backends.
//!
//! Valence cannot depend on the platform crates, so this fixture mirrors the families an
//! installer writes: identity, Gluon, Pion, Nucleus, Gauge, and sealed secrets with their
//! hash-chained audit log. It runs [`TableTransfer`] over every table at once and checks
//! parity family by family: row counts, row contents, history rows, schema stamps and the
//! edges that cross families.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use serde_json::{json, Value};
use valence_core::backend::{DatabaseBackend, EdgeRow, TableTransfer, TransferCheckpoint};
use valence_core::record_id::RecordId;
use valence_core::Result;

use crate::transfer_contract::scan_rows;

const SECRETS_AUDIT: &str = "valence_plat_secret_audit";
/// Not in the transferred set: its rows and outgoing edges stay behind.
const OUTSIDE: &str = "valence_plat_outside";
const SCHEMA_VERSION: &str = "plat-v2";

/// Families and their tables: every table of the platform schema set.
const FAMILIES: [(&str, &[&str]); 6] = [
    (
        "identity",
        &[
            "valence_plat_user",
            "valence_plat_account",
            "valence_plat_session",
            "valence_plat_user_history",
        ],
    ),
    (
        "gluon",
        &["valence_plat_gluon_cell", "valence_plat_gluon_node"],
    ),
    ("pion", &["valence_plat_pion_pool", "valence_plat_pion_job"]),
    (
        "nucleus",
        &[
            "valence_plat_nucleus_zone",
            "valence_plat_nucleus_placement",
        ],
    ),
    (
        "gauge",
        &["valence_plat_gauge_grant", "valence_plat_gauge_role"],
    ),
    ("sealed_secrets", &["valence_plat_secret", SECRETS_AUDIT]),
];

const ROWS_PER_TABLE: usize = 4;
const AUDIT_ENTRIES: usize = 5;

/// Rows seen in one family on the target after the transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyCount {
    /// Family name.
    pub family: &'static str,
    /// Rows in the family's tables on the source.
    pub source_rows: usize,
    /// Rows in the family's tables on the target.
    pub target_rows: usize,
}

fn chain_hash(prev: &str, seq: usize, payload: &str) -> String {
    let mut h = DefaultHasher::new();
    (prev, seq, payload).hash(&mut h);
    format!("{:016x}", h.finish())
}

fn audit_rows() -> Vec<Value> {
    let mut prev = String::from("genesis");
    (1..=AUDIT_ENTRIES)
        .map(|seq| {
            let payload = format!("rotate secret {seq}");
            let hash = chain_hash(&prev, seq, &payload);
            let row = json!({
                "id": format!("a{seq}"), "seq": seq, "payload": payload,
                "prev_hash": prev, "hash": hash,
            });
            prev = hash;
            row
        })
        .collect()
}

fn rows_for(table: &str) -> Vec<Value> {
    if table == SECRETS_AUDIT {
        return audit_rows();
    }
    (1..=ROWS_PER_TABLE)
        .map(|n| {
            json!({
                "id": format!("r{n}"),
                "name": format!("{table} {n}"),
                "n": n,
                // Ciphertext-like payload; the transfer must carry it byte for byte.
                "blob": format!("{:0>8x}", n * 40_503),
            })
        })
        .collect()
}

fn set_tables() -> Vec<&'static str> {
    FAMILIES
        .iter()
        .flat_map(|(_, t)| t.iter().copied())
        .collect()
}

fn edge(from: (&str, &str), edge_type: &str, to: (&str, &str)) -> EdgeRow {
    EdgeRow {
        from: RecordId::new(from.0, from.1),
        edge_type: edge_type.to_string(),
        to: RecordId::new(to.0, to.1),
    }
}

/// Edges inside families, across families, and one leaving the set.
fn fixture_edges() -> Vec<EdgeRow> {
    vec![
        edge(
            ("valence_plat_user", "r1"),
            "plat_owns",
            ("valence_plat_account", "r1"),
        ),
        edge(
            ("valence_plat_session", "r1"),
            "plat_of",
            ("valence_plat_user", "r1"),
        ),
        edge(
            ("valence_plat_user_history", "r1"),
            "plat_revision_of",
            ("valence_plat_user", "r1"),
        ),
        edge(
            ("valence_plat_gluon_node", "r1"),
            "plat_in_cell",
            ("valence_plat_gluon_cell", "r1"),
        ),
        edge(
            ("valence_plat_pion_job", "r2"),
            "plat_runs_on",
            ("valence_plat_pion_pool", "r1"),
        ),
        edge(
            ("valence_plat_nucleus_placement", "r1"),
            "plat_in_zone",
            ("valence_plat_nucleus_zone", "r1"),
        ),
        edge(
            ("valence_plat_gluon_cell", "r1"),
            "plat_placed_by",
            ("valence_plat_nucleus_placement", "r1"),
        ),
        edge(
            ("valence_plat_gauge_grant", "r1"),
            "plat_grants",
            ("valence_plat_gauge_role", "r1"),
        ),
        edge(
            ("valence_plat_gauge_grant", "r2"),
            "plat_for",
            ("valence_plat_user", "r2"),
        ),
        edge(
            ("valence_plat_secret", "r1"),
            "plat_audited_by",
            (SECRETS_AUDIT, "a1"),
        ),
        edge(
            (OUTSIDE, "o1"),
            "plat_mentions",
            ("valence_plat_user", "r1"),
        ),
    ]
}

async fn clear(backend: &dyn DatabaseBackend) -> Result<()> {
    for e in fixture_edges() {
        backend.unrelate_edge(&e.from, &e.edge_type, &e.to).await?;
    }
    for table in set_tables().into_iter().chain([OUTSIDE]) {
        // A fresh target has no tables yet; leave it that way so the transfer creates them.
        if backend.inspect_typed_layout(table).await?.is_none() {
            continue;
        }
        let rows = if table == OUTSIDE {
            vec![json!({"id": "o1"})]
        } else {
            rows_for(table)
        };
        for row in rows {
            if let Some(id) = row["id"].as_str() {
                backend.delete_record(table, id).await?;
            }
        }
    }
    Ok(())
}

async fn seed(source: &dyn DatabaseBackend) -> Result<()> {
    for table in set_tables() {
        source.ensure_schemaless_table(table).await?;
        for row in rows_for(table) {
            source.create_record(table, row).await?;
        }
        source.write_schema_version(table, SCHEMA_VERSION).await?;
    }
    source.ensure_schemaless_table(OUTSIDE).await?;
    source
        .create_record(OUTSIDE, json!({"id": "o1", "note": "stays behind"}))
        .await?;
    for e in fixture_edges() {
        source.relate_edge(&e.from, &e.edge_type, &e.to).await?;
    }
    Ok(())
}

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

/// Walk the audit log in sequence order and check every link and hash.
fn audit_chain_verifies(rows: &[Value]) -> bool {
    let mut sorted: Vec<&Value> = rows.iter().collect();
    sorted.sort_by_key(|r| r["seq"].as_u64());
    let mut prev = String::from("genesis");
    sorted.iter().all(|r| {
        let ok = r["prev_hash"].as_str() == Some(prev.as_str())
            && r["hash"].as_str()
                == Some(
                    chain_hash(
                        &prev,
                        r["seq"].as_u64().unwrap_or(0) as usize,
                        r["payload"].as_str().unwrap_or_default(),
                    )
                    .as_str(),
                );
        prev = r["hash"].as_str().unwrap_or_default().to_string();
        ok
    })
}

/// Transfer the whole platform schema set and check parity per family.
///
/// Returns the per-family row counts so a caller can report them; any mismatch fails with
/// the family and table named in the assertion message.
pub async fn run_platform_transfer_contract(
    source: Arc<dyn DatabaseBackend>,
    target: Arc<dyn DatabaseBackend>,
) -> Result<Vec<FamilyCount>> {
    let (source, target) = (source.as_ref(), target.as_ref());
    clear(source).await?;
    clear(target).await?;
    seed(source).await?;

    let tables = set_tables();
    let mut checkpoint = TransferCheckpoint::default();
    let report = TableTransfer::new(source, target, &tables)
        .page_size(3)
        .run(&mut checkpoint)
        .await?;
    assert!(checkpoint.is_done());
    assert!(report.edges >= fixture_edges().len() - 1);

    let mut counts = Vec::new();
    for (family, family_tables) in FAMILIES {
        let (mut source_rows, mut target_rows) = (0, 0);
        for table in family_tables {
            let src = scan_rows(source, table).await?;
            let dst = scan_rows(target, table).await?;
            assert_eq!(src.len(), rows_for(table).len(), "{family}: {table} seeded");
            assert_eq!(dst, src, "{family}: rows of {table}");
            assert_eq!(
                target.read_schema_version(table).await?.as_deref(),
                Some(SCHEMA_VERSION),
                "{family}: schema stamp of {table}"
            );
            source_rows += src.len();
            target_rows += dst.len();
        }
        assert_eq!(source_rows, target_rows, "{family}: row count");
        counts.push(FamilyCount {
            family,
            source_rows,
            target_rows,
        });
    }

    assert!(
        audit_chain_verifies(&scan_rows(target, SECRETS_AUDIT).await?),
        "sealed_secrets: audit chain verifies on the target"
    );
    assert!(
        target.get_record(OUTSIDE, "o1").await?.is_none(),
        "table outside the set is not copied"
    );
    let expected: Vec<EdgeRow> = scan_fixture_edges(source)
        .await?
        .into_iter()
        .filter(|e| e.from.table() != OUTSIDE)
        .collect();
    assert_eq!(expected.len(), fixture_edges().len() - 1);
    assert_eq!(scan_fixture_edges(target).await?, expected, "edges");

    let mut again = TransferCheckpoint::default();
    TableTransfer::new(source, target, &tables)
        .page_size(3)
        .run(&mut again)
        .await?;
    for (family, family_tables) in FAMILIES {
        for table in family_tables {
            assert_eq!(
                scan_rows(target, table).await?.len(),
                rows_for(table).len(),
                "{family}: {table} unchanged by a second run"
            );
        }
    }
    Ok(counts)
}
