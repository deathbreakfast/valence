//! `QueryCore::sum_values` on SQLite: totals past one query window, grouping, filters,
//! currency amounts, and read privacy.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, OnceLock};

use serde_json::json;
use valence_backend_sqlite::SqliteBackend;
use valence_core::actor::Actor;
use valence_core::data_use::DataUsePurpose;
use valence_core::evaluator::DEFAULT_IN_MEMORY;
use valence_core::privacy_policies::common::AUTHENTICATED;
use valence_core::privacy_policies::owner::OWNER_BY_USER_FIELD;
use valence_core::query::{QueryCore, StringPredicate};
use valence_core::schema::{SchemaMetadata, SchemaMetadataInit};
use valence_core::schema_api::{
    Schema, SchemaField, SchemaMeta, SchemaPolicies, SchemaPolicyRule, SchemaPolicyRules,
    SchemaPrivacy,
};
use valence_core::{CurrencyCode, DatabaseBackend, DatabaseEvaluator, Error, Valence};

const TABLES: [&str; 3] = ["sum_lines", "sum_mixed", "sum_text"];

fn field(name: &str, field_type: &str) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        field_type: field_type.to_string(),
        primary: name == "id",
        nullable: name != "id",
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

fn allow(name: &str, eval: &'static valence_core::privacy::PrivacyRule) -> SchemaPolicyRules {
    SchemaPolicyRules {
        allow: vec![SchemaPolicyRule {
            name: name.to_string(),
            description: None,
            evaluator: Some(eval),
        }],
        ..SchemaPolicyRules::default()
    }
}

/// Lines readable by their `user` only, with an integer and a currency amount.
fn line_schema(table: &str) -> &'static SchemaMetadata {
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
            read: Some(allow("OWNER", &OWNER_BY_USER_FIELD)),
            create: Some(allow("AUTH", &AUTHENTICATED)),
            ..SchemaPolicies::default()
        }),
        fields: vec![
            field("id", "string"),
            field("user", "string"),
            field("account", "string"),
            field("units", "integer"),
            field("amount", "currency"),
            field("memo", "string"),
        ],
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
    METAS[I].get_or_init(|| line_schema(TABLES[I]))
}

valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<0>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<1>) }
valence_core::inventory::submit! { SchemaMetadataInit(table_meta::<2>) }

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Sums seeded ledger lines so the suite can check totals against values computed in the test. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

async fn valence_for(rows: Vec<serde_json::Value>, table: &str) -> Valence {
    let backend: Arc<dyn DatabaseBackend> =
        Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    for row in rows {
        backend.create_record(table, row).await.unwrap();
    }
    Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build")
}

/// 2,500 lines over accounts `a0..a2`; alice owns lines whose index is not a multiple of 4.
fn line(i: i64) -> serde_json::Value {
    json!({
        "id": format!("l{i:05}"),
        "user": if i % 4 == 0 { "bob" } else { "alice" },
        "account": format!("a{}", i % 3),
        "units": i,
        "amount": {"code": "USD", "amount_minor": i * 10},
        "memo": "x",
    })
}

#[tokio::test]
async fn sum_values_groups_past_one_window_and_hides_unreadable_rows() {
    let v = valence_for((0..2500).map(line).collect(), "sum_lines").await;
    let readable = || (0..2500).filter(|i| i % 4 != 0);

    let total = QueryCore::new("sum_lines".into())
        .sum_values("units", None, &v, purpose())
        .await
        .unwrap();
    assert_eq!(total.len(), 1);
    assert_eq!(total[0].key, None);
    assert_eq!(total[0].sum, readable().sum::<i64>());
    assert_eq!(total[0].rows, readable().count() as u64);
    assert_eq!(total[0].currency, None);

    let by_account = QueryCore::new("sum_lines".into())
        .sum_values("amount", Some("account"), &v, purpose())
        .await
        .unwrap();
    assert_eq!(by_account.len(), 3);
    for group in &by_account {
        let account: i64 = group.key.as_deref().unwrap()[1..].parse().unwrap();
        let expected: i64 = readable()
            .filter(|i| i % 3 == account)
            .map(|i| i * 10)
            .sum();
        assert_eq!(group.sum, expected, "account a{account}");
        assert_eq!(group.currency, Some(CurrencyCode::Usd));
    }

    let filtered = QueryCore::new("sum_lines".into())
        .where_string("account".into(), StringPredicate::Equals("a1".into()))
        .sum_values("units", Some("account"), &v, purpose())
        .await
        .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(
        filtered[0].sum,
        readable().filter(|i| i % 3 == 1).sum::<i64>()
    );
}

#[tokio::test]
async fn sum_values_rejects_mixed_currencies_in_a_group() {
    let mut rows: Vec<_> = (1..4).map(line).collect();
    rows.push(json!({
        "id": "eur", "user": "alice", "account": "a1", "units": 1,
        "amount": {"code": "EUR", "amount_minor": 5}, "memo": "x",
    }));
    let v = valence_for(rows, "sum_mixed").await;
    let err = QueryCore::new("sum_mixed".into())
        .sum_values("amount", Some("account"), &v, purpose())
        .await
        .expect_err("USD and EUR in account a1");
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");
}

#[tokio::test]
async fn sum_values_rejects_non_numeric_fields() {
    let v = valence_for((1..3).map(line).collect(), "sum_text").await;
    let err = QueryCore::new("sum_text".into())
        .sum_values("memo", None, &v, purpose())
        .await
        .expect_err("memo is text");
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");
    let err = QueryCore::new("sum_text".into())
        .sum_values("units; DROP", None, &v, purpose())
        .await
        .expect_err("unsafe field name");
    assert!(matches!(err, Error::Validation(_)), "got {err:?}");
}
