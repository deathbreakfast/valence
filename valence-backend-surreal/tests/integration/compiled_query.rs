//! Compiled query tests for embedded Surreal backend.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use surrealdb::engine::local::Mem;
use valence_backend_surreal::{extract_id_from_select_value, SDb, SurrealEmbeddedBackend};
use valence_core::compiled_query::CompiledQuery;
use valence_core::DatabaseBackend;

async fn backend() -> SurrealEmbeddedBackend {
    let db = SDb::init();
    db.connect::<Mem>(()).await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealEmbeddedBackend::new(db)
}

#[tokio::test]
async fn select_value_id_returns_scalar() {
    let b = backend().await;
    b.upsert_record(
        "permission_group",
        "pg_unique_check",
        serde_json::json!({
            "name": "unique_name_for_select_value_test",
            "description": "d",
        }),
    )
    .await
    .expect("upsert");

    let compiled = CompiledQuery {
        query_string: "SELECT VALUE id FROM permission_group WHERE name = $value LIMIT 2"
            .to_string(),
        params: vec![(
            "value".to_string(),
            serde_json::json!("unique_name_for_select_value_test"),
        )],
    };
    let rows = b.execute_compiled_query(&compiled).await.expect("query");
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].is_string(),
        "expected scalar id string, got: {:?}",
        rows[0]
    );
    let id_str = extract_id_from_select_value(&rows[0]).expect("parse id");
    assert_eq!(id_str, "pg_unique_check");
}

#[tokio::test]
async fn starts_with_filter_order_limit_runs() {
    let b = backend().await;
    for name in ["cx-00001", "cx-00002", "other"] {
        b.create_record("project", serde_json::json!({ "id": name, "name": name }))
            .await
            .expect("create");
    }

    let compiled = CompiledQuery {
        query_string: "SELECT * FROM project WHERE string::starts_with(name, $param_0) ORDER BY name DESC LIMIT 25 START 0".to_string(),
        params: vec![("param_0".to_string(), serde_json::json!("cx-"))],
    };
    let rows = b.execute_compiled_query(&compiled).await.expect("query");
    assert_eq!(rows.len(), 2, "expected two cx-* rows, got {rows:?}");
}

#[tokio::test]
async fn missing_table_select_count_returns_empty() {
    let b = backend().await;
    let compiled = CompiledQuery {
        query_string: "SELECT count() AS count FROM permission_allowed_principal WHERE `in` = type::record($from_tb, $from_id) AND `out` = type::record($to_tb, $to_id)".to_string(),
        params: vec![
            ("from_tb".to_string(), serde_json::json!("permission")),
            ("from_id".to_string(), serde_json::json!("p1")),
            ("to_tb".to_string(), serde_json::json!("permission_user_principal")),
            ("to_id".to_string(), serde_json::json!("user:member")),
        ],
    };
    let rows = b
        .execute_compiled_query(&compiled)
        .await
        .expect("missing table select count");
    assert!(rows.is_empty() || rows[0].get("count").and_then(|v| v.as_i64()) == Some(0));
}
