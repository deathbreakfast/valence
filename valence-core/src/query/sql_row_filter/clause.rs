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
/// missing from `params`, a right-hand side that is not a `$key`, quoted literal or number,
/// unbalanced parens or quotes) does not match, so it excludes the row instead of being
/// skipped.
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

/// One `field {=,>,>=,<,<=} rhs` or `field LIKE rhs` term, where `rhs` is a `$key`, a
/// `'quoted'` string literal or a number.
fn term_matches(row: &serde_json::Value, term: &str, params: &Params) -> bool {
    let Some((before, value)) = split_rhs(term, params) else {
        return false;
    };
    let value = &value;

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

/// Split a term into the text before its right-hand side and the value that side stands for.
/// `None` for a `$key` missing from `params` or a right-hand side that is not a `$key`, a
/// `'quoted'` literal (`''` is an escaped quote) or a number.
fn split_rhs<'a>(term: &'a str, params: &Params) -> Option<(&'a str, serde_json::Value)> {
    let term = term.trim_end();
    if let Some(body) = term.strip_suffix('\'') {
        // Walk back to the opening quote, stepping over `''` pairs.
        let bytes = body.as_bytes();
        let mut i = bytes.len();
        while i > 0 {
            i -= 1;
            if bytes[i] == b'\'' {
                if i > 0 && bytes[i - 1] == b'\'' {
                    i -= 1;
                    continue;
                }
                let literal = body[i + 1..].replace("''", "'");
                return Some((body[..i].trim_end(), serde_json::Value::String(literal)));
            }
        }
        return None;
    }

    let start = term
        .rfind(|c: char| c.is_whitespace() || matches!(c, '=' | '<' | '>'))
        .map_or(0, |at| at + 1);
    let (before, token) = (term[..start].trim_end(), &term[start..]);
    if let Some(key) = token.strip_prefix('$') {
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return None;
        }
        let (_, value) = params.iter().find(|(k, _)| k == key)?;
        return Some((before, value.clone()));
    }
    if let Ok(n) = token.parse::<i64>() {
        return Some((before, n.into()));
    }
    let n = token.parse::<f64>().ok()?;
    Some((before, serde_json::Number::from_f64(n)?.into()))
}
