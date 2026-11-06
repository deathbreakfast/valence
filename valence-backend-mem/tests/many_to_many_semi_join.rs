//! ManyToMany semi-joins (`where_connection_exists_many_to_many`,
//! `where_connection_contains_many_to_many`) evaluate against the in-memory edge store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use valence_backend_mem::InMemoryBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::query::{QueryCore, RecordPredicate, StringPredicate};
use valence_core::{DatabaseBackend, RecordId, Valence};

const DOC: &str = "m2m_doc";
const TAG: &str = "m2m_tag";
const EDGE: &str = "m2m_tagged";

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Reads documents linked to tags in an in-memory fixture so the suite can check ManyToMany semi-join filtering. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

/// Register `EDGE` as a ManyToMany edge table so the query compiler accepts it.
fn register_edge() {
    use valence_core::{
        Schema, SchemaConnection, SchemaMeta, SchemaMetadataStruct, SchemaPrivacy, SchemaRegistry,
    };
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let schema = Box::leak(Box::new(Schema {
            name: DOC.to_string(),
            version: "1.0.0".to_string(),
            databases: vec!["default".to_string()],
            database_evaluator: &valence_core::evaluator::DEFAULT_IN_MEMORY,
            privacy: SchemaPrivacy {
                read: "public".to_string(),
                write: "service".to_string(),
            },
            policies: None,
            fields: Vec::new(),
            edges: Vec::new(),
            connections: vec![SchemaConnection {
                name: "tags".to_string(),
                from_table: DOC.to_string(),
                from_field: "id".to_string(),
                to_table: TAG.to_string(),
                cardinality: "ManyToMany".to_string(),
                required: false,
                on_delete: "Cascade".to_string(),
                label: "tags".to_string(),
                model_path: None,
                reverse_field: None,
                edge_table: Some(EDGE.to_string()),
                edge_policy: None,
                target_trait: None,
            }],
            side_effects: Vec::new(),
            iters: Vec::new(),
            composite_key: Vec::new(),
            traits: Vec::new(),
            retired_fields: Vec::new(),
            ttl: None,
            ownership: None,
            meta: SchemaMeta {
                retention: "365 days".to_string(),
                row_count: 0,
                owner: "system".to_string(),
                description: None,
                repository: "https://github.com/unified-field-dev/valence".to_string(),
            },
        }));
        SchemaRegistry::register_overlay(Box::leak(Box::new(SchemaMetadataStruct::from_schema(
            schema,
        ))));
    });
}

async fn seeded() -> Valence {
    register_edge();
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    backend.ensure_schemaless_table(DOC).await.unwrap();
    backend.ensure_schemaless_table(TAG).await.unwrap();
    for (id, owner) in [("d1", "ann"), ("d2", "ann"), ("d3", "bob"), ("d4", "bob")] {
        backend
            .create_record(DOC, serde_json::json!({"id": id, "owner": owner}))
            .await
            .unwrap();
    }
    for (id, label) in [("t1", "red"), ("t2", "blue"), ("t3", "green")] {
        backend
            .create_record(TAG, serde_json::json!({"id": id, "label": label}))
            .await
            .unwrap();
    }
    for (doc, tag) in [("d1", "t1"), ("d2", "t2"), ("d3", "t1"), ("d3", "t2")] {
        backend
            .relate_edge(&RecordId::new(DOC, doc), EDGE, &RecordId::new(TAG, tag))
            .await
            .unwrap();
    }
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build")
}

/// Ids the backend returns for `q` before any read-privacy filtering, with the compiled SQL.
async fn raw_ids(v: &Valence, q: &QueryCore) -> (Vec<String>, String) {
    let backend = v.backend_for_table(DOC).unwrap();
    let compiled = valence_core::compile_for_engine(backend.engine_id(), q).unwrap();
    let rows = backend.execute_compiled_query(&compiled).await.unwrap();
    let mut ids: Vec<String> = rows
        .iter()
        .filter_map(|r| r["id"].as_str().map(str::to_string))
        .collect();
    ids.sort();
    (ids, compiled.query_string)
}

async fn ids(v: &Valence, q: QueryCore) -> Vec<String> {
    let (raw, sql) = raw_ids(v, &q).await;
    let rows: Vec<serde_json::Value> = q.execute(v, purpose()).await.unwrap();
    let mut ids: Vec<String> = rows
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(
        raw, ids,
        "read privacy changed the backend rows; sql: {sql}"
    );
    ids
}

fn tag_label(label: &str) -> QueryCore {
    QueryCore::new(TAG.to_string())
        .where_string("label".into(), StringPredicate::Equals(label.into()))
}

fn docs() -> QueryCore {
    QueryCore::new(DOC.to_string())
}

#[tokio::test]
async fn exists_returns_rows_linked_to_a_matching_target() {
    let v = seeded().await;
    let q = docs().where_connection_exists_many_to_many(EDGE.into(), TAG.into(), tag_label("red"));
    assert_eq!(ids(&v, q).await, ["d1", "d3"]);
}

#[tokio::test]
async fn exists_with_no_matching_target_returns_nothing() {
    let v = seeded().await;
    let q =
        docs().where_connection_exists_many_to_many(EDGE.into(), TAG.into(), tag_label("green"));
    assert_eq!(ids(&v, q).await, Vec::<String>::new());
}

#[tokio::test]
async fn contains_returns_rows_linked_to_the_record() {
    let v = seeded().await;
    let q = docs().where_connection_contains_many_to_many(EDGE.into(), RecordId::new(TAG, "t2"));
    assert_eq!(ids(&v, q).await, ["d2", "d3"]);
}

#[tokio::test]
async fn semi_join_composes_with_an_ordinary_predicate() {
    let v = seeded().await;
    let q = docs()
        .where_string("owner".into(), StringPredicate::Equals("bob".into()))
        .where_connection_exists_many_to_many(EDGE.into(), TAG.into(), tag_label("red"));
    assert_eq!(ids(&v, q).await, ["d3"]);
}

#[tokio::test]
async fn semi_join_composes_with_a_record_or_group() {
    let v = seeded().await;
    let q = docs()
        .where_record(
            "id".into(),
            RecordPredicate::Equals(RecordId::new(DOC, "d3")),
        )
        .where_connection_contains_many_to_many(EDGE.into(), RecordId::new(TAG, "t1"));
    assert_eq!(ids(&v, q).await, ["d3"]);
}
