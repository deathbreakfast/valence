//! Persist [`chrono::DateTime<Utc>`](chrono::DateTime) as signed **UTC unix milliseconds**.
//!
//! # Storage contract
//!
//! - Wire JSON: a JSON number (`i64`) of milliseconds since the Unix epoch. Earlier
//!   releases wrote whole seconds, which left rows written in the same second with no
//!   guaranteed order.
//! - Model API: only `chrono::DateTime<chrono::Utc>` (never raw integers on the public surface).
//!
//! # Reading legacy seconds
//!
//! A stored integer whose magnitude is below [`LEGACY_SECONDS_LIMIT`] (`1e11`) is read as
//! legacy unix **seconds**; anything larger is read as milliseconds. In the millisecond
//! reading the cutoff falls in March 1973, so an instant before that written in
//! milliseconds reads back as seconds. SQL predicates always compare against milliseconds,
//! so a legacy seconds row mis-orders in `DateTimePredicate` filters until it is rewritten.
//!
//! On deserialize, digit-only strings (SQLite INTEGER→TEXT coercion) and RFC3339
//! strings are accepted as best-effort read tolerance. This helper only round-trips
//! datetime fields.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serializer};

/// Integers with a magnitude below this are legacy unix seconds, not milliseconds.
pub const LEGACY_SECONDS_LIMIT: i64 = 100_000_000_000;

/// Convert a stored integer (milliseconds, or legacy seconds) to `DateTime<Utc>`.
fn from_stored_int<E: serde::de::Error>(n: i64) -> Result<DateTime<Utc>, E> {
    let parsed = if n.unsigned_abs() < LEGACY_SECONDS_LIMIT.unsigned_abs() {
        Utc.timestamp_opt(n, 0).single()
    } else {
        Utc.timestamp_millis_opt(n).single()
    };
    parsed.ok_or_else(|| E::custom(format!("invalid unix timestamp: {n}")))
}

/// Serialize `DateTime<Utc>` as unix milliseconds.
pub fn serialize<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(dt.timestamp_millis())
}

/// Deserialize unix milliseconds, legacy seconds, or an RFC3339 string into `DateTime<Utc>`.
pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_impl(deserializer)
}

fn deserialize_impl<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Wire {
        Secs(i64),
        Rfc3339(String),
    }

    match Wire::deserialize(deserializer)? {
        Wire::Secs(n) => from_stored_int(n),
        Wire::Rfc3339(s) => parse_datetime_string(&s),
    }
}

/// Accept unix digit strings (SQLite INTEGER→TEXT coercion) or RFC3339.
fn parse_datetime_string<E: serde::de::Error>(s: &str) -> Result<DateTime<Utc>, E> {
    let trimmed = s.trim();
    if !trimmed.is_empty()
        && trimmed
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_digit() || (i == 0 && b == b'-'))
    {
        if let Ok(n) = trimmed.parse::<i64>() {
            return from_stored_int(n);
        }
    }
    DateTime::parse_from_rfc3339(trimmed)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(E::custom)
}

/// Serde helpers for `Option<DateTime<Utc>>`.
pub mod option {
    use super::*;

    pub fn serialize<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(dt) => serializer.serialize_some(&dt.timestamp_millis()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<serde_json::Value>::deserialize(deserializer)?.map_or(Ok(None), |v| match v {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::Number(n) => {
                let n = n.as_i64().ok_or_else(|| {
                    serde::de::Error::custom("datetime unix timestamp must be i64")
                })?;
                from_stored_int(n).map(Some)
            }
            serde_json::Value::String(s) => parse_datetime_string(&s).map(Some),
            other => Err(serde::de::Error::custom(format!(
                "expected unix seconds or RFC3339 string, got {other}"
            ))),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Row {
        #[serde(with = "crate::datetime_unix")]
        at: DateTime<Utc>,
    }

    #[test]
    fn round_trip_millis() {
        let dt = Utc.timestamp_millis_opt(1_700_000_000_123).unwrap();
        let row = Row { at: dt };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["at"], json!(1_700_000_000_123i64));
        let back: Row = serde_json::from_value(v).unwrap();
        assert_eq!(back.at, dt);
    }

    #[test]
    fn same_second_values_stay_distinct() {
        let a = Utc.timestamp_millis_opt(1_700_000_000_001).unwrap();
        let b = Utc.timestamp_millis_opt(1_700_000_000_002).unwrap();
        let va = serde_json::to_value(Row { at: a }).unwrap()["at"]
            .as_i64()
            .unwrap();
        let vb = serde_json::to_value(Row { at: b }).unwrap()["at"]
            .as_i64()
            .unwrap();
        assert!(va < vb);
    }

    #[test]
    fn legacy_seconds_number_still_reads() {
        let row: Row = serde_json::from_value(json!({ "at": 1_700_000_000i64 })).unwrap();
        assert_eq!(row.at.timestamp(), 1_700_000_000);
        assert_eq!(row.at.timestamp_subsec_millis(), 0);
    }

    #[test]
    fn legacy_seconds_boundary() {
        let below: Row = serde_json::from_value(json!({ "at": LEGACY_SECONDS_LIMIT - 1 })).unwrap();
        assert_eq!(below.at.timestamp(), LEGACY_SECONDS_LIMIT - 1);
        let at: Row = serde_json::from_value(json!({ "at": LEGACY_SECONDS_LIMIT })).unwrap();
        assert_eq!(at.at.timestamp_millis(), LEGACY_SECONDS_LIMIT);
    }

    #[test]
    fn option_variant_round_trips_and_reads_legacy() {
        #[derive(Debug, PartialEq, Serialize, Deserialize)]
        struct Opt {
            #[serde(default, with = "crate::datetime_unix::option")]
            at: Option<DateTime<Utc>>,
        }
        let dt = Utc.timestamp_millis_opt(1_700_000_000_456).unwrap();
        let v = serde_json::to_value(Opt { at: Some(dt) }).unwrap();
        assert_eq!(v["at"], json!(1_700_000_000_456i64));
        assert_eq!(serde_json::from_value::<Opt>(v).unwrap().at, Some(dt));
        let legacy: Opt = serde_json::from_value(json!({ "at": 1_700_000_000i64 })).unwrap();
        assert_eq!(legacy.at.unwrap().timestamp(), 1_700_000_000);
        let none: Opt = serde_json::from_value(json!({ "at": null })).unwrap();
        assert_eq!(none.at, None);
    }

    #[test]
    fn accepts_rfc3339_on_read() {
        let v = json!({ "at": "2023-11-14T22:13:20Z" });
        let row: Row = serde_json::from_value(v).unwrap();
        assert_eq!(row.at.timestamp(), 1_700_000_000);
    }

    #[test]
    fn accepts_unix_seconds_as_string() {
        // SQLite often yields INTEGER columns via try_get::<String>.
        let v = json!({ "at": "1700000000" });
        let row: Row = serde_json::from_value(v).unwrap();
        assert_eq!(row.at.timestamp(), 1_700_000_000);
    }

    #[test]
    fn rejects_float_seconds() {
        let v = json!({ "at": 1.5 });
        assert!(serde_json::from_value::<Row>(v).is_err());
    }

    #[test]
    fn negative_timestamp() {
        // Before 1970-03-03 in milliseconds the magnitude is above the legacy limit again.
        let dt = Utc.timestamp_millis_opt(-1_700_000_000_123).unwrap();
        let row = Row { at: dt };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["at"], json!(-1_700_000_000_123i64));
        let back: Row = serde_json::from_value(v).unwrap();
        assert_eq!(back.at, dt);
        let legacy: Row = serde_json::from_value(json!({ "at": -1i64 })).unwrap();
        assert_eq!(legacy.at.timestamp(), -1);
    }
}
