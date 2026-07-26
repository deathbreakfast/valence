//! Error types for Valence routing and backends.

use thiserror::Error;

use crate::redact::redact_credentials_in_text;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Privacy policy violation: {0}")]
    Privacy(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Pending deletion: {0}")]
    PendingDeletion(String),

    #[error("Identity error: {0}")]
    Identity(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

impl Error {
    /// Build a [`Error::Database`] with URL userinfo redacted from the message.
    #[must_use]
    pub fn database(msg: impl AsRef<str>) -> Self {
        Self::Database(redact_credentials_in_text(msg.as_ref()))
    }

    /// True when the database engine reported MVCC / transaction contention that may succeed on retry.
    pub fn is_retryable_transaction_contention(&self) -> bool {
        match self {
            Error::Database(msg) => {
                let s = msg.to_lowercase();
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
}
