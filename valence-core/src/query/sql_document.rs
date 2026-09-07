//! SQL column references for typed Valence tables.

use crate::safe_ident::assert_safe_ident;

/// Map a Valence field name to a SQL column reference.
///
/// Callers that accept untrusted field names must use [`sql_doc_column_checked`].
pub fn sql_doc_column(field: &str) -> String {
    sql_doc_column_checked(field).unwrap_or_else(|_| "__valence_rejected_ident".to_string())
}

/// Map a field name or JSON subpath (`price` / `price.code`) to a SQL expression.
///
/// Bare identifiers become column references. Dotted paths become
/// `json_extract(<column>, '$.<path>')` so Currency (and other JSON cells) can
/// be filtered by subfields. Postgres rewrites `json_extract` via the SQL
/// backend.
///
/// # Errors
///
/// Returns [`crate::Error::Validation`] when any path segment fails
/// [`assert_safe_ident`] or the path is empty / has empty segments.
pub fn sql_doc_column_checked(field: &str) -> crate::error::Result<String> {
    if !field.contains('.') {
        assert_safe_ident(field)?;
        return Ok(field.to_string());
    }
    let mut parts = field.split('.');
    let column = parts
        .next()
        .ok_or_else(|| crate::error::Error::Validation("empty identifier".to_string()))?;
    assert_safe_ident(column)?;
    let subpath: Vec<&str> = parts.collect();
    if subpath.is_empty() || subpath.iter().any(|p| p.is_empty()) {
        return Err(crate::error::Error::Validation(format!(
            "empty identifier segment in {field:?}"
        )));
    }
    for seg in &subpath {
        assert_safe_ident(seg)?;
    }
    sql_json_subpath(column, &subpath.join("."))
}

/// Build `json_extract(column, '$.path')` after validating `column` and each
/// path segment with [`assert_safe_ident`].
///
/// # Errors
///
/// Returns [`crate::Error::Validation`] when `column` or any path segment is
/// unsafe or empty.
pub fn sql_json_subpath(column: &str, path: &str) -> crate::error::Result<String> {
    assert_safe_ident(column)?;
    if path.is_empty() {
        return Err(crate::error::Error::Validation(
            "empty JSON subpath".to_string(),
        ));
    }
    for seg in path.split('.') {
        if seg.is_empty() {
            return Err(crate::error::Error::Validation(
                "empty identifier segment in JSON subpath".to_string(),
            ));
        }
        assert_safe_ident(seg)?;
    }
    Ok(format!("json_extract({column}, '$.{path}')"))
}

/// True when `field` is a dotted JSON subpath (Currency / Json cell filter).
#[must_use]
pub fn is_json_subpath(field: &str) -> bool {
    field.contains('.')
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn sql_doc_column_bare_field_unchanged() {
        assert_eq!(sql_doc_column_checked("price").unwrap(), "price");
        assert_eq!(sql_doc_column_checked("at").unwrap(), "at");
    }

    #[test]
    fn sql_doc_column_one_segment_json_extract() {
        assert_eq!(
            sql_doc_column_checked("price.code").unwrap(),
            "json_extract(price, '$.code')"
        );
        assert_eq!(
            sql_doc_column_checked("price.amount_minor").unwrap(),
            "json_extract(price, '$.amount_minor')"
        );
    }

    #[test]
    fn sql_doc_column_multi_segment_json_extract() {
        assert_eq!(
            sql_doc_column_checked("a.b.c").unwrap(),
            "json_extract(a, '$.b.c')"
        );
    }

    #[test]
    fn sql_doc_column_rejects_injection_chars() {
        let err = sql_doc_column_checked("price.code;drop").unwrap_err();
        assert!(
            err.to_string().contains("unsafe"),
            "expected unsafe ident error, got {err}"
        );
        let err = sql_doc_column_checked("$.x").unwrap_err();
        assert!(
            err.to_string().contains("unsafe"),
            "expected unsafe ident error, got {err}"
        );
    }

    #[test]
    fn sql_doc_column_rejects_empty_segment() {
        let err = sql_doc_column_checked("price..code").unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected empty segment error, got {err}"
        );
        let err = sql_doc_column_checked("").unwrap_err();
        assert!(
            err.to_string().contains("empty"),
            "expected empty ident error, got {err}"
        );
    }
}
