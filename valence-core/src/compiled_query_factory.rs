//! Dialect-specific compiled queries for deletion DAG and ownership helpers.

use serde_json::Value;

use crate::compiled_query::CompiledQuery;
use crate::error::{Error, Result};
use crate::known_engines::KnownEngines;

fn is_sql_family(engine_id: &str) -> bool {
    matches!(
        engine_id,
        KnownEngines::SQLITE
            | KnownEngines::POSTGRES
            | KnownEngines::INMEMORY_MEM
            | KnownEngines::MONGODB
            | KnownEngines::REDIS
            | KnownEngines::INDRADB
            // Hybrid delegates non-hop compiled queries to its SQL primary,
            // so deletion DAG queries use the SQL dialect.
            | KnownEngines::HYBRID_INDRA_SQL
    )
}

fn is_surreal(engine_id: &str) -> bool {
    engine_id == KnownEngines::SURREALDB
}

#[allow(clippy::unnecessary_wraps)] // fallible when compiler-* features are disabled
fn require_compiler(engine_id: &str, family: &str) -> Result<()> {
    let _ = family;
    if is_sql_family(engine_id) {
        #[cfg(not(feature = "compiler-sql"))]
        return Err(Error::Internal(format!(
            "deletion queries for `{engine_id}` require valence-core/compiler-sql ({family})"
        )));
    } else if is_surreal(engine_id) {
        #[cfg(not(feature = "compiler-surreal"))]
        return Err(Error::Internal(format!(
            "deletion queries for surrealdb require valence-core/compiler-surreal ({family})"
        )));
    }
    Ok(())
}

/// Count M2M edge rows where `in` points at `(root_table, bare_root_id)`.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn count_m2m_edges_from_root(
    engine_id: &str,
    edge_table: &str,
    root_table: &str,
    bare_root_id: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "count_m2m_edges")?;
    if is_surreal(engine_id) {
        let q = format!(
            "SELECT VALUE count FROM (SELECT count() AS count FROM {edge_table} \
             WHERE `in` = type::record($tb, $rid) GROUP ALL)"
        );
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("tb".to_string(), Value::String(root_table.to_string())),
                ("rid".to_string(), Value::String(bare_root_id.to_string())),
            ],
        ));
    }
    if is_sql_family(engine_id) {
        let q = "SELECT COUNT(*) AS count FROM valence_edges \
             WHERE edge_type = $edge_type AND from_table = $tb AND from_id = $rid"
            .to_string();
        return Ok(CompiledQuery::new(
            q,
            vec![
                (
                    "edge_type".to_string(),
                    Value::String(edge_table.to_string()),
                ),
                ("tb".to_string(), Value::String(root_table.to_string())),
                ("rid".to_string(), Value::String(bare_root_id.to_string())),
            ],
        ));
    }
    Ok(CompiledQuery::new(
        format!("/* count_m2m_edges_from_root unsupported for {engine_id} */"),
        vec![],
    ))
}

/// Count rows in `from_table` whose FK equals the target record.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn count_where_thing_eq(
    engine_id: &str,
    from_table: &str,
    fk_field: &str,
    target_table: &str,
    bare_target_id: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "count_where_thing_eq")?;
    if is_surreal(engine_id) {
        let parent_rid = format!("{target_table}:{bare_target_id}");
        let q = format!(
            "SELECT VALUE count FROM (SELECT count() AS count FROM {from_table} \
             WHERE {fk_field} = $parent_rid OR {fk_field} = type::record($ptb, $prid) GROUP ALL)"
        );
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("parent_rid".to_string(), Value::String(parent_rid)),
                ("ptb".to_string(), Value::String(target_table.to_string())),
                (
                    "prid".to_string(),
                    Value::String(bare_target_id.to_string()),
                ),
            ],
        ));
    }
    if is_sql_family(engine_id) {
        let q = format!(
            "SELECT COUNT(*) AS count FROM {from_table} \
             WHERE json_extract(body, '$.{fk_field}') = $bare_id \
                OR json_extract(body, '$.{fk_field}.id') = $bare_id \
                OR json_extract(body, '$.{fk_field}') = $parent_rid"
        );
        let parent_rid = format!("{target_table}:{bare_target_id}");
        return Ok(CompiledQuery::new(
            q,
            vec![
                (
                    "bare_id".to_string(),
                    Value::String(bare_target_id.to_string()),
                ),
                ("parent_rid".to_string(), Value::String(parent_rid)),
            ],
        ));
    }
    Ok(CompiledQuery::new(
        format!("/* count_where_thing_eq unsupported for {engine_id} */"),
        vec![],
    ))
}

/// Select child ids for HasMany reverse lookup.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn select_child_ids_hasmany(
    engine_id: &str,
    child_table: &str,
    reverse_field: &str,
    parent_table: &str,
    bare_parent_id: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "select_child_ids_hasmany")?;
    if is_surreal(engine_id) {
        let parent_rid = format!("{parent_table}:{bare_parent_id}");
        let q = format!(
            "SELECT VALUE id FROM {child_table} \
             WHERE {reverse_field} = $parent_rid OR {reverse_field} = type::record($ptb, $prid)"
        );
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("parent_rid".to_string(), Value::String(parent_rid)),
                ("ptb".to_string(), Value::String(parent_table.to_string())),
                (
                    "prid".to_string(),
                    Value::String(bare_parent_id.to_string()),
                ),
            ],
        ));
    }
    if is_sql_family(engine_id) {
        let q = format!(
            "SELECT id FROM {child_table} \
             WHERE json_extract(body, '$.{reverse_field}') = $parent_rid \
                OR json_extract(body, '$.{reverse_field}.id') = $bare_id \
                OR json_extract(body, '$.{reverse_field}') = $bare_id"
        );
        let parent_rid = format!("{parent_table}:{bare_parent_id}");
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("parent_rid".to_string(), Value::String(parent_rid)),
                (
                    "bare_id".to_string(),
                    Value::String(bare_parent_id.to_string()),
                ),
            ],
        ));
    }
    // Third-party / stub engines without a dialect compiler: empty child set.
    Ok(CompiledQuery::new(
        format!("/* select_child_ids_hasmany unsupported for {engine_id} */"),
        vec![],
    ))
}

/// Select HasOne cascade children ids.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn select_hasone_cascade_children(
    engine_id: &str,
    other: &str,
    from_field: &str,
    parent_table: &str,
    bare_parent_id: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "select_hasone_cascade_children")?;
    if is_surreal(engine_id) {
        let parent_rid = format!("{parent_table}:{bare_parent_id}");
        let q = format!(
            "SELECT VALUE id FROM {other} \
             WHERE {from_field} = $parent_rid OR {from_field} = type::record($tb, $rid)"
        );
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("parent_rid".to_string(), Value::String(parent_rid)),
                ("tb".to_string(), Value::String(parent_table.to_string())),
                ("rid".to_string(), Value::String(bare_parent_id.to_string())),
            ],
        ));
    }
    if is_sql_family(engine_id) {
        let q = format!(
            "SELECT id FROM {other} \
             WHERE json_extract(body, '$.{from_field}') = $parent_rid \
                OR json_extract(body, '$.{from_field}.id') = $bare_id \
                OR json_extract(body, '$.{from_field}') = $bare_id"
        );
        let parent_rid = format!("{parent_table}:{bare_parent_id}");
        return Ok(CompiledQuery::new(
            q,
            vec![
                ("parent_rid".to_string(), Value::String(parent_rid)),
                (
                    "bare_id".to_string(),
                    Value::String(bare_parent_id.to_string()),
                ),
            ],
        ));
    }
    // Third-party / stub engines without a dialect compiler: empty child set.
    Ok(CompiledQuery::new(
        format!("/* select_hasone_cascade_children unsupported for {engine_id} */"),
        vec![],
    ))
}
