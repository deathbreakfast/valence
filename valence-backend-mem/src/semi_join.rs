//! ManyToMany semi-joins in compiled SQL, answered from the backend's own edge store.
//!
//! `where_*_has_results` on a ManyToMany connection compiles to
//! `id IN (SELECT from_id FROM valence_edges WHERE edge_type = $e AND to_id IN (SELECT id FROM
//! <tbl> WHERE …))`, and `where_connection_contains_many_to_many` to
//! `… AND to_table = $t AND to_id = $i)`. The document-row filter cannot read either, so the
//! clause is cut out of the query here and evaluated against the edges.

use valence_core::{CompiledQuery, Error, Result};

const HEAD: &str = "id IN (SELECT from_id FROM valence_edges WHERE edge_type = $";

/// Where the edge targets of one semi-join come from.
#[derive(Debug)]
pub(crate) enum Targets {
    /// `to_table = $t AND to_id = $i`: one named record.
    Record { table: String, id: String },
    /// `to_id IN (SELECT id FROM …)`: every row the inner query returns.
    Subquery(CompiledQuery),
}

/// One cut-out `id IN (SELECT from_id FROM valence_edges …)` clause.
#[derive(Debug)]
pub(crate) struct SemiJoin {
    pub(crate) edge_table: String,
    pub(crate) targets: Targets,
}

fn param(params: &[(String, serde_json::Value)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.as_str())
        .map(str::to_string)
}

/// `$key` placeholders in `sql`, in order.
fn param_keys(sql: &str) -> Vec<String> {
    sql.split('$')
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

fn unsupported(why: &str) -> Error {
    Error::Internal(format!(
        "in-memory backend cannot evaluate semi-join: {why}"
    ))
}

/// Byte offset one past the paren that closes the one opening at `open`.
fn closing_paren(sql: &str, open: usize) -> Option<usize> {
    let (mut depth, mut in_quote) = (0usize, false);
    for (i, ch) in sql[open..].char_indices() {
        match ch {
            '\'' => in_quote = !in_quote,
            '(' if !in_quote => depth += 1,
            ')' if !in_quote => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Paren depth of `sql` at byte `at`.
fn depth_at(sql: &str, at: usize) -> usize {
    let (mut depth, mut in_quote) = (0usize, false);
    for ch in sql[..at].chars() {
        match ch {
            '\'' => in_quote = !in_quote,
            '(' if !in_quote => depth += 1,
            ')' if !in_quote => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// The query without its semi-join clauses, plus those clauses.
///
/// A semi-join that is not a top-level `AND` conjunct (inside an `OR` group, say) cannot be
/// cut out without changing the query, so it is an error instead of being dropped.
pub(crate) fn split(compiled: &CompiledQuery) -> Result<(CompiledQuery, Vec<SemiJoin>)> {
    let mut query = compiled.query_string.clone();
    let mut joins = Vec::new();
    while let Some(start) = query.find(HEAD) {
        let open = start + "id IN ".len();
        let end = closing_paren(&query, open).ok_or_else(|| unsupported("unbalanced parens"))?;
        if depth_at(&query, start) != 0 {
            return Err(unsupported("clause is nested inside a parenthesised group"));
        }
        let clause = query[start..end].to_string();
        let keys = param_keys(&clause);
        let edge_table = keys
            .first()
            .and_then(|k| param(&compiled.params, k))
            .ok_or_else(|| unsupported("missing edge_type param"))?;
        let targets = if clause.contains(" AND to_table = $") {
            let table = keys.get(1).and_then(|k| param(&compiled.params, k));
            let id = keys.get(2).and_then(|k| param(&compiled.params, k));
            match (table, id) {
                (Some(table), Some(id)) => Targets::Record { table, id },
                _ => return Err(unsupported("missing to_table / to_id param")),
            }
        } else {
            let marker = " AND to_id IN (";
            let at = clause
                .find(marker)
                .ok_or_else(|| unsupported("unrecognised clause shape"))?;
            let inner_open = at + marker.len() - 1;
            let inner_end = closing_paren(&clause, inner_open)
                .ok_or_else(|| unsupported("unbalanced parens"))?;
            let inner = clause[inner_open + 1..inner_end - 1].to_string();
            let params = param_keys(&inner)
                .iter()
                .filter_map(|k| compiled.params.iter().find(|(pk, _)| pk == k).cloned())
                .collect();
            Targets::Subquery(CompiledQuery::new(inner, params))
        };
        joins.push(SemiJoin {
            edge_table,
            targets,
        });

        let (cut_start, cut_end) = if query[..start].ends_with(" AND ") {
            (start - " AND ".len(), end)
        } else if query[end..].starts_with(" AND ") {
            (start, end + " AND ".len())
        } else if query[..start].ends_with(" WHERE ") {
            (start - " WHERE ".len(), end)
        } else if query[..start].trim_end().to_uppercase().ends_with(" OR")
            || query[end..].trim_start().to_uppercase().starts_with("OR ")
        {
            return Err(unsupported("clause is joined by OR"));
        } else {
            (start, end)
        };
        query.replace_range(cut_start..cut_end, "");
    }
    Ok((CompiledQuery::new(query, compiled.params.clone()), joins))
}

/// The bare id of a row's `id` cell (`"x"`, `"table:x"`, or `{table, id}`).
pub(crate) fn bare_id(cell: &serde_json::Value) -> Option<String> {
    match cell {
        serde_json::Value::String(s) => Some(s.rsplit(':').next().unwrap_or(s).to_string()),
        serde_json::Value::Object(map) => map.get("id")?.as_str().map(str::to_string),
        _ => None,
    }
}

/// Table named after `FROM` in `sql`.
pub(crate) fn from_table(sql: &str) -> Option<String> {
    let at = sql.to_uppercase().find(" FROM ")?;
    sql[at + 6..]
        .split_whitespace()
        .next()
        .map(str::to_string)
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(k: &str, v: &str) -> (String, serde_json::Value) {
        (k.into(), serde_json::json!(v))
    }

    #[test]
    fn cuts_direct_clause_between_conjuncts() {
        let c = CompiledQuery::new(
            "SELECT * FROM doc WHERE name = $a AND id IN (SELECT from_id FROM valence_edges WHERE edge_type = $e AND to_table = $t AND to_id = $i) ORDER BY id".into(),
            vec![p("a", "x"), p("e", "tagged"), p("t", "tag"), p("i", "t1")],
        );
        let (rest, joins) = split(&c).unwrap();
        assert_eq!(
            rest.query_string,
            "SELECT * FROM doc WHERE name = $a ORDER BY id"
        );
        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].edge_table, "tagged");
        assert!(matches!(
            &joins[0].targets,
            Targets::Record { table, id } if table == "tag" && id == "t1"
        ));
    }

    #[test]
    fn rejects_clause_inside_or_group() {
        let c = CompiledQuery::new(
            "SELECT * FROM doc WHERE (name = $a OR id IN (SELECT from_id FROM valence_edges WHERE edge_type = $e AND to_table = $t AND to_id = $i))".into(),
            vec![p("a", "x"), p("e", "tagged"), p("t", "tag"), p("i", "t1")],
        );
        assert!(split(&c).is_err());
    }
}
