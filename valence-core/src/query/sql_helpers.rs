//! String parsing and SQL/SurrealQL fragments shared by query emitters and hop execution.
//!
//! Kept separate from [`super::types`] so hop/connection SQL generation stays testable in isolation.

#[cfg(any(
    feature = "compiler-surreal",
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
use crate::error::Result;

/// Extract record ID from a JSON value that may be a Thing (string or object) or plain id.
pub(super) fn extract_id_from_fk_value(value: Option<&serde_json::Value>) -> Option<String> {
    let value = value?;
    if let Some(s) = value.as_str() {
        return Some(
            s.split(':')
                .next_back()
                .unwrap_or(s)
                .trim_matches(|c| {
                    c == '⟨' || c == '‹' || c == '«' || c == '⟩' || c == '›' || c == '»'
                })
                .to_string(),
        );
    }
    if let Some(obj) = value.as_object() {
        let id_val = obj.get("id")?;
        if let Some(s) = id_val.as_str() {
            return Some(s.to_string());
        }
        if let Some(id_obj) = id_val.as_object() {
            if let Some(inner) = id_obj.get("String").and_then(|v| v.as_str()) {
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// Parse `SELECT <projection> FROM <table> [WHERE …]` produced by query emitters.
///
/// Returns `(table, where_sql)` where `where_sql` is `true` if there is no `WHERE` clause.
#[cfg(any(
    feature = "compiler-surreal",
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub(super) fn parse_select_from_subquery(sub_query: &str) -> Result<(&str, String)> {
    let s = sub_query.trim();
    let upper = s.to_uppercase();
    if !upper.starts_with("SELECT ") {
        return Err(crate::Error::Validation(
            "connection subquery must start with SELECT ".to_string(),
        ));
    }
    let from_idx = upper.find(" FROM ").ok_or_else(|| {
        crate::Error::Validation("connection subquery must contain FROM ".to_string())
    })?;
    let table_and_rest = s[from_idx + 6..].trim_start();
    let where_idx = table_and_rest.to_uppercase().find(" WHERE ");
    let (table, tail) = if let Some(wi) = where_idx {
        (
            table_and_rest[..wi].trim(),
            table_and_rest[wi..].trim_start(),
        )
    } else {
        (table_and_rest.trim(), "")
    };
    if table.is_empty() {
        return Err(crate::Error::Validation(
            "connection subquery has empty table name".into(),
        ));
    }
    if tail.is_empty() {
        return Ok((table, "true".to_string()));
    }
    let after_where = tail
        .strip_prefix("WHERE ")
        .or_else(|| tail.strip_prefix("where "))
        .ok_or_else(|| {
            crate::Error::Validation(format!(
                "connection subquery must use WHERE or end after table; got: {tail:?}"
            ))
        })?
        .trim();
    let cond = trim_after_where_clauses(after_where);
    Ok((table, cond.to_string()))
}

/// Parse `SELECT * FROM <table> [WHERE …]` for simple connection subqueries.
#[cfg(any(
    feature = "compiler-surreal",
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub(super) fn parse_select_star_subquery_for_exists(sub_query: &str) -> Result<(&str, String)> {
    parse_select_from_subquery(sub_query)
}

/// Build correlated `(SELECT id … LIMIT 1)` for HasOneForward hops from a trait union subquery.
#[cfg(feature = "compiler-surreal")]
pub(super) fn hasone_forward_exists_sql(
    sub_query: &str,
    fk_field: &str,
    parent_id_expr: &str,
) -> Result<String> {
    let fk_lhs = surreal_type_record_from_colon_strand(fk_field);
    let fk_match = format!("({fk_field} = {parent_id_expr} OR {fk_lhs} = {parent_id_expr})");
    let (table_part, where_sql) = parse_select_from_subquery(sub_query)?;

    if table_part.contains(',') {
        let branches: Vec<String> = table_part
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|tbl| format!("(SELECT id FROM {tbl} WHERE ({where_sql}) AND {fk_match} LIMIT 1)"))
            .collect();
        return Ok(format!("({})", branches.join(" OR ")));
    }

    Ok(format!(
        "(SELECT id FROM {table_part} WHERE ({where_sql}) AND {fk_match} LIMIT 1)"
    ))
}

/// Strip `ORDER BY`, `GROUP BY`, `LIMIT`, `OFFSET` tails from a WHERE clause fragment.
#[cfg(any(
    feature = "compiler-surreal",
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
pub(super) fn trim_after_where_clauses(mut s: &str) -> &str {
    const SEPARATORS: &[&str] = &[" ORDER BY ", " GROUP BY ", " LIMIT ", " OFFSET "];
    loop {
        let mut min_pos: Option<usize> = None;
        for sep in SEPARATORS {
            if let Some(p) = s.find(sep) {
                min_pos = Some(min_pos.map_or(p, |m| m.min(p)));
            }
        }
        let Some(p) = min_pos else {
            break;
        };
        s = s[..p].trim();
    }
    s.trim()
}

/// SurrealQL: build `type::record(string::split(<string> path, ':')[0], join([1..], ':'))`.
#[cfg(feature = "compiler-surreal")]
pub(super) fn surreal_type_record_from_colon_strand(parent_field_path: &str) -> String {
    format!(
        "type::record(string::split(<string> {parent_field_path}, ':')[0], array::join(string::split(<string> {parent_field_path}, ':')[1..], ':'))"
    )
}

/// For `SELECT … FROM a, b, c` (trait query-all), use the first table to resolve DB routing.
pub(super) fn backend_schema_table(from_clause_table: &str) -> &str {
    from_clause_table
        .split_once(',')
        .map(|(first, _)| first.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| from_clause_table.trim())
}

fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (serde_json::Value::Null, serde_json::Value::Null) => Ordering::Equal,
        (serde_json::Value::Null, _) => Ordering::Less,
        (_, serde_json::Value::Null) => Ordering::Greater,
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a.cmp(b),
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            }
        }
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a.cmp(b),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

fn compare_json_field(
    a: &serde_json::Value,
    b: &serde_json::Value,
    field: &str,
) -> std::cmp::Ordering {
    match (a.get(field), b.get(field)) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(a), Some(b)) => compare_json_values(a, b),
    }
}

/// Apply global ORDER BY / OFFSET / LIMIT after merging per-table trait QueryAll branches.
pub fn apply_post_merge_query_window(
    rows: &mut Vec<serde_json::Value>,
    order_by: &[super::predicates::OrderBy],
    offset: Option<u32>,
    limit: Option<u32>,
) {
    if !order_by.is_empty() {
        rows.sort_by(|a, b| {
            for ob in order_by {
                let cmp = compare_json_field(a, b, &ob.field);
                if cmp != std::cmp::Ordering::Equal {
                    return match ob.direction {
                        super::predicates::SortDirection::Asc => cmp,
                        super::predicates::SortDirection::Desc => cmp.reverse(),
                    };
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    let start = offset.unwrap_or(0) as usize;
    if start >= rows.len() {
        rows.clear();
        return;
    }
    if start > 0 {
        rows.drain(0..start);
    }
    if let Some(limit) = limit {
        rows.truncate(limit as usize);
    }
}
