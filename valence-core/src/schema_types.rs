//! DSL type vocabulary used by `valence_schema!` and `valence_trait_schema!`.

/// How `FieldType::JsonAs` handles serde failures on the model boundary.
///
/// Write APIs take the declared Rust type `T`, so serialization is not an expected
/// failure for ordinary `Serialize` impls. Prefer [`Error`](Self::Error) (the default)
/// so hosts receive [`crate::Error::Serialization`]. Reserve [`Panic`](Self::Panic) for
/// explicitly trusted stored JSON / programmer-invariant paths only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonAsSerdeError {
    /// Map serde failures to [`crate::Error::Serialization`] (via serde custom errors
    /// that include table, field, and type path context).
    #[default]
    Error,
    /// Panic with table/field/type context (trusted `T` / trusted stored JSON).
    /// Prefer [`Error`](Self::Error) unless a panic is an intentional invariant.
    Panic,
}

/// Field data types accepted by the Valence DSL.
///
/// # DateTime storage
///
/// [`FieldType::DateTime`] exposes `chrono::DateTime<chrono::Utc>` on the Model API.
/// Persistence uses signed `i64` **UTC unix seconds** since the epoch (not milliseconds).
///
/// # JsonAs
///
/// [`FieldType::JsonAs`] stores JSON and deserializes into an external serde type.
/// Chain [`.serde_error`](FieldType::serde_error) to select panic vs error behavior.
///
/// # Record targets
///
/// [`FieldType::Record`] may be chained with [`.target`](FieldType::target) for an
/// explicit cross-crate model path used by connection helpers and query hops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    String,
    Integer,
    Decimal,
    Boolean,
    DateTime,
    Date,
    Json,
    /// Typed JSON field; default serde policy is [`JsonAsSerdeError::Error`].
    JsonAs(&'static str),
    /// Typed JSON with an explicit serde failure policy.
    JsonAsConfigured {
        type_path: &'static str,
        serde_error: JsonAsSerdeError,
    },
    Record(&'static str),
    /// Record FK with an explicit target model path (cross-crate or same-crate).
    RecordTargeted {
        table: &'static str,
        target: &'static str,
    },
    Enum(&'static [&'static str]),
    ExternalEnum(&'static str),
    /// Composite monetary value (`valence::Currency` / ISO-4217 `CurrencyCode`).
    Currency,
}

impl FieldType {
    /// Logical / DB type name (without cross-crate target or JsonAs policy metadata).
    pub fn as_str(&self) -> String {
        match self {
            FieldType::String => "string".to_string(),
            FieldType::Integer => "integer".to_string(),
            FieldType::Decimal => "decimal".to_string(),
            FieldType::Boolean => "boolean".to_string(),
            FieldType::DateTime => "datetime".to_string(),
            FieldType::Date => "date".to_string(),
            FieldType::Json => "json".to_string(),
            FieldType::JsonAs(_) | FieldType::JsonAsConfigured { .. } => "json".to_string(),
            FieldType::Record(table) | FieldType::RecordTargeted { table, .. } => {
                format!("record<{table}>")
            }
            FieldType::Enum(_) | FieldType::ExternalEnum(_) => "string".to_string(),
            FieldType::Currency => "currency".to_string(),
        }
    }

    /// Set serde failure policy for a [`JsonAs`](Self::JsonAs) / configured JsonAs field.
    ///
    /// Other variants are returned unchanged (DSL extract rejects invalid chains).
    #[must_use]
    pub fn serde_error(self, mode: JsonAsSerdeError) -> Self {
        match self {
            FieldType::JsonAs(type_path) => FieldType::JsonAsConfigured {
                type_path,
                serde_error: mode,
            },
            FieldType::JsonAsConfigured { type_path, .. } => FieldType::JsonAsConfigured {
                type_path,
                serde_error: mode,
            },
            other => other,
        }
    }

    /// Attach an explicit model path for connection helpers / query hops.
    ///
    /// Only valid on [`Record`](Self::Record) / [`RecordTargeted`](Self::RecordTargeted).
    /// Other variants are returned unchanged (DSL extract rejects invalid chains).
    #[must_use]
    pub fn target(self, path: &'static str) -> Self {
        match self {
            FieldType::Record(table) => FieldType::RecordTargeted {
                table,
                target: path,
            },
            FieldType::RecordTargeted { table, .. } => FieldType::RecordTargeted {
                table,
                target: path,
            },
            other => other,
        }
    }

    /// JsonAs type path when this is a JsonAs variant.
    pub fn json_as_type_path(&self) -> Option<&'static str> {
        match self {
            FieldType::JsonAs(path) => Some(path),
            FieldType::JsonAsConfigured { type_path, .. } => Some(type_path),
            _ => None,
        }
    }

    /// Serde policy for JsonAs (default [`JsonAsSerdeError::Error`]).
    pub fn json_as_serde_error(&self) -> Option<JsonAsSerdeError> {
        match self {
            FieldType::JsonAs(_) => Some(JsonAsSerdeError::Error),
            FieldType::JsonAsConfigured { serde_error, .. } => Some(*serde_error),
            _ => None,
        }
    }

    /// Record table name when this is a Record variant.
    pub fn record_table(&self) -> Option<&'static str> {
        match self {
            FieldType::Record(table) | FieldType::RecordTargeted { table, .. } => Some(table),
            _ => None,
        }
    }

    /// Explicit model path from [`.target`](Self::target), if any.
    pub fn record_target(&self) -> Option<&'static str> {
        match self {
            FieldType::RecordTargeted { target, .. } => Some(target),
            _ => None,
        }
    }
}

/// Built-in validators attachable to schema or trait fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validator {
    Email,
    Phone,
    Url,
    NonEmpty,
    MinLength(usize),
    MaxLength(usize),
    Pattern(&'static str),
    Enum(&'static [&'static str]),
    Positive,
    NonNegative,
    Min(i64),
    Max(i64),
    Range(i64, i64),
    Custom(&'static str),
}

impl Validator {
    pub fn to_toml_string(&self) -> String {
        match self {
            Validator::Email => "email".to_string(),
            Validator::Phone => "phone".to_string(),
            Validator::Url => "url".to_string(),
            Validator::NonEmpty => "non_empty".to_string(),
            Validator::MinLength(n) => format!("min_length:{n}"),
            Validator::MaxLength(n) => format!("max_length:{n}"),
            Validator::Pattern(p) => format!("pattern:{p}"),
            Validator::Enum(values) => {
                let joined = values
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("enum:{joined}")
            }
            Validator::Positive => "positive".to_string(),
            Validator::NonNegative => "non_negative".to_string(),
            Validator::Min(n) => format!("min:{n}"),
            Validator::Max(n) => format!("max:{n}"),
            Validator::Range(min, max) => format!("range:{min},{max}"),
            Validator::Custom(name) => format!("fn:{name}"),
        }
    }
}

/// Role markers used by older privacy/mutation DSL shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Public,
    User,
    Admin,
    System,
    Owner,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Public => "public",
            Role::User => "user",
            Role::Admin => "admin",
            Role::System => "system",
            Role::Owner => "owner",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldType, JsonAsSerdeError};

    #[test]
    fn field_type_record_as_str() {
        assert_eq!(FieldType::Record("user").as_str(), "record<user>");
    }

    #[test]
    fn field_type_record_target() {
        let ft = FieldType::Record("user").target("other_crate::generated::User");
        assert_eq!(ft.as_str(), "record<user>");
        assert_eq!(ft.record_target(), Some("other_crate::generated::User"));
    }

    #[test]
    fn field_type_json_as_serde_error() {
        let ft = FieldType::JsonAs("crate::Payload").serde_error(JsonAsSerdeError::Panic);
        assert_eq!(ft.json_as_type_path(), Some("crate::Payload"));
        assert_eq!(ft.json_as_serde_error(), Some(JsonAsSerdeError::Panic));
        assert_eq!(FieldType::JsonAs("crate::Payload").as_str(), "json");
    }

    #[test]
    fn field_type_currency_as_str() {
        assert_eq!(FieldType::Currency.as_str(), "currency");
    }
}
