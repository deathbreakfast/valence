//! Live row-count queries and id extraction for deletion graph construction.

use std::sync::Arc;

use serde_json::Value;

use crate::backend::DatabaseBackend;
use crate::compiled_query_factory::{
    count_m2m_edges_from_root as compile_count_m2m, count_where_thing_eq as compile_count_fk,
    select_child_ids_hasmany as compile_select_hasmany,
    select_hasone_cascade_children as compile_select_hasone,
};
use crate::error::{Error, Result};
use crate::runtime::Valence;

use super::validate::assert_safe_ident;

fn query_err_is_missing_table(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("no such table")
        || (m.contains("does not exist") && (m.contains("table") || m.contains("relation")))
}

/// Resolve backend for ad-hoc compiled queries used while building the deletion graph.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn backend_for_deletion_query(v: &Valence, table: &str) -> Result<Arc<dyn DatabaseBackend>> {
    match v.backend_for_table(table) {
        Ok(b) => Ok(b),
        Err(Error::NotFound(_)) => v.active_backend(),
        Err(e) => Err(e),
    }
}

fn strip_surreal_thing_decorations(s: &str) -> String {
    let mut t = s
        .trim()
        .trim_start_matches('⟨')
        .trim_end_matches('⟩')
        .to_string();
    if t.len() >= 2 && t.starts_with('`') && t.ends_with('`') {
        t = t[1..t.len() - 1].to_string();
    }
    t
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn bare_id_from_query_cell(value: &Value) -> String {
    if let Some(s) = value.as_str() {
        let bare = strip_surreal_thing_decorations(s);
        if let Some((_table, id)) = bare.split_once(':') {
            if !id.is_empty() {
                return strip_surreal_thing_decorations(id);
            }
        }
        return bare;
    }
    if let Some(obj) = value.as_object() {
        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
            return strip_surreal_thing_decorations(id);
        }
    }
    let s = value.to_string();
    let tail = s.rsplit(':').next().unwrap_or(&s);
    strip_surreal_thing_decorations(tail)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "backend count values may be represented as floating-point JSON"
)]
async fn run_count_query(
    backend: &dyn DatabaseBackend,
    compiled: crate::compiled_query::CompiledQuery,
) -> Result<i64> {
    let rows = match backend.execute_compiled_query(&compiled).await {
        Ok(rows) => rows,
        Err(e) if query_err_is_missing_table(&e.to_string()) => return Ok(0),
        Err(e) => return Err(Error::Database(e.to_string())),
    };
    Ok(rows
        .into_iter()
        .next()
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.get("count").and_then(|c| c.as_i64()))
                .or_else(|| v.as_f64().map(|f| f as i64))
        })
        .unwrap_or(0))
}

async fn run_id_query(
    backend: &dyn DatabaseBackend,
    compiled: crate::compiled_query::CompiledQuery,
) -> Result<Vec<String>> {
    let rows = match backend.execute_compiled_query(&compiled).await {
        Ok(rows) => rows,
        Err(e) if query_err_is_missing_table(&e.to_string()) => return Ok(vec![]),
        Err(e) => return Err(Error::Database(e.to_string())),
    };
    let mut out = Vec::new();
    for cell in rows {
        out.push(bare_id_from_query_cell(&cell));
    }
    Ok(out)
}

/// Count M2M edge rows where `in` points at `(root_table, bare_root_id)`.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn count_m2m_edges_from_root(
    v: &Valence,
    edge_table: &str,
    root_table: &str,
    bare_root_id: &str,
) -> Result<i64> {
    assert_safe_ident(edge_table)?;
    assert_safe_ident(root_table)?;
    let backend = backend_for_deletion_query(v, edge_table)?;
    let compiled = compile_count_m2m(backend.engine_id(), edge_table, root_table, bare_root_id)?;
    run_count_query(backend.as_ref(), compiled).await
}

pub async fn count_where_thing_eq(
    v: &Valence,
    from_table: &str,
    fk_field: &str,
    target_table: &str,
    bare_target_id: &str,
) -> Result<i64> {
    assert_safe_ident(from_table)?;
    assert_safe_ident(fk_field)?;
    assert_safe_ident(target_table)?;
    let backend = backend_for_deletion_query(v, from_table)?;
    let compiled = compile_count_fk(
        backend.engine_id(),
        from_table,
        fk_field,
        target_table,
        bare_target_id,
    )?;
    run_count_query(backend.as_ref(), compiled).await
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn select_child_ids_hasmany(
    v: &Valence,
    child_table: &str,
    reverse_field: &str,
    parent_table: &str,
    bare_parent_id: &str,
) -> Result<Vec<String>> {
    assert_safe_ident(child_table)?;
    assert_safe_ident(reverse_field)?;
    assert_safe_ident(parent_table)?;
    let backend = backend_for_deletion_query(v, child_table)?;
    let compiled = compile_select_hasmany(
        backend.engine_id(),
        child_table,
        reverse_field,
        parent_table,
        bare_parent_id,
    )?;
    run_id_query(backend.as_ref(), compiled).await
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn select_hasone_cascade_children(
    v: &Valence,
    other: &str,
    from_field: &str,
    parent_table: &str,
    bare_parent_id: &str,
) -> Result<Vec<String>> {
    assert_safe_ident(other)?;
    assert_safe_ident(from_field)?;
    let backend = backend_for_deletion_query(v, other)?;
    let compiled = compile_select_hasone(
        backend.engine_id(),
        other,
        from_field,
        parent_table,
        bare_parent_id,
    )?;
    run_id_query(backend.as_ref(), compiled).await
}
