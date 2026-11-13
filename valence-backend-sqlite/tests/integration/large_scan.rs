//! Queries over more than 1000 rows on SQLite: the pending-deletion lookup stays under
//! SQLite's expression depth limit, and resumed paging reads each raw row once.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use valence_backend_sqlite::SqliteBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::AUTHENTICATED;
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::query::{QueryCore, SortDirection};
use valence_core::schema::{SchemaMetadata, SchemaMetadataInit};
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::{
    DatabaseBackend, DatabaseEvaluator, OwnershipService, Valence, MAX_QUERY_LIMIT,
};

const TABLES: [&str; 4] = ["ls_lookup", "ls_window", "ls_resume", "ls_resume_small"];

fn field(name: &str, primary: bool) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: "string".to_string(),
        primary,
        nullable: false,
        indexed: false,
        unique: false,
        default: None,
        fk: None,
        validations: Vec::new(),
        policies: None,
        encrypted: false,
        enum_variants: Vec::new(),
        enum_type: None,
        model_path: None,
    }
}

fn rule(name: &str, eval: &'static valence_core::privacy::PrivacyRule) -> SchemaPolicyRule {
    SchemaPolicyRule {
        name: name.to_string(),
        description: None,
        evaluator: Some(eval),
    }
}

/// A table alice reads only the rows whose `user` is `alice`.
fn owner_schema(table: &str) -> &'static SchemaMetadata {
    let schema: &'static Schema = Box::leak(Box::new(Schema {
        name: table.to_string(),
        version: "0.1.0".to_string(),
        databases: vec![DEFAULT_IN_MEMORY.name().to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "test".to_string(),
            write: "test".to_string(),
        },
        policies: Some(SchemaPolicies {
            read: Some(SchemaPolicyRules {
                allow: vec![rule("OWNER", &OWNER_BY_USER_FIELD)],
                ..SchemaPolicyRules::default()
            }),
            create: Some(SchemaPolicyRules {
                allow: vec![rule("AUTH", &AUTHENTICATED)],
                ..SchemaPolicyRules::default()
            }),
            ..SchemaPolicies::default()
        }),
        fields: vec![field("id", true), field("user", false)],
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        retired_fields: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        partition_export: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }));
    Box::leak(Box::new(SchemaMetadata::from_schema(schema)))
}

fn table_meta<const I: usize>() -> &'static SchemaMetadata {
    static METAS: [OnceLock<&'static SchemaMetadata>; TABLES.len()] =
        [const { OnceLock::new() }; TABLES.len()];
    METAS[I].get_or_init(|| owner_schema(TABLES[I]))
}

valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<0>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<1>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<2>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<3>) }

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Reads large SQLite tables page by page so the suite can check pending-deletion filtering and resumed paging. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

fn row_id(i: usize) -> String {
    format!("r{i:05}")
}

/// `rows` rows `r00000..`; `owned(i)` decides which alice owns. Rows in `pending` are
/// queued for deletion. Returns alice's session.
async fn seed(table: &str, rows: usize, owned: fn(usize) -> bool, pending: &[usize]) -> Valence {
    assert!(TABLES.contains(&table));
    let sqlite = SqliteBackend::connect_memory()
        .await
        .expect("connect sqlite");
    let col = |name: &str, primary_key: bool| LayoutField {
        name: name.into(),
        storage: FieldStorage::String,
        primary_key,
        nullable: !primary_key,
        unique: primary_key,
        indexed: false,
        default: None,
        record_table: None,
    };
    sqlite
        .ensure_typed_table(&StorageLayout {
            table: table.into(),
            fields: vec![col("id", true), col("user", false)],
            retired: Vec::new(),
        })
        .await
        .expect("ensure table");
    let backend: Arc<dyn DatabaseBackend> = Arc::new(sqlite);
    for i in 0..rows {
        let owner = if owned(i) { "alice" } else { "bob" };
        backend
            .create_record(table, serde_json::json!({"id": row_id(i), "user": owner}))
            .await
            .unwrap();
    }
    let v = Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build");
    for &i in pending {
        OwnershipService::mark_pending_deletion(table, &row_id(i), &v)
            .await
            .expect("mark pending");
    }
    v
}

fn ids(rows: &[serde_json::Value]) -> Vec<String> {
    rows.iter()
        .map(|r| {
            let id = &r["id"];
            id.as_str()
                .map(str::to_string)
                .or_else(|| id["id"].as_str().map(str::to_string))
                .unwrap_or_else(|| panic!("row without id: {r}"))
        })
        .map(|id| id.rsplit(':').next().unwrap_or_default().to_string())
        .collect()
}

fn ordered(table: &str) -> QueryCore {
    QueryCore::new(table.to_string()).order_by("id".to_string(), SortDirection::Asc)
}

/// Page through `table` with `resume_at`, returning every id, the raw rows read, and
/// the number of pages.
async fn read_resumed(v: &Valence, table: &str, limit: u32) -> (Vec<String>, usize, usize) {
    let mut all = Vec::new();
    let mut fetched = 0;
    let mut resume = None;
    for page in 1..10_000 {
        let mut q = ordered(table).limit(limit);
        if let Some(at) = resume {
            q = q.resume_at(at);
        }
        let (rows, stats) = q
            .execute_with_stats::<serde_json::Value>(v, purpose())
            .await
            .unwrap();
        all.extend(ids(&rows));
        fetched += stats.fetched;
        match stats.resume {
            Some(at) => resume = Some(at),
            None => return (all, fetched, page),
        }
    }
    panic!("resumed paging never reached the end of {table}");
}

#[tokio::test]
async fn pending_lookup_over_1000_ids_happy() {
    let pending = [100, 999, 1400];
    let v = seed("ls_lookup", 1500, |_| true, &pending).await;
    let all: Vec<String> = (0..1500).map(row_id).collect();
    let found = OwnershipService::pending_deletion_bare_ids_subset("ls_lookup", &all, &v)
        .await
        .expect("a lookup over 1500 ids stays under SQLite's expression depth");
    let want: HashSet<String> = pending.iter().map(|&i| row_id(i)).collect();
    assert_eq!(found, want);
}

#[tokio::test]
async fn full_window_drops_pending_rows_happy() {
    let pending = [3, 500, 1001];
    let v = seed("ls_window", 1500, |_| true, &pending).await;
    let rows: Vec<serde_json::Value> = ordered("ls_window")
        .limit(MAX_QUERY_LIMIT)
        .execute(&v, purpose())
        .await
        .expect("a 1000-row window passes the pending-deletion filter");
    let got = ids(&rows);
    assert_eq!(got.len(), MAX_QUERY_LIMIT as usize);
    assert!(!got.contains(&row_id(3)) && !got.contains(&row_id(500)));
    // Two pending rows inside the first 1002 leave r01001 pending and r01002 last.
    assert_eq!(got.last(), Some(&row_id(1002)));
}

#[tokio::test]
async fn resume_pages_linearly_happy() {
    let owned = |i: usize| i % 3 != 1;
    let pending = [0, 999, 1000, 2400];
    let v = seed("ls_resume", 2500, owned, &pending).await;
    let want: Vec<String> = (0..2500)
        .filter(|&i| owned(i) && !pending.contains(&i))
        .map(row_id)
        .collect();

    let (got, fetched, pages) = read_resumed(&v, "ls_resume", MAX_QUERY_LIMIT).await;
    assert_eq!(got, want, "every readable row once, in order");
    // A page that fills partway through a backend window re-reads the rest of that
    // window next time, so each page costs at most one window more than its rows.
    assert!(
        fetched <= 2500 + pages * MAX_QUERY_LIMIT as usize,
        "resumed paging stays linear: {fetched} raw rows over {pages} pages"
    );

    // Offset paging returns the same rows, by re-reading from row 0 on each call.
    let mut by_offset = Vec::new();
    let mut offset_fetched = 0;
    let mut offset = 0;
    loop {
        let (rows, stats) = ordered("ls_resume")
            .offset(offset)
            .limit(MAX_QUERY_LIMIT)
            .execute_with_stats::<serde_json::Value>(&v, purpose())
            .await
            .unwrap();
        offset_fetched += stats.fetched;
        if rows.is_empty() {
            break;
        }
        offset += u32::try_from(rows.len()).unwrap();
        by_offset.extend(ids(&rows));
    }
    assert_eq!(by_offset, want);
    assert!(
        fetched < offset_fetched,
        "resuming reads fewer raw rows ({fetched}) than offset paging ({offset_fetched})"
    );
}

#[tokio::test]
async fn resume_mid_window_keeps_unreturned_rows_edge() {
    // A page of 7 fills partway through a backend window; the next page must start at
    // the first readable row it didn't return, not at the end of the window.
    let owned = |i: usize| !i.is_multiple_of(4);
    let pending = [5, 6, 1200];
    let v = seed("ls_resume_small", 1300, owned, &pending).await;
    let want: Vec<String> = (0..1300)
        .filter(|&i| owned(i) && !pending.contains(&i))
        .map(row_id)
        .collect();
    let (got, _, _) = read_resumed(&v, "ls_resume_small", 7).await;
    assert_eq!(got, want);
}
