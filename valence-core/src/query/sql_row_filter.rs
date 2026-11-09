//! SELECT WHERE / ORDER / LIMIT / OFFSET support for document-row adapters (mem, IndraDB,
//! Redis, MongoDB, hybrid).
//!
//! Filtering works on the text of the compiled SQL, not on a parsed query.

mod clause;

use crate::compiled_query::CompiledQuery;

/// Apply `field {=,>,>=,<,<=} $param` and `LIKE` filters from compiled SQL.
///
/// The clause is matched term by term with `OR` looser than `AND`: a row is kept when every
/// top-level `AND` conjunct matches, and each conjunct matches when any of its `OR` terms does
/// (two record predicates joined by `AND`, for example). A term the filter cannot read
/// (`!=`, `IN`, an unknown `$key`, unbalanced quotes) excludes the row.
///
/// Matching is string-level on the compiled SQL.
pub fn apply_equality_where(
    rows: Vec<serde_json::Value>,
    compiled: &CompiledQuery,
) -> Vec<serde_json::Value> {
    let upper = compiled.query_string.to_uppercase();
    let Some(where_idx) = upper.find(" WHERE ") else {
        return rows;
    };
    let where_part = &compiled.query_string[where_idx + 7..];
    let where_upper = where_part.to_uppercase();
    let end = where_upper
        .find(" ORDER BY ")
        .or_else(|| where_upper.find(" LIMIT "))
        .or_else(|| where_upper.find(" OFFSET "))
        .or_else(|| where_upper.find(" GROUP BY "))
        .unwrap_or(where_part.len());
    let clause = where_part[..end].trim();

    rows.into_iter()
        .filter(|row| clause::clause_matches(row, clause, &compiled.params))
        .collect()
}

/// Match a SQL `LIKE` pattern: `%` is any run of characters, `_` is one character, and
/// `escape` (when set) makes the next `%`, `_` or escape character literal. ASCII letters
/// compare case-insensitively, as in SQLite. A pattern that ends on a bare escape matches
/// nothing.
fn sql_like_matches(value: &str, pattern: &str, escape: Option<char>) -> bool {
    enum Tok {
        Any,
        One,
        Lit(char),
    }
    let mut toks = Vec::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        toks.push(match c {
            _ if Some(c) == escape => match chars.next() {
                Some(next) => Tok::Lit(next.to_ascii_lowercase()),
                None => return false,
            },
            '%' => Tok::Any,
            '_' => Tok::One,
            _ => Tok::Lit(c.to_ascii_lowercase()),
        });
    }
    let value: Vec<char> = value.chars().map(|c| c.to_ascii_lowercase()).collect();
    let (mut v, mut t) = (0usize, 0usize);
    // Last `%` seen and the value index it is currently absorbing up to.
    let mut backtrack: Option<(usize, usize)> = None;
    while v < value.len() {
        match toks.get(t) {
            Some(Tok::Any) => {
                backtrack = Some((t, v));
                t += 1;
            }
            Some(Tok::One) => {
                v += 1;
                t += 1;
            }
            Some(Tok::Lit(c)) if *c == value[v] => {
                v += 1;
                t += 1;
            }
            _ => match backtrack {
                Some((bt, bv)) => {
                    backtrack = Some((bt, bv + 1));
                    t = bt + 1;
                    v = bv + 1;
                }
                None => return false,
            },
        }
    }
    toks[t..].iter().all(|tok| matches!(tok, Tok::Any))
}

fn row_field_cmp(
    row: &serde_json::Value,
    field: &str,
    threshold: &serde_json::Value,
    op: &str,
) -> bool {
    let Some(actual) = nested_field(row, field) else {
        return false;
    };
    let Some(ord) = compare_json_values(actual, threshold) else {
        return false;
    };
    match op {
        ">" => ord.is_gt(),
        ">=" => ord.is_ge(),
        "<" => ord.is_lt(),
        "<=" => ord.is_le(),
        _ => false,
    }
}

fn compare_json_values(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    if let (Some(a), Some(b)) = (json_as_i64(actual), json_as_i64(expected)) {
        return Some(a.cmp(&b));
    }
    if let (Some(a), Some(b)) = (json_as_f64(actual), json_as_f64(expected)) {
        return a.partial_cmp(&b);
    }
    match (actual.as_str(), expected.as_str()) {
        (Some(a), Some(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn json_as_i64(v: &serde_json::Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(n) = v.as_u64() {
        return i64::try_from(n).ok();
    }
    v.as_str().and_then(|s| s.trim().parse().ok())
}

fn json_as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Apply ORDER BY (every key, each with its own direction) plus LIMIT/OFFSET windowing.
///
/// Numbers compare numerically and strings lexically; a missing field sorts before any
/// value, so DESC puts it last. The sort is stable, so rows tied on every key keep their
/// input order.
pub fn apply_order_limit_offset(
    mut rows: Vec<serde_json::Value>,
    sql: &str,
) -> Vec<serde_json::Value> {
    let upper = sql.to_uppercase();
    if let Some(order_idx) = upper.find(" ORDER BY ") {
        let rest = &sql[order_idx + 10..];
        let rest_upper = rest.to_uppercase();
        let end = [" LIMIT ", " OFFSET ", " START "]
            .iter()
            .filter_map(|kw| rest_upper.find(kw))
            .min()
            .unwrap_or(rest.len());
        let keys: Vec<(String, bool)> = split_top_level_commas(rest[..end].trim())
            .into_iter()
            .map(|part| {
                let part = part.trim();
                let desc = part.to_uppercase().ends_with(" DESC");
                (order_by_field(part), desc)
            })
            .collect();
        rows.sort_by(|a, b| {
            keys.iter()
                .map(|(field, desc)| {
                    let ord = compare_for_sort(nested_field(a, field), nested_field(b, field));
                    if *desc {
                        ord.reverse()
                    } else {
                        ord
                    }
                })
                .find(|ord| ord.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    let offset = if let Some((off_idx, kw_len)) = upper
        .rfind(" OFFSET ")
        .map(|at| (at, " OFFSET ".len()))
        .or_else(|| upper.rfind(" START ").map(|at| (at, " START ".len())))
    {
        let tail = sql[off_idx + kw_len..].trim();
        let num = tail.split_whitespace().next().unwrap_or("0");
        num.parse().unwrap_or(0)
    } else {
        0usize
    };
    if offset > 0 {
        rows = rows.into_iter().skip(offset).collect();
    }

    if let Some(limit_idx) = upper.rfind(" LIMIT ") {
        let tail = sql[limit_idx + 7..].trim();
        let num = tail.split_whitespace().next().unwrap_or("0");
        if let Ok(limit) = num.parse::<usize>() {
            rows.truncate(limit);
        }
    }
    rows
}

/// Split on commas outside parentheses, so `json_extract(body, '$.a')` stays whole.
fn split_top_level_commas(clause: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in clause.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&clause[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&clause[start..]);
    parts
}

/// Total order for sorting: missing < present; numbers numerically, strings lexically,
/// `{table, id}` objects by `id`; mixed or other types compare equal.
fn compare_for_sort(
    a: Option<&serde_json::Value>,
    b: Option<&serde_json::Value>,
) -> std::cmp::Ordering {
    use serde_json::Value;
    match (a, b) {
        (Some(Value::Number(x)), Some(Value::Number(y))) => {
            if let (Some(x), Some(y)) = (x.as_i64(), y.as_i64()) {
                return x.cmp(&y);
            }
            if let (Some(x), Some(y)) = (x.as_u64(), y.as_u64()) {
                return x.cmp(&y);
            }
            x.as_f64()
                .partial_cmp(&y.as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        }
        (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
        (Some(Value::Object(x)), Some(Value::Object(y))) => {
            compare_for_sort(x.get("id"), y.get("id"))
        }
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    }
}

/// Parse a SQL LHS into a Valence field path (`name`, `body` path, or `price.code`).
fn extract_field_ref(before: &str) -> String {
    let mut trimmed = before.trim();
    // Strip CAST(… AS INTEGER) wrappers from int JSON subpath compares.
    if let Some(inner) = trimmed
        .strip_prefix("CAST(")
        .or_else(|| trimmed.strip_prefix("cast("))
    {
        if let Some(as_at) = inner.to_uppercase().rfind(" AS ") {
            trimmed = inner[..as_at].trim();
        }
    }
    if let Some(start) = trimmed.find("json_extract(") {
        let rest = &trimmed[start + "json_extract(".len()..];
        if let Some(comma) = rest.find(',') {
            let col = rest[..comma].trim();
            let after = rest[comma + 1..].trim_start();
            if let Some(path_rest) = after.strip_prefix("'$.") {
                if let Some(end) = path_rest.find('\'') {
                    let path = &path_rest[..end];
                    if col == "body" {
                        return path.to_string();
                    }
                    if !col.is_empty() && !path.is_empty() {
                        return format!("{col}.{path}");
                    }
                }
            }
        }
    }
    trimmed
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c == '"' || c == '`')
        .to_string()
}

fn order_by_field(order_clause: &str) -> String {
    if order_clause.contains("json_extract(") {
        return extract_field_ref(order_clause);
    }
    let token = order_clause.split_whitespace().next().unwrap_or("id");
    extract_field_ref(token)
}

fn row_string_field<'a>(row: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    if let Some(v) = nested_field(row, field) {
        if let Some(s) = v.as_str() {
            return Some(s);
        }
    }
    None
}

fn nested_field<'a>(row: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    let mut cur = row;
    for part in field.split('.') {
        cur = cur
            .get(part)
            .or_else(|| cur.get("body").and_then(|b| b.get(part)))?;
    }
    Some(cur)
}

fn row_field_equals(row: &serde_json::Value, field: &str, expected: &serde_json::Value) -> bool {
    if let Some(actual) = nested_field(row, field) {
        return values_equal(actual, expected);
    }
    false
}

fn values_equal(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Object(obj), serde_json::Value::String(expected_s)) => {
            let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let table = obj.get("table").and_then(|v| v.as_str()).unwrap_or("");
            expected_s == id || expected_s == &format!("{table}:{id}")
        }
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests;
