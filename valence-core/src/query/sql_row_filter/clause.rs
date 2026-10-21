//! Row matching for WHERE clauses that contain `OR`, including `OR` groups joined by `AND`.
//!
//! Record predicates compile to a parenthesised `OR` group, so two of them joined by `AND`
//! must match as "every group matches", not as one flat `OR`.

use super::{
    extract_field_ref, row_field_cmp, row_field_equals, row_string_field, sql_like_matches,
};

type Params = [(String, serde_json::Value)];

/// Split `clause` on `keyword` (e.g. `" AND "`, matched case-insensitively) at paren depth 0
/// and outside `'...'` literals. `None` when parens or quotes are unbalanced.
pub(super) fn split_top_level<'a>(clause: &'a str, keyword: &str) -> Option<Vec<&'a str>> {
    let upper = clause.to_ascii_uppercase();
    let (mut depth, mut in_quote, mut start) = (0usize, false, 0usize);
    let mut parts = Vec::new();
    let mut i = 0;
    while i < clause.len() {
        let byte = clause.as_bytes()[i];
        if byte == b'\'' {
            in_quote = !in_quote;
        } else if !in_quote {
            match byte {
                b'(' => depth += 1,
                b')' => depth = depth.checked_sub(1)?,
                _ if depth == 0 && upper.as_bytes()[i..].starts_with(keyword.as_bytes()) => {
                    parts.push(clause[start..i].trim());
                    i += keyword.len();
                    start = i;
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if depth != 0 || in_quote {
        return None;
    }
    parts.push(clause[start..].trim());
    Some(parts)
}

/// Inner text when one paren pair wraps the whole of `s`.
fn strip_wrapping_parens(s: &str) -> Option<&str> {
    let s = s.trim();
    let inner = s.strip_prefix('(')?.strip_suffix(')')?;
    // `(a) AND (b)` starts and ends with parens but is not wrapped: the first one must
    // stay open until the last character.
    let mut depth = 0usize;
    let mut in_quote = false;
    for byte in inner.bytes() {
        match byte {
            b'\'' => in_quote = !in_quote,
            b'(' if !in_quote => depth += 1,
            b')' if !in_quote => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    (depth == 0 && !in_quote).then_some(inner)
}

/// Whether `row` satisfies `clause`. `OR` binds looser than `AND`, as in SQL.
///
/// Fails closed: a term this matcher cannot read (unknown operator, `IS NULL`, a `$key`
/// missing from `params`, unbalanced parens or quotes) does not match, so it excludes the row
/// instead of being skipped.
pub(super) fn clause_matches(row: &serde_json::Value, clause: &str, params: &Params) -> bool {
    let Some(or_parts) = split_top_level(clause, " OR ") else {
        return false;
    };
    if or_parts.len() > 1 {
        return or_parts.iter().any(|p| clause_matches(row, p, params));
    }
    let Some(and_parts) = split_top_level(clause, " AND ") else {
        return false;
    };
    if and_parts.len() > 1 {
        return and_parts.iter().all(|p| clause_matches(row, p, params));
    }
    match strip_wrapping_parens(clause) {
        Some(inner) => clause_matches(row, inner, params),
        None => term_matches(row, clause, params),
    }
}

/// One `field {=,>,>=,<,<=} $key` or `field LIKE $key` term.
fn term_matches(row: &serde_json::Value, term: &str, params: &Params) -> bool {
    let Some(dollar) = term.rfind('$') else {
        return false;
    };
    let key = term[dollar + 1..].trim();
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return false;
    }
    let Some((_, value)) = params.iter().find(|(k, _)| k == key) else {
        return false;
    };
    let before = term[..dollar].trim_end();

    let upper = before.to_ascii_uppercase();
    if upper.ends_with(" LIKE") {
        let field = extract_field_ref(&before[..before.len() - " LIKE".len()]);
        return !field.is_empty()
            && value.as_str().is_some_and(|pattern| {
                row_string_field(row, &field).is_some_and(|s| sql_like_matches(s, pattern))
            });
    }

    // Longer operators first so `>=` is not read as `>`; `!=` and `<>` are rejected.
    for op in [">=", "<=", ">", "<", "="] {
        let Some(lhs) = before.strip_suffix(op) else {
            continue;
        };
        if lhs.ends_with(['!', '<', '>', '=']) {
            return false;
        }
        let field = extract_field_ref(lhs);
        if field.is_empty() {
            return false;
        }
        return if op == "=" {
            row_field_equals(row, &field, value)
        } else {
            row_field_cmp(row, &field, value, op)
        };
    }
    false
}
