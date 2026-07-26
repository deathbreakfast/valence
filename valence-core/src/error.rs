//! Error types for Valence routing and backends.

use thiserror::Error;

use crate::redact::redact_credentials_in_text;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Database error: {message}")]
    Database {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Privacy policy violation: {0}")]
    Privacy(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<serde_json::Error>,
    },

    #[error("Pending deletion: {0}")]
    PendingDeletion(String),

    #[error("Identity error: {0}")]
    Identity(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::serialization(e)
    }
}

impl Error {
    /// Build a [`Error::Database`] with URL userinfo redacted from the message.
    #[must_use]
    pub fn database(msg: impl AsRef<str>) -> Self {
        Self::Database {
            message: redact_credentials_in_text(msg.as_ref()),
            source: None,
        }
    }

    /// Build a [`Error::Database`] with a typed source cause and redacted message.
    #[must_use]
    pub fn database_with_source(
        msg: impl AsRef<str>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Database {
            message: redact_credentials_in_text(msg.as_ref()),
            source: Some(Box::new(source)),
        }
    }

    /// Build a [`Error::Serialization`] that preserves the `serde_json` cause.
    #[must_use]
    pub fn serialization(e: serde_json::Error) -> Self {
        Self::Serialization {
            message: e.to_string(),
            source: Some(e),
        }
    }

    /// Build a [`Error::Serialization`] from a message when no serde cause is available.
    #[must_use]
    pub fn serialization_msg(msg: impl Into<String>) -> Self {
        Self::Serialization {
            message: msg.into(),
            source: None,
        }
    }

    /// True when the database engine reported MVCC / transaction contention that may succeed on retry.
    pub fn is_retryable_transaction_contention(&self) -> bool {
        match self {
            Error::Database { message, .. } => {
                let s = message.to_lowercase();
                s.contains("read or write conflict")
                    || s.contains("can be retried")
                    || (s.contains("failed transaction") && s.contains("conflict"))
            }
            _ => false,
        }
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Validation(s.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_redacts_url_userinfo() {
        let err = Error::database("connect failed: postgres://user:secret@host/db");
        let s = err.to_string();
        assert!(s.contains("postgres://***@host/db"));
        assert!(!s.contains("secret"));
    }

    #[test]
    fn serialization_preserves_source() {
        let raw = serde_json::from_str::<u32>("not-json").expect_err("bad json");
        let err = Error::from(raw);
        let Error::Serialization {
            source: Some(_), ..
        } = err
        else {
            panic!("expected Serialization with source: {err:?}");
        };
    }
}
