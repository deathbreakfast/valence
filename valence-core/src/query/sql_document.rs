//! SQL column references for JSON document rows (`id` + `body`).

use crate::safe_ident::assert_safe_ident;

/// Map a Valence field name to a SQL expression against document storage.
///
/// Callers that accept untrusted field names must use [`sql_doc_column_checked`].
pub fn sql_doc_column(field: &str) -> String {
    sql_doc_column_checked(field)
        .unwrap_or_else(|_| "json_extract(body, '$.__valence_rejected_ident')".to_string())
}

/// Map a field name to a SQL document column expression after charset validation.
///
/// # Errors
///
/// Returns [`crate::Error::Validation`] when `field` is not a safe identifier.
pub fn sql_doc_column_checked(field: &str) -> crate::error::Result<String> {
    if field == "id" {
        return Ok("id".to_string());
    }
    assert_safe_ident(field)?;
    Ok(format!("json_extract(body, '$.{field}')"))
}

/// Rewrite `SELECT *` / field lists for document tables.
pub fn sql_select_clause(projection: Option<&Vec<String>>) -> String {
    match projection {
        None => "id, body".to_string(),
        Some(fields) if fields.len() == 1 && fields[0].trim() == "*" => "id, body".to_string(),
        Some(fields) => fields
            .iter()
            .map(|f| {
                let trimmed = f.trim();
                if trimmed.starts_with("VALUE ") {
                    let inner = trimmed.trim_start_matches("VALUE ").trim();
                    if inner == "id" {
                        "id".to_string()
                    } else {
                        sql_doc_column(inner)
                    }
                } else if trimmed == "id" {
                    "id".to_string()
                } else if trimmed == "*" {
                    "id, body".to_string()
                } else {
                    sql_doc_column(trimmed)
                }
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}
