//! Backend-agnostic record identifier (`table` + `id`).

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordId {
    table: String,
    id: String,
}

impl RecordId {
    #[must_use]
    pub fn new(table: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            id: id.into(),
        }
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Parse `table:id` wire form (Surreal / SQL document adapters).
    pub fn parse(s: &str) -> Option<Self> {
        let (table, id) = s.split_once(':')?;
        if table.is_empty() || id.is_empty() {
            return None;
        }
        Some(Self::new(table, id))
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.table, self.id)
    }
}

impl Serialize for RecordId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RecordId", 2)?;
        state.serialize_field("table", &self.table)?;
        state.serialize_field("id", &self.id)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for RecordId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RecordIdVisitor;

        impl<'de> Visitor<'de> for RecordIdVisitor {
            type Value = RecordId;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("RecordId object or \"table:id\" string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                RecordId::parse(v).ok_or_else(|| E::custom(format!("invalid RecordId string: {v}")))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_str(&v)
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut table: Option<String> = None;
                let mut id: Option<String> = None;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "table" | "tb" => table = Some(map.next_value::<String>()?),
                        "id" => id = Some(map.next_value::<String>()?),
                        _ => {
                            let _: de::IgnoredAny = map.next_value()?;
                        }
                    }
                }
                let table = table.ok_or_else(|| de::Error::missing_field("table"))?;
                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                Ok(RecordId::new(table, id))
            }
        }

        deserializer.deserialize_any(RecordIdVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_object_and_colon_string() {
        let obj: RecordId = serde_json::from_str(r#"{"table":"task","id":"1"}"#).expect("object");
        assert_eq!(obj.table(), "task");
        assert_eq!(obj.id(), "1");

        let s: RecordId = serde_json::from_str(r#""task:1""#).expect("string");
        assert_eq!(s.table(), "task");
        assert_eq!(s.id(), "1");
    }
}
