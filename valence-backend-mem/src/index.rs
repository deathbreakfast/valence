//! Equality indexes for [`crate::InMemoryBackend`], declared through
//! [`valence_core::DatabaseBackend::define_index`].
//!
//! An index only narrows which rows a `SELECT` looks at. The usual row filter still runs
//! on the candidates, so a query returns the same rows with or without an index.

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use valence_core::CompiledQuery;

/// Joins the per-field keys. JSON escapes control characters inside strings, so this
/// byte never appears inside a key component.
const KEY_SEPARATOR: char = '\u{1f}';

/// One equality index over `fields` of a single table.
#[derive(Debug)]
pub(crate) struct EqIndex {
    fields: Vec<String>,
    /// Exact key of the indexed values → row ids.
    exact: HashMap<String, HashSet<String>>,
    /// Rows whose indexed value is an object or array. Those can equal a string in more
    /// than one way (a record id matches `id` and `table:id`), so they stay candidates for
    /// every lookup.
    loose: HashSet<String>,
}

enum Entry {
    Exact(String),
    Loose,
    /// A field is missing, so the row can't satisfy an equality on it.
    Absent,
}

impl EqIndex {
    pub(crate) fn new(fields: &[&str]) -> Self {
        Self {
            fields: fields.iter().map(|f| (*f).to_string()).collect(),
            exact: HashMap::new(),
            loose: HashSet::new(),
        }
    }

    pub(crate) fn covers(&self, fields: &[&str]) -> bool {
        self.fields.len() == fields.len() && self.fields.iter().zip(fields).all(|(a, b)| a == b)
    }

    fn entry(&self, row: &Value) -> Entry {
        let mut key = String::new();
        for (i, field) in self.fields.iter().enumerate() {
            let Some(value) = row.get(field) else {
                return Entry::Absent;
            };
            if value.is_object() || value.is_array() {
                return Entry::Loose;
            }
            if i > 0 {
                key.push(KEY_SEPARATOR);
            }
            key.push_str(&value.to_string());
        }
        Entry::Exact(key)
    }

    pub(crate) fn insert(&mut self, id: &str, row: &Value) {
        match self.entry(row) {
            Entry::Exact(key) => {
                self.exact.entry(key).or_default().insert(id.to_string());
            }
            Entry::Loose => {
                self.loose.insert(id.to_string());
            }
            Entry::Absent => {}
        }
    }

    pub(crate) fn remove(&mut self, id: &str, row: &Value) {
        match self.entry(row) {
            Entry::Exact(key) => {
                if let Some(ids) = self.exact.get_mut(&key) {
                    ids.remove(id);
                    if ids.is_empty() {
                        self.exact.remove(&key);
                    }
                }
            }
            Entry::Loose => {
                self.loose.remove(id);
            }
            Entry::Absent => {}
        }
    }

    /// Candidate row ids for `pinned` (field → required value), or `None` when the query
    /// doesn't pin every indexed field.
    fn candidates(&self, pinned: &HashMap<String, &Value>) -> Option<HashSet<String>> {
        let mut key = String::new();
        for (i, field) in self.fields.iter().enumerate() {
            let value = pinned.get(field)?;
            if i > 0 {
                key.push(KEY_SEPARATOR);
            }
            key.push_str(&value.to_string());
        }
        let mut out = self.exact.get(&key).cloned().unwrap_or_default();
        out.extend(self.loose.iter().cloned());
        Some(out)
    }
}

/// Candidate ids from the first of `indexes` the query pins completely, or `None` when the
/// query must look at every row.
pub(crate) fn candidates_for(
    indexes: &[EqIndex],
    compiled: &CompiledQuery,
) -> Option<HashSet<String>> {
    if indexes.is_empty() {
        return None;
    }
    let pinned = top_level_equalities(compiled);
    if pinned.is_empty() {
        return None;
    }
    indexes.iter().find_map(|idx| idx.candidates(&pinned))
}

/// `field = $param` terms that the whole `WHERE` clause requires: top-level `AND` terms,
/// outside any parentheses. A term inside an `OR` group doesn't narrow anything, so it is
/// left out.
fn top_level_equalities(compiled: &CompiledQuery) -> HashMap<String, &Value> {
    let mut out = HashMap::new();
    let Some(clause) = where_clause(&compiled.query_string) else {
        return out;
    };
    for term in split_top_level_and(clause) {
        let Some((lhs, rhs)) = term.split_once('=') else {
            continue;
        };
        let (field, param) = (lhs.trim(), rhs.trim());
        // Rule out `>=`, `<=`, `!=`, and `==`-style forms the row filter doesn't treat as equality.
        if lhs.ends_with(['<', '>', '!']) || rhs.starts_with('=') {
            continue;
        }
        let Some(key) = param.strip_prefix('$') else {
            continue;
        };
        if !is_ident(field) || !is_ident(key) {
            continue;
        }
        if let Some((_, value)) = compiled.params.iter().find(|(k, _)| k == key) {
            out.insert(field.to_string(), value);
        }
    }
    out
}

fn where_clause(query: &str) -> Option<&str> {
    let upper = query.to_ascii_uppercase();
    let start = upper.find(" WHERE ")? + " WHERE ".len();
    let rest = &upper[start..];
    let end = [" ORDER BY ", " LIMIT ", " OFFSET ", " GROUP BY "]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    Some(query[start..start + end].trim())
}

/// Split on ` AND ` (any case) at parenthesis depth 0. Returns nothing when the clause has
/// an `OR` at depth 0, since then no single term is required.
fn split_top_level_and(clause: &str) -> Vec<&str> {
    let bytes = clause.as_bytes();
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut terms = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => in_quote = !in_quote,
            b'(' if !in_quote => depth += 1,
            b')' if !in_quote => depth -= 1,
            _ if !in_quote && depth == 0 => {
                if keyword_at(bytes, i, b" OR ") {
                    return Vec::new();
                }
                if keyword_at(bytes, i, b" AND ") {
                    terms.push(&clause[start..i]);
                    i += b" AND ".len();
                    start = i;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    terms.push(&clause[start..]);
    terms
}

fn keyword_at(bytes: &[u8], at: usize, keyword: &[u8]) -> bool {
    bytes
        .get(at..at + keyword.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(keyword))
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    fn q(sql: &str, params: Vec<(&str, Value)>) -> CompiledQuery {
        CompiledQuery::new(
            sql.to_string(),
            params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    #[test]
    fn pins_top_level_and_terms_happy() {
        let c = q(
            "SELECT record_id FROM valence_data_ownership WHERE valence_model = $model \
             AND status = $status AND (record_id = $record_id_0 OR record_id = $record_id_1)",
            vec![
                ("model", json!("team")),
                ("status", json!("pending_deletion")),
                ("record_id_0", json!("a")),
                ("record_id_1", json!("b")),
            ],
        );
        let pinned = top_level_equalities(&c);
        assert_eq!(pinned.len(), 2);
        assert_eq!(pinned["valence_model"], &json!("team"));
        assert_eq!(pinned["status"], &json!("pending_deletion"));
    }

    #[test]
    fn top_level_or_pins_nothing_sad() {
        let c = q(
            "SELECT * FROM t WHERE a = $a OR b = $b",
            vec![("a", json!(1)), ("b", json!(2))],
        );
        assert!(top_level_equalities(&c).is_empty());
    }

    #[test]
    fn comparisons_and_literals_are_not_pinned_sad() {
        let c = q(
            "SELECT * FROM t WHERE a >= $a AND b != $b AND c = 'x' AND d = $d LIMIT 5",
            vec![("a", json!(1)), ("b", json!(2)), ("d", json!("y"))],
        );
        let pinned = top_level_equalities(&c);
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned["d"], &json!("y"));
    }

    #[test]
    fn and_inside_quotes_does_not_split_sad() {
        let c = q(
            "SELECT * FROM t WHERE name = 'a AND b' AND kind = $k",
            vec![("k", json!("z"))],
        );
        let pinned = top_level_equalities(&c);
        assert_eq!(pinned.len(), 1);
        assert_eq!(pinned["kind"], &json!("z"));
    }

    #[test]
    fn index_tracks_insert_and_remove_happy() {
        let mut idx = EqIndex::new(&["valence_model", "status"]);
        let a = json!({"valence_model": "team", "status": "active"});
        let b = json!({"valence_model": "team", "status": "pending_deletion"});
        idx.insert("a", &a);
        idx.insert("b", &b);
        let pinned: HashMap<String, &Value> = [
            ("valence_model".to_string(), &b["valence_model"]),
            ("status".to_string(), &b["status"]),
        ]
        .into();
        assert_eq!(
            idx.candidates(&pinned).unwrap(),
            HashSet::from(["b".to_string()])
        );
        idx.remove("b", &b);
        assert!(idx.candidates(&pinned).unwrap().is_empty());
    }

    #[test]
    fn object_values_stay_candidates_and_missing_fields_drop_sad() {
        let mut idx = EqIndex::new(&["owner"]);
        idx.insert("obj", &json!({"owner": {"table": "user", "id": "u1"}}));
        idx.insert("none", &json!({"other": 1}));
        let want = json!("user:u1");
        let pinned: HashMap<String, &Value> = [("owner".to_string(), &want)].into();
        assert_eq!(
            idx.candidates(&pinned).unwrap(),
            HashSet::from(["obj".to_string()])
        );
    }

    #[test]
    fn partial_pin_uses_no_index_sad() {
        let idx = EqIndex::new(&["valence_model", "status"]);
        let v = json!("team");
        let pinned: HashMap<String, &Value> = [("valence_model".to_string(), &v)].into();
        assert!(idx.candidates(&pinned).is_none());
    }
}
