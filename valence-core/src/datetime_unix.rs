//! Persist [`chrono::DateTime<Utc>`](chrono::DateTime) as signed **UTC unix seconds**.
//!
//! # Storage contract
//!
//! - Wire JSON: a JSON number (`i64`) of seconds since the Unix epoch (not milliseconds).
//! - Model API: only `chrono::DateTime<chrono::Utc>` (never raw integers on the public surface).
//!
//! On deserialize, an RFC3339 string is accepted as a best-effort read tolerance for
//! legacy rows. This helper only round-trips datetime fields.

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Deserializer, Serializer};

/// Serialize `DateTime<Utc>` as unix seconds.
pub fn serialize<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_i64(dt.timestamp())
}

/// Deserialize unix seconds (or legacy RFC3339 string) into `DateTime<Utc>`.
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
        Wire::Secs(secs) => Utc
            .timestamp_opt(secs, 0)
            .single()
            .ok_or_else(|| serde::de::Error::custom(format!("invalid unix seconds: {secs}"))),
        Wire::Rfc3339(s) => DateTime::parse_from_rfc3339(&s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(serde::de::Error::custom),
    }
}

/// Serde helpers for `Option<DateTime<Utc>>`.
pub mod option {
    use super::*;

    pub fn serialize<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(dt) => serializer.serialize_some(&dt.timestamp()),
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
                let secs = n
                    .as_i64()
                    .ok_or_else(|| serde::de::Error::custom("datetime unix seconds must be i64"))?;
                Utc.timestamp_opt(secs, 0)
                    .single()
                    .map(Some)
                    .ok_or_else(|| {
                        serde::de::Error::custom(format!("invalid unix seconds: {secs}"))
                    })
            }
            serde_json::Value::String(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| Some(dt.with_timezone(&Utc)))
                .map_err(serde::de::Error::custom),
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
    fn round_trip_seconds() {
        let dt = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        let row = Row { at: dt };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["at"], json!(1_700_000_000i64));
        let back: Row = serde_json::from_value(v).unwrap();
        assert_eq!(back.at, dt);
    }

    #[test]
    fn accepts_rfc3339_on_read() {
        let v = json!({ "at": "2023-11-14T22:13:20Z" });
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
        let dt = Utc.timestamp_opt(-1, 0).unwrap();
        let row = Row { at: dt };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["at"], json!(-1i64));
        let back: Row = serde_json::from_value(v).unwrap();
        assert_eq!(back.at, dt);
    }
}
