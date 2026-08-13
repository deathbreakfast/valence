//! SQL column references for typed Valence tables.

use crate::safe_ident::assert_safe_ident;

/// Map a Valence field name to a SQL column reference.
///
/// Callers that accept untrusted field names must use [`sql_doc_column_checked`].
pub fn sql_doc_column(field: &str) -> String {
    sql_doc_column_checked(field).unwrap_or_else(|_| "__valence_rejected_ident".to_string())
}

/// Map a field name to a SQL column after charset validation.
///
/// # Errors
///
/// Returns [`crate::Error::Validation`] when `field` is not a safe identifier.
pub fn sql_doc_column_checked(field: &str) -> crate::error::Result<String> {
    assert_safe_ident(field)?;
    Ok(field.to_string())
}

/// Rewrite `SELECT *` / field lists for typed tables (`SELECT *` or explicit columns).
pub fn sql_select_clause(projection: Option<&Vec<String>>) -> String {
    match projection {
        None => "*".to_string(),
        Some(fields) if fields.len() == 1 && fields[0].trim() == "*" => "*".to_string(),
        Some(fields) => fields
            .iter()
            .map(|f| {
                let trimmed = f.trim();
                if trimmed.starts_with("VALUE ") {
                    let inner = trimmed.trim_start_matches("VALUE ").trim();
                    sql_doc_column(inner)
                } else if trimmed == "*" {
                    "*".to_string()
                } else {
                    sql_doc_column(trimmed)
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}
