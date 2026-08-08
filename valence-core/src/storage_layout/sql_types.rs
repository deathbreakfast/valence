//! Logical schema type strings → physical storage kinds.

use crate::error::{Error, Result};

/// Physical storage kind for one field (engine-agnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldStorage {
    /// UTF-8 text / record id string.
    String,
    /// Signed integer (also DateTime unix seconds).
    Integer,
    /// Decimal / floating numeric.
    Decimal,
    /// Boolean.
    Boolean,
    /// Calendar date as text `YYYY-MM-DD`.
    Date,
    /// JSON document cell (SQL JSON/JSONB, Surreal object, etc.).
    Json,
    /// Currency composite stored as JSON cell.
    Currency,
}

/// SQLite / Postgres column type fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlColumnType {
    Text,
    Integer,
    Real,
    /// SQLite TEXT holding JSON; Postgres JSONB.
    Json,
}

impl FieldStorage {
    /// Map to SQLite column type.
    #[must_use]
    pub const fn sqlite_type(self) -> SqlColumnType {
        match self {
            Self::String | Self::Date => SqlColumnType::Text,
            Self::Integer | Self::Boolean => SqlColumnType::Integer,
            Self::Decimal => SqlColumnType::Real,
            Self::Json | Self::Currency => SqlColumnType::Json,
        }
    }

    /// SQLite DDL type name.
    #[must_use]
    pub const fn sqlite_ddl(self) -> &'static str {
        match self.sqlite_type() {
            SqlColumnType::Text | SqlColumnType::Json => "TEXT",
            SqlColumnType::Integer => "INTEGER",
            SqlColumnType::Real => "REAL",
        }
    }

    /// Postgres DDL type name.
    #[must_use]
    pub const fn postgres_ddl(self) -> &'static str {
        match self {
            Self::String | Self::Date => "TEXT",
            Self::Integer => "BIGINT",
            Self::Boolean => "BOOLEAN",
            Self::Decimal => "DOUBLE PRECISION",
            Self::Json | Self::Currency => "JSONB",
        }
    }
}

/// Surreal `DEFINE FIELD` type fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurrealFieldType {
    String,
    Int,
    Float,
    Bool,
    Datetime,
    Object,
    Any,
}

impl FieldStorage {
    /// Surreal field type.
    #[must_use]
    pub const fn surreal_type(self) -> SurrealFieldType {
        match self {
            Self::String | Self::Date => SurrealFieldType::String,
            Self::Integer => SurrealFieldType::Int,
            Self::Decimal => SurrealFieldType::Float,
            Self::Boolean => SurrealFieldType::Bool,
            Self::Json | Self::Currency => SurrealFieldType::Object,
        }
    }

    /// Surreal DDL type keyword.
    #[must_use]
    pub const fn surreal_ddl(self) -> &'static str {
        match self.surreal_type() {
            SurrealFieldType::String => "string",
            SurrealFieldType::Int => "int",
            SurrealFieldType::Float => "float",
            SurrealFieldType::Bool => "bool",
            SurrealFieldType::Datetime => "datetime",
            SurrealFieldType::Object => "object",
            SurrealFieldType::Any => "any",
        }
    }
}

/// Map `SchemaField.field_type` / [`crate::FieldType::as_str`] to [`FieldStorage`].
///
/// # Errors
///
/// Returns [`Error::Validation`] for unrecognized type strings.
pub fn logical_type_to_storage(field_type: &str) -> Result<FieldStorage> {
    let base = field_type
        .split('<')
        .next()
        .unwrap_or(field_type)
        .trim()
        .to_ascii_lowercase();
    match base.as_str() {
        "string" | "enum" => Ok(FieldStorage::String),
        "integer" | "int" | "datetime" => Ok(FieldStorage::Integer),
        "decimal" | "float" | "number" => Ok(FieldStorage::Decimal),
        "boolean" | "bool" => Ok(FieldStorage::Boolean),
        "date" => Ok(FieldStorage::Date),
        "json" => Ok(FieldStorage::Json),
        "currency" => Ok(FieldStorage::Currency),
        // Record links persist as the canonical `table:id` string so equality works
        // uniformly across engines (no engine-specific JSON extraction on FK columns).
        "record" => Ok(FieldStorage::String),
        other if other.starts_with("record") => Ok(FieldStorage::String),
        other => Err(Error::Validation(format!(
            "unknown field type for storage layout: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_record_generic() {
        assert_eq!(
            logical_type_to_storage("record<user>").unwrap(),
            FieldStorage::String
        );
    }

    #[test]
    fn maps_datetime_to_integer_unix() {
        assert_eq!(
            logical_type_to_storage("datetime").unwrap(),
            FieldStorage::Integer
        );
    }
}
