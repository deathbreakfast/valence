//! Build a deletion DAG from [`crate::schema::SchemaRegistry`] metadata and live row counts.

mod build;
mod plan;
mod validate;

pub use plan::{DeletionAction, DeletionDag, DeletionNode, RestrictViolation};
pub use validate::{table_skips_pending_deletion_filter, SKIP_DELETION_GRAPH_TABLES};

use std::collections::{HashMap, VecDeque};

use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::{schema_connections_for_table, SchemaRegistry};
use crate::trait_registry::TraitRegistry;

use build::{
    count_m2m_edges_from_root, count_where_thing_eq, select_child_ids_hasmany,
    select_hasone_cascade_children,
};
use validate::{assert_safe_bare_thing_id, assert_safe_ident, skip_graph_table};

/// Child tables for a connection `to_table`, expanding `trait:TraitName` via [`TraitRegistry`].
fn connection_child_tables(
    conn: &crate::schema_api::SchemaConnection,
    traits: &TraitRegistry,
) -> Vec<String> {
    if let Some(trait_name) = conn.to_table.strip_prefix("trait:") {
        traits
            .tables_for_trait(trait_name)
            .into_iter()
            .map(str::to_string)
            .collect()
    } else {
        vec![conn.to_table.clone()]
    }
}

fn conn_targets_table(
    conn: &crate::schema_api::SchemaConnection,
    table: &str,
    traits: &TraitRegistry,
) -> bool {
    conn.to_table == table
        || conn.to_table.strip_prefix("trait:").is_some_and(|t| {
            traits
                .tables_for_trait(t)
                .into_iter()
                .any(|tbl| tbl == table)
        })
}

/// When the root row merges a HasMany Cascade that will delete children via `reverse_field`,
/// skip incoming HasOne Restrict on those child rows (parent-driven cascade covers cleanup).
fn root_parent_cascade_covers_child_restrict(
    root_meta: &crate::schema::SchemaMetadata,
    traits: &TraitRegistry,
    child_table: &str,
    child_conn: &crate::schema_api::SchemaConnection,
) -> bool {
    for conn in schema_connections_for_table(root_meta).iter() {
        if conn.cardinality != "HasMany" {
            continue;
        }
        if !conn.on_delete.eq_ignore_ascii_case("cascade") {
            continue;
        }
        let Some(rf) = conn.reverse_field.as_deref() else {
            continue;
        };
        if rf != child_conn.from_field.as_str() {
            continue;
        }
        let child_tables = connection_child_tables(conn, traits);
        if child_tables.iter().any(|t| t == child_table) {
            return true;
        }
    }
    false
}

async fn collect_outgoing_hasmany_restrict_violations(
    v: &Valence,
    root_table: &str,
    root_record_id: &str,
    meta: &crate::schema::SchemaMetadata,
    traits: &TraitRegistry,
) -> Result<Vec<RestrictViolation>> {
    let mut violations = Vec::new();
    for conn in schema_connections_for_table(meta).iter() {
        if conn.cardinality != "HasMany" || !conn.on_delete.eq_ignore_ascii_case("restrict") {
            continue;
        }
        let Some(rf) = conn.reverse_field.as_deref() else {
            continue;
        };
        let child_tables = connection_child_tables(conn, traits);
        let mut total = 0i64;
        for child_table in &child_tables {
            total += count_where_thing_eq(v, child_table, rf, root_table, root_record_id).await?;
        }
        if total > 0 {
            violations.push(RestrictViolation {
                blocking_table: conn.to_table.clone(),
                blocking_field: rf.to_string(),
                blocking_record_count: total,
                connection_name: conn.name.clone(),
            });
        }
    }
    Ok(violations)
}

async fn collect_m2m_restrict_violations(
    v: &Valence,
    root_table: &str,
    root_record_id: &str,
    meta: &crate::schema::SchemaMetadata,
) -> Result<Vec<RestrictViolation>> {
    let mut violations = Vec::new();
    for conn in schema_connections_for_table(meta).iter() {
        if conn.cardinality != "ManyToMany" || !conn.on_delete.eq_ignore_ascii_case("restrict") {
            continue;
        }
        let Some(edge) = conn.edge_table.as_deref() else {
            continue;
        };
        let n = count_m2m_edges_from_root(v, edge, root_table, root_record_id).await?;
        if n > 0 {
            violations.push(RestrictViolation {
                blocking_table: edge.to_string(),
                blocking_field: "in".to_string(),
                blocking_record_count: n,
                connection_name: conn.name.clone(),
            });
        }
    }
    Ok(violations)
}

async fn collect_incoming_hasone_restrict_violations(
    v: &Valence,
    root_table: &str,
    root_record_id: &str,
    root_meta: Option<&crate::schema::SchemaMetadata>,
    registry: &SchemaRegistry,
    traits: &TraitRegistry,
) -> Result<Vec<RestrictViolation>> {
    let mut violations = Vec::new();
    for table_name in registry.list_schemas() {
        if skip_graph_table(table_name) {
            continue;
        }
        let Some(meta) = registry.get_schema(table_name) else {
            continue;
        };
        for conn in schema_connections_for_table(meta).iter() {
            if conn.cardinality != "HasOne" || !conn_targets_table(conn, root_table, traits) {
                continue;
            }
            if !conn.on_delete.eq_ignore_ascii_case("restrict") {
                continue;
            }
            if let Some(root_meta) = root_meta {
                if root_parent_cascade_covers_child_restrict(root_meta, traits, table_name, conn) {
                    continue;
                }
            }
            let n =
                count_where_thing_eq(v, table_name, &conn.from_field, root_table, root_record_id)
                    .await?;
            if n > 0 {
                violations.push(RestrictViolation {
                    blocking_table: table_name.to_string(),
                    blocking_field: conn.from_field.clone(),
                    blocking_record_count: n,
                    connection_name: conn.name.clone(),
                });
            }
        }
    }
    Ok(violations)
}

async fn expand_outgoing_hasmany_cascade(
    v: &Valence,
    tbl: &str,
    rid: &str,
    depth: u32,
    meta: &crate::schema::SchemaMetadata,
    traits: &TraitRegistry,
    visited: &mut HashMap<(String, String), u32>,
    queue: &mut VecDeque<(String, String, u32)>,
) -> Result<()> {
    for conn in schema_connections_for_table(meta).iter() {
        if conn.cardinality != "HasMany" || !conn.on_delete.eq_ignore_ascii_case("cascade") {
            continue;
        }
        let Some(rf) = conn.reverse_field.as_deref() else {
            continue;
        };
        for child_table in connection_child_tables(conn, traits) {
            if skip_graph_table(&child_table) {
                continue;
            }
            let kids = select_child_ids_hasmany(v, &child_table, rf, tbl, rid).await?;
            for kid in kids {
                let key = (child_table.clone(), kid.clone());
                if visited.contains_key(&key) {
                    continue;
                }
                visited.insert(key.clone(), depth + 1);
                queue.push_back((child_table.clone(), kid, depth + 1));
            }
        }
    }
    Ok(())
}

async fn expand_incoming_hasone_cascade(
    v: &Valence,
    tbl: &str,
    rid: &str,
    depth: u32,
    registry: &SchemaRegistry,
    traits: &TraitRegistry,
    visited: &mut HashMap<(String, String), u32>,
    queue: &mut VecDeque<(String, String, u32)>,
) -> Result<()> {
    for other in registry.list_schemas() {
        if skip_graph_table(other) {
            continue;
        }
        let Some(om) = registry.get_schema(other) else {
            continue;
        };
        for conn in schema_connections_for_table(om).iter() {
            if conn.cardinality != "HasOne" || !conn.on_delete.eq_ignore_ascii_case("cascade") {
                continue;
            }
            if !conn_targets_table(conn, tbl, traits) || skip_graph_table(other) {
                continue;
            }
            for kid in select_hasone_cascade_children(v, other, &conn.from_field, tbl, rid).await? {
                let key = (other.to_string(), kid.clone());
                if visited.contains_key(&key) {
                    continue;
                }
                visited.insert(key.clone(), depth + 1);
                queue.push_back((other.to_string(), kid, depth + 1));
            }
        }
    }
    Ok(())
}

async fn bfs_cascade_expansion(
    v: &Valence,
    root_table: &str,
    root_record_id: &str,
    registry: &SchemaRegistry,
    traits: &TraitRegistry,
) -> Result<HashMap<(String, String), u32>> {
    let mut visited: HashMap<(String, String), u32> = HashMap::new();
    let mut queue = VecDeque::new();
    visited.insert((root_table.to_string(), root_record_id.to_string()), 0);
    queue.push_back((root_table.to_string(), root_record_id.to_string(), 0u32));

    while let Some((tbl, rid, depth)) = queue.pop_front() {
        if skip_graph_table(&tbl) {
            continue;
        }
        let Some(meta) = registry.get_schema(&tbl) else {
            continue;
        };
        expand_outgoing_hasmany_cascade(
            v,
            &tbl,
            &rid,
            depth,
            meta,
            traits,
            &mut visited,
            &mut queue,
        )
        .await?;
        expand_incoming_hasone_cascade(
            v,
            &tbl,
            &rid,
            depth,
            registry,
            traits,
            &mut visited,
            &mut queue,
        )
        .await?;
    }
    Ok(visited)
}

impl DeletionDag {
    /// Compute cascade nodes and `Restrict` violations for deleting `(root_table, root_record_id)`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn compute(root_table: &str, root_record_id: &str, v: &Valence) -> Result<Self> {
        Self::compute_with_registry(
            root_table,
            root_record_id,
            v,
            SchemaRegistry::global(),
            TraitRegistry::global(),
        )
        .await
    }

    /// Like [`Self::compute`], but uses the provided registries (for tests and tooling).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn compute_with_registry(
        root_table: &str,
        root_record_id: &str,
        v: &Valence,
        registry: &SchemaRegistry,
        traits: &TraitRegistry,
    ) -> Result<Self> {
        if skip_graph_table(root_table) {
            return Err(Error::Validation(format!(
                "deletion graph not supported for platform table {root_table:?}"
            )));
        }
        assert_safe_ident(root_table)?;
        assert_safe_bare_thing_id(root_record_id)?;

        let root_meta = registry.get_schema(root_table);
        let mut violations = Vec::new();

        if let Some(meta) = root_meta {
            violations.extend(
                collect_outgoing_hasmany_restrict_violations(
                    v,
                    root_table,
                    root_record_id,
                    meta,
                    traits,
                )
                .await?,
            );
            violations.extend(
                collect_m2m_restrict_violations(v, root_table, root_record_id, meta).await?,
            );
        }

        violations.extend(
            collect_incoming_hasone_restrict_violations(
                v,
                root_table,
                root_record_id,
                root_meta,
                registry,
                traits,
            )
            .await?,
        );

        if !violations.is_empty() {
            #[cfg(feature = "instrumentation")]
            crate::instrumentation::record_dag_computed(
                root_table,
                root_record_id,
                0,
                0,
                violations.len(),
                0,
            );
            return Ok(Self {
                root_table: root_table.to_string(),
                root_record_id: root_record_id.to_string(),
                nodes: Vec::new(),
                restrict_violations: violations,
            });
        }

        let visited =
            bfs_cascade_expansion(v, root_table, root_record_id, registry, traits).await?;
        let dag = Self::nodes_from_visited(root_table, root_record_id, visited, violations);
        #[cfg(feature = "instrumentation")]
        {
            let max_depth = dag.nodes.iter().map(|n| n.depth).max().unwrap_or(0) as usize;
            crate::instrumentation::record_dag_computed(
                root_table,
                root_record_id,
                dag.nodes.len(),
                max_depth,
                0,
                0,
            );
        }
        Ok(dag)
    }
}
