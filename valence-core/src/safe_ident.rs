//! Charset validation for SQL/Surreal identifiers interpolated into queries.

use crate::error::{Error, Result};

/// Accept only ASCII alphanumeric + underscore identifiers (tables, fields, edges).
///
/// # Errors
///
/// Returns [`Error::Validation`] when `s` is empty or contains any other character.
pub fn assert_safe_ident(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(Error::Validation("empty identifier".to_string()));
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(Error::Validation(format!("unsafe identifier: {s:?}")))
    }
}

/// Quote a previously validated identifier for SQLite / Postgres DDL and DML.
///
/// Double-quotes avoid reserved-word failures (`group`, `order`, …). Callers must
/// [`assert_safe_ident`] first so the value cannot contain quote characters.
#[must_use]
pub fn quote_sql_ident(s: &str) -> String {
    format!("\"{s}\"")
}

#[cfg(test)]
mod tests {
    use super::{assert_safe_ident, quote_sql_ident};

    #[test]
    fn accepts_simple_idents() {
        assert_safe_ident("user").unwrap();
        assert_safe_ident("valence_data_ownership").unwrap();
        assert_safe_ident("a1_b2").unwrap();
    }

    #[test]
    fn rejects_injection_shapes() {
        assert!(assert_safe_ident("").is_err());
        assert!(assert_safe_ident("user;drop").is_err());
        assert!(assert_safe_ident("a'b").is_err());
        assert!(assert_safe_ident("a b").is_err());
        assert!(assert_safe_ident("$.x").is_err());
    }

    #[test]
    fn quote_sql_ident_double_quotes() {
        assert_eq!(quote_sql_ident("group"), "\"group\"");
        assert_eq!(quote_sql_ident("order"), "\"order\"");
    }
}
