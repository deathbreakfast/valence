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

#[cfg(test)]
mod tests {
    use super::assert_safe_ident;

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
}
