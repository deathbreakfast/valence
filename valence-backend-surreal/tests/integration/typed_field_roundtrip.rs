//! Typed field persistence round-trips through Surreal embedded.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{TimeZone, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::engine::local::Mem;
use valence_backend_surreal::{SDb, SurrealEmbeddedBackend};
use valence_core::currency::{Currency, CurrencyCode};
use valence_core::schema_types::JsonAsSerdeError;
use valence_core::DatabaseBackend;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Payload {
    n: i64,
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

async fn backend() -> SurrealEmbeddedBackend {
    let db = SDb::init();
    db.connect::<Mem>(()).await.unwrap();
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealEmbeddedBackend::new(db)
}

#[tokio::test]
async fn surreal_round_trip_json_as_currency_datetime() {
    let b = backend().await;
    let at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let row = TypedRow {
        id: "r1".into(),
        payload: Payload { n: 9 },
        price: Currency::new(CurrencyCode::Eur, 99),
        at,
    };
    let created = b
        .create_record("typed_row", serde_json::to_value(&row).unwrap())
        .await
        .expect("create");
    assert_eq!(
        created.get("at").and_then(|v| v.as_i64()),
        Some(1_700_000_000)
    );
    assert_eq!(
        created.pointer("/price/code").and_then(|v| v.as_str()),
        Some("EUR")
    );

    let fetched = b
        .get_record("typed_row", "r1")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(
        fetched.get("at").and_then(|v| v.as_i64()),
        Some(1_700_000_000),
        "datetime stored as unix seconds: {fetched}"
    );
    assert_eq!(
        fetched.pointer("/price/code").and_then(|v| v.as_str()),
        Some("EUR"),
        "currency code: {fetched}"
    );
    assert_eq!(
        fetched
            .pointer("/price/amount_minor")
            .and_then(|v| v.as_i64()),
        Some(99),
        "currency minor: {fetched}"
    );
    assert_eq!(
        fetched.pointer("/payload/n").and_then(|v| v.as_i64()),
        Some(9),
        "json_as payload: {fetched}"
    );
    // Re-parse payload/price/at (Surreal may reshape `id` as a record object).
    let payload: Payload =
        serde_json::from_value(fetched.get("payload").cloned().unwrap()).expect("payload");
    let price: Currency =
        serde_json::from_value(fetched.get("price").cloned().unwrap()).expect("price");
    let at_back: chrono::DateTime<Utc> = {
        let v = fetched.get("at").cloned().unwrap();
        if let Some(secs) = v.as_i64() {
            Utc.timestamp_opt(secs, 0).unwrap()
        } else {
            panic!("expected unix seconds for at, got {v}");
        }
    };
    assert_eq!(payload, row.payload);
    assert_eq!(price, row.price);
    assert_eq!(at_back, row.at);
}
