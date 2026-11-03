//! Backend round-trips for JsonAs, Currency, and DateTime unix-seconds shapes.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use valence_backend_mem::InMemoryBackend;
use valence_core::backend::DatabaseBackend;
use valence_core::currency::{Currency, CurrencyCode};
use valence_core::schema_types::JsonAsSerdeError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Payload {
    n: i64,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TypedRow {
    id: String,
    #[serde(serialize_with = "ser_payload", deserialize_with = "de_payload")]
    payload: Payload,
    price: Currency,
    #[serde(with = "valence_core::datetime_unix")]
    at: chrono::DateTime<Utc>,
}

fn ser_payload<S: serde::Serializer>(value: &Payload, serializer: S) -> Result<S::Ok, S::Error> {
    valence_core::json_as::serialize(
        value,
        serializer,
        "typed_row",
        "payload",
        "crate::Payload",
        JsonAsSerdeError::Error,
    )
}

fn de_payload<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Payload, D::Error> {
    valence_core::json_as::deserialize(
        deserializer,
        "typed_row",
        "payload",
        "crate::Payload",
        JsonAsSerdeError::Error,
    )
}

#[tokio::test]
async fn mem_round_trip_json_as_currency_datetime() {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    backend.ensure_schemaless_table("typed_row").await.unwrap();

    let at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let row = TypedRow {
        id: "r1".into(),
        payload: Payload {
            n: 7,
            label: "ok".into(),
        },
        price: Currency::new(CurrencyCode::Usd, 12345),
        at,
    };
    let created = backend
        .create_record("typed_row", serde_json::to_value(&row).unwrap())
        .await
        .unwrap();

    assert_eq!(created["at"], serde_json::json!(1_700_000_000i64));
    assert_eq!(
        created["price"],
        serde_json::json!({ "code": "USD", "amount_minor": 12345 })
    );
    assert_eq!(created["payload"]["n"], serde_json::json!(7));

    let fetched = backend
        .get_record("typed_row", "r1")
        .await
        .unwrap()
        .expect("exists");
    let back: TypedRow = serde_json::from_value(fetched).unwrap();
    assert_eq!(back, row);
}

#[tokio::test]
async fn mem_json_as_error_includes_field_context() {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    backend.ensure_schemaless_table("typed_row").await.unwrap();
    backend
        .create_record(
            "typed_row",
            serde_json::json!({
                "id": "bad",
                "payload": "not-an-object",
                "price": { "code": "USD", "amount_minor": 1 },
                "at": 0
            }),
        )
        .await
        .unwrap();
    let fetched = backend
        .get_record("typed_row", "bad")
        .await
        .unwrap()
        .unwrap();
    let err = serde_json::from_value::<TypedRow>(fetched).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("typed_row.payload"), "{msg}");
    assert!(msg.contains("crate::Payload"), "{msg}");
}
