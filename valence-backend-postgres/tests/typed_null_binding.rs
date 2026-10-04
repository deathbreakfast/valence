//! Null cells in a registered schema bind with the column's own type, so
//! BIGINT/BOOLEAN/DOUBLE/JSONB columns accept them on create, update, and merge.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::expect_used, clippy::print_stderr)]

use serde_json::{json, Value};
use valence_backend_postgres::PostgresBackend;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::schema::{SchemaMetadata, SchemaRegistry};
use valence_core::schema_api::{Schema, SchemaField, SchemaMeta, SchemaPrivacy};
use valence_core::storage_layout::StorageLayout;
use valence_core::DatabaseBackend;

const TABLE: &str = "typed_null_binding_probe";

/// One nullable column per Postgres column type Valence emits.
const NULLABLE: [(&str, &str); 8] = [
    ("count", "integer"),
    ("seen_at", "datetime"),
    ("flag", "boolean"),
    ("ratio", "decimal"),
    ("doc", "json"),
    ("price", "currency"),
    ("day", "date"),
    ("label", "string"),
];

fn schema_field(name: &str, field_type: &str, primary: bool) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: field_type.to_string(),
        primary,
        nullable: !primary,
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

fn leak_schema() -> &'static Schema {
    let mut fields = vec![schema_field("id", "string", true)];
    fields.extend(NULLABLE.iter().map(|(n, t)| schema_field(n, t, false)));
    Box::leak(Box::new(Schema {
        name: TABLE.to_string(),
        version: "1.0.0".to_string(),
        databases: vec!["default".to_string()],
        database_evaluator: &DEFAULT_IN_MEMORY,
        privacy: SchemaPrivacy {
            read: "public".to_string(),
            write: "service".to_string(),
        },
        policies: None,
        fields,
        edges: Vec::new(),
        connections: Vec::new(),
        side_effects: Vec::new(),
        iters: Vec::new(),
        composite_key: Vec::new(),
        traits: Vec::new(),
        ttl: None,
        ownership: None,
        meta: SchemaMeta {
            retention: "365 days".to_string(),
            row_count: 0,
            owner: "system".to_string(),
            description: None,
            repository: "https://github.com/unified-field-dev/valence".to_string(),
        },
    }))
}

async fn connect() -> Option<PostgresBackend> {
    match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("postgres connect failed: {e} — skipping");
            None
        }
    }
}

fn all_null(id: &str) -> Value {
    let mut row = json!({"id": {"table": TABLE, "id": id}});
    for (name, _) in NULLABLE {
        row[name] = Value::Null;
    }
    row
}

fn assert_nulls(row: &Value) {
    for (name, _) in NULLABLE {
        assert!(
            row.get(name).is_none_or(Value::is_null),
            "{name} should read back null, got {:?}",
            row.get(name)
        );
    }
}

#[tokio::test]
async fn null_cells_bind_with_column_type_happy_path() {
    let Some(backend) = connect().await else {
        return;
    };
    let mut registry = SchemaRegistry::new();
    registry.register(Box::leak(Box::new(SchemaMetadata::from_schema(
        leak_schema(),
    ))));
    SchemaRegistry::set_global(registry);

    reset_row(&backend).await;
    let layout = StorageLayout::from_registry_table(TABLE).expect("layout");
    backend.ensure_typed_table(&layout).await.expect("ensure");

    backend
        .create_record(TABLE, all_null("r1"))
        .await
        .expect("create with null cells");
    let got = backend
        .get_record(TABLE, "r1")
        .await
        .expect("get")
        .expect("row");
    assert_nulls(&got);

    backend
        .update_record(
            TABLE,
            "r1",
            json!({"id": {"table": TABLE, "id": "r1"}, "count": 7, "flag": true, "doc": {"a": 1}}),
        )
        .await
        .expect("update to values");
    backend
        .update_record(TABLE, "r1", all_null("r1"))
        .await
        .expect("update back to null cells");
    backend
        .merge_record(TABLE, "r1", json!({"count": null, "doc": null}))
        .await
        .expect("merge null cells");
    let got = backend
        .get_record(TABLE, "r1")
        .await
        .expect("get")
        .expect("row");
    assert_nulls(&got);
}

/// Start from an empty table so reruns against a shared database stay deterministic.
async fn reset_row(backend: &PostgresBackend) {
    if backend
        .get_record(TABLE, "r1")
        .await
        .ok()
        .flatten()
        .is_some()
    {
        backend.delete_record(TABLE, "r1").await.expect("reset row");
    }
}
