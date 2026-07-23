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

/// Count ownership sidecar rows for one schema / owner / status.
///
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn count_ownership_rows_for_schema(
    engine_id: &str,
    valence_model: &str,
    owner_ids: &[String],
    owner_type: &str,
    status: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "count_ownership_rows_for_schema")?;
    if is_surreal(engine_id) {
        let q = concat!(
            "SELECT count() AS n FROM valence_data_ownership ",
            "WHERE valence_model = $model AND owner_id IN $owner_ids ",
            "AND owner_type = $owner_type AND status = $status GROUP ALL"
        );
        return Ok(CompiledQuery::new(
            q.to_string(),
            vec![
                (
                    "model".to_string(),
                    Value::String(valence_model.to_string()),
                ),
                (
                    "owner_ids".to_string(),
                    Value::Array(owner_ids.iter().cloned().map(Value::String).collect()),
                ),
                (
                    "owner_type".to_string(),
                    Value::String(owner_type.to_string()),
                ),
                ("status".to_string(), Value::String(status.to_string())),
            ],
        ));
    }
    if is_sql_family(engine_id) {
        let mut params = vec![
            (
                "model".to_string(),
                Value::String(valence_model.to_string()),
            ),
            (
                "owner_type".to_string(),
                Value::String(owner_type.to_string()),
            ),
            ("status".to_string(), Value::String(status.to_string())),
        ];
        let mut owner_ors = Vec::with_capacity(owner_ids.len());
        for (i, owner_id) in owner_ids.iter().enumerate() {
            let key = format!("owner_id_{i}");
            owner_ors.push(format!("json_extract(body, '$.owner_id') = ${key}"));
            params.push((key, Value::String(owner_id.clone())));
        }
        let owner_clause = match owner_ors.as_slice() {
            [] => "0".to_string(),
            [only] => only.clone(),
            many => format!("({})", many.join(" OR ")),
        };
        let q = format!(
            "SELECT COUNT(*) AS n FROM valence_data_ownership \
             WHERE json_extract(body, '$.valence_model') = $model \
               AND {owner_clause} \
               AND json_extract(body, '$.owner_type') = $owner_type \
               AND json_extract(body, '$.status') = $status"
        );
        return Ok(CompiledQuery::new(q, params));
    }
    Ok(CompiledQuery::new(
        format!("/* count_ownership_rows_for_schema unsupported for {engine_id} */"),
        vec![],
    ))
}

const ACTIVE_DELETION_STATUSES: &[&str] = &["queued", "scanning", "processing"];

/// Count in-flight deletion runs for a requester JSON string.
///
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn count_active_deletion_runs_for_requester(
    engine_id: &str,
    requested_by: &str,
) -> Result<CompiledQuery> {
    require_compiler(engine_id, "count_active_deletion_runs_for_requester")?;
    if is_surreal(engine_id) {
        let q = concat!(
            "SELECT count() AS n FROM valence_deletion_run ",
            "WHERE requested_by = $requested_by ",
            "AND status IN ['queued', 'scanning', 'processing'] GROUP ALL"
        );
        return Ok(CompiledQuery::new(
            q.to_string(),
            vec![(
                "requested_by".to_string(),
                Value::String(requested_by.to_string()),
            )],
        ));
    }
    if is_sql_family(engine_id) {
        let mut params = vec![(
            "requested_by".to_string(),
            Value::String(requested_by.to_string()),
        )];
        let mut status_ors = Vec::with_capacity(ACTIVE_DELETION_STATUSES.len());
        for (i, status) in ACTIVE_DELETION_STATUSES.iter().enumerate() {
            let key = format!("status_{i}");
            status_ors.push(format!("json_extract(body, '$.status') = ${key}"));
            params.push((key, Value::String((*status).to_string())));
        }
        let status_clause = status_ors.join(" OR ");
        let q = format!(
            "SELECT COUNT(*) AS n FROM valence_deletion_run \
             WHERE json_extract(body, '$.requested_by') = $requested_by \
               AND ({status_clause})"
        );
        return Ok(CompiledQuery::new(q, params));
    }
    Ok(CompiledQuery::new(
        format!("/* count_active_deletion_runs_for_requester unsupported for {engine_id} */"),
        vec![],
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::known_engines::KnownEngines;

    #[test]
    fn ownership_count_sql_uses_count_star_and_json_extract() {
        let cq = count_ownership_rows_for_schema(
            KnownEngines::SQLITE,
            "task",
            &["user:abc".into(), "abc".into()],
            "user",
            "active",
        )
        .expect("compile");
        assert!(cq.query_string.contains("COUNT(*)"), "{}", cq.query_string);
        assert!(
            !cq.query_string.contains("GROUP ALL"),
            "{}",
            cq.query_string
        );
        assert!(cq.query_string.contains("json_extract(body, '$.owner_id')"));
        assert_eq!(cq.params.len(), 5);
    }

    #[test]
    fn ownership_count_surreal_keeps_group_all() {
        let cq = count_ownership_rows_for_schema(
            KnownEngines::SURREALDB,
            "task",
            &["user:abc".into()],
            "user",
            "active",
        )
        .expect("compile");
        assert!(cq.query_string.contains("count()"));
        assert!(cq.query_string.contains("GROUP ALL"));
        assert!(cq.query_string.contains("IN $owner_ids"));
    }

    #[test]
    fn active_deletion_count_sql_avoids_surreal_in_list() {
        let cq = count_active_deletion_runs_for_requester(
            KnownEngines::SQLITE,
            r#"{"User":{"user_id":"x"}}"#,
        )
        .expect("compile");
        assert!(cq.query_string.contains("COUNT(*)"), "{}", cq.query_string);
        assert!(
            !cq.query_string.contains("GROUP ALL"),
            "{}",
            cq.query_string
        );
        assert!(!cq.query_string.contains("IN ["), "{}", cq.query_string);
        assert!(cq.query_string.contains("json_extract(body, '$.status')"));
    }
}
