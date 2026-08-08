//! Adapter CRUD, compiled query, union/join IR, M2M edge smoke.

use valence_core::compiled_query::CompiledQuery;
use valence_core::query::QueryCore;
use valence_core::record_id::RecordId;
use valence_core::storage_layout::{FieldStorage, LayoutField, StorageLayout};
use valence_core::StringPredicate;

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::CrudSmoke { table, id } => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            // Wire stores are shared across matrix rows; clear leftovers from prior runs.
            let _ = backend.delete_record(table, id).await;
            backend
                .create_record(table, serde_json::json!({"id": id, "name": "smoke"}))
                .await
                .map_err(|e| e.to_string())?;
            let fetched = backend
                .get_record(table, id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "record missing after create".to_string())?;
            if mode == RunMode::Correctness {
                assert_eq!(fetched.get("name").and_then(|v| v.as_str()), Some("smoke"));
            }
        }
        ScenarioStep::AssertGetMissing { table, id } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let fetched = backend
                .get_record(table, id)
                .await
                .map_err(|e| e.to_string())?;
            if fetched.is_some() {
                return Err(format!("expected missing record {table}:{id}"));
            }
        }
        ScenarioStep::CompiledQueryEmpty { table } => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let compiled = CompiledQuery::new(format!("SELECT * FROM {table} LIMIT 10"), vec![]);
            let rows = backend
                .execute_compiled_query(&compiled)
                .await
                .map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness && !rows.is_empty() {
                return Err(format!(
                    "expected empty query result, got {} rows",
                    rows.len()
                ));
            }
        }
        ScenarioStep::QueryUnionJoinSmoke => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let left = QueryCore::new("project".into())
                .where_string("name".into(), StringPredicate::Equals("a".into()));
            let right = QueryCore::new("project".into())
                .where_string("name".into(), StringPredicate::Equals("b".into()));
            let joined = left.clone().join_with(right.clone());
            let unioned = left.union_with(right);
            if joined.where_clauses.len() < 2 {
                return Err("join_with should combine where clauses".into());
            }
            if unioned.or_groups.is_empty() && unioned.where_clauses.is_empty() {
                return Err("union_with should produce or_groups or clauses".into());
            }
        }
        ScenarioStep::M2mRelateSmoke => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            if !backend.capabilities().supports_graph_edges {
                return Ok(());
            }
            let table = "m2m_smoke_node";
            // Wire stores are shared across matrix rows; clear leftovers from prior runs.
            let _ = backend.delete_record(table, "a").await;
            let _ = backend.delete_record(table, "b").await;
            backend
                .create_record(table, serde_json::json!({"id": "a", "name": "a"}))
                .await
                .map_err(|e| e.to_string())?;
            backend
                .create_record(table, serde_json::json!({"id": "b", "name": "b"}))
                .await
                .map_err(|e| e.to_string())?;
            let from = RecordId::new(table, "a");
            let to = RecordId::new(table, "b");
            backend
                .relate_edge(&from, "m2m_edge", &to)
                .await
                .map_err(|e| e.to_string())?;
            let targets = backend
                .get_edge_targets(&from, "m2m_edge")
                .await
                .map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness && targets.is_empty() {
                return Err("expected M2M edge targets after relate".into());
            }
            backend
                .unrelate_edge(&from, "m2m_edge", &to)
                .await
                .map_err(|e| e.to_string())?;
        }
        ScenarioStep::TypedSyncAddField { table } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let layout_v1 = StorageLayout {
                table: table.clone(),
                fields: vec![
                    LayoutField {
                        name: "id".into(),
                        storage: FieldStorage::String,
                        primary_key: true,
                        nullable: false,
                        unique: true,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                    LayoutField {
                        name: "name".into(),
                        storage: FieldStorage::String,
                        primary_key: false,
                        nullable: true,
                        unique: false,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                ],
            };
            backend
                .ensure_typed_table(&layout_v1)
                .await
                .map_err(|e| e.to_string())?;
            let _ = backend.delete_record(table, "r1").await;
            backend
                .create_record(
                    table,
                    serde_json::json!({
                        "id": {"table": table, "id": "r1"},
                        "name": "alpha"
                    }),
                )
                .await
                .map_err(|e| e.to_string())?;

            let mut layout_v2 = layout_v1.clone();
            layout_v2.fields.push(LayoutField {
                name: "score".into(),
                storage: FieldStorage::Integer,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: None,
            record_table: None,
            });
            backend
                .sync_typed_table(&layout_v2)
                .await
                .map_err(|e| e.to_string())?;

            if let Some(inspected) = backend
                .inspect_typed_layout(table)
                .await
                .map_err(|e| e.to_string())?
            {
                if !inspected.fields.iter().any(|f| f.name == "score") {
                    return Err(format!(
                        "score field missing after sync on {table}: {inspected:?}"
                    ));
                }
            }

            backend
                .update_record(
                    table,
                    "r1",
                    serde_json::json!({"name": "alpha", "score": 7}),
                )
                .await
                .map_err(|e| e.to_string())?;
            let got = backend
                .get_record(table, "r1")
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("row missing after typed sync update on {table}"))?;
            let score = got.get("score").and_then(|v| v.as_i64());
            if score != Some(7) {
                return Err(format!("expected score=7 after typed sync, got {got}"));
            }
        }
        ScenarioStep::SchemaVersionSkip => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let table = crate::CATALOG_TTL_PROBE_TABLE;
            let meta = valence_core::schema::SchemaRegistry::global()
                .get_schema(table)
                .ok_or_else(|| format!("SchemaRegistry missing {table}"))?;
            let backend = valence
                .backend_for_table(table)
                .map_err(|e| e.to_string())?;
            let sql_stamp = matches!(
                backend.engine_id(),
                valence_core::KnownEngines::SQLITE | valence_core::KnownEngines::POSTGRES
            );
            let before = backend
                .read_schema_version(table)
                .await
                .map_err(|e| e.to_string())?;
            if sql_stamp {
                let stamp = before.as_deref().ok_or_else(|| {
                    format!("schema-version-skip: expected stamp on SQL engine after boot sync")
                })?;
                if stamp != meta.version {
                    return Err(format!(
                        "expected stamp {} after boot sync, got {stamp}",
                        meta.version
                    ));
                }
            } else if let Some(stamp) = before.as_deref() {
                if stamp != meta.version {
                    return Err(format!(
                        "expected stamp {} after boot sync, got {stamp}",
                        meta.version
                    ));
                }
            }
            valence_core::storage_layout::sync_typed_table_for(valence, table)
                .await
                .map_err(|e| e.to_string())?;
            let after = backend
                .read_schema_version(table)
                .await
                .map_err(|e| e.to_string())?;
            if before != after {
                return Err(format!(
                    "schema-version-skip: stamp changed {before:?} → {after:?}"
                ));
            }
        }
        ScenarioStep::SchemaVersionBumpAddField { table } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            let sql_stamp = matches!(
                backend.engine_id(),
                valence_core::KnownEngines::SQLITE | valence_core::KnownEngines::POSTGRES
            );
            let layout_v1 = StorageLayout {
                table: table.clone(),
                fields: vec![
                    LayoutField {
                        name: "id".into(),
                        storage: FieldStorage::String,
                        primary_key: true,
                        nullable: false,
                        unique: true,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                    LayoutField {
                        name: "name".into(),
                        storage: FieldStorage::String,
                        primary_key: false,
                        nullable: true,
                        unique: false,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                ],
            };
            backend
                .ensure_typed_table(&layout_v1)
                .await
                .map_err(|e| e.to_string())?;
            backend
                .write_schema_version(table, "1.0.0")
                .await
                .map_err(|e| e.to_string())?;
            let stamped = backend
                .read_schema_version(table)
                .await
                .map_err(|e| e.to_string())?;
            if sql_stamp {
                if stamped.as_deref() != Some("1.0.0") {
                    return Err(format!("expected stamp 1.0.0 on SQL, got {stamped:?}"));
                }
            } else if stamped.is_some() && stamped.as_deref() != Some("1.0.0") {
                return Err(format!("expected stamp 1.0.0, got {stamped:?}"));
            }

            let mut layout_v2 = layout_v1.clone();
            layout_v2.fields.push(LayoutField {
                name: "score".into(),
                storage: FieldStorage::Integer,
                primary_key: false,
                nullable: true,
                unique: false,
                indexed: false,
                default: None,
            record_table: None,
            });
            backend
                .sync_typed_table(&layout_v2)
                .await
                .map_err(|e| e.to_string())?;
            backend
                .write_schema_version(table, "1.1.0")
                .await
                .map_err(|e| e.to_string())?;

            let inspected = backend
                .inspect_typed_layout(table)
                .await
                .map_err(|e| e.to_string())?;
            if sql_stamp {
                let inspected = inspected.ok_or_else(|| {
                    format!("schema-version-bump: expected inspect layout on SQL for {table}")
                })?;
                if !inspected.fields.iter().any(|f| f.name == "score") {
                    return Err(format!(
                        "score field missing after version bump sync on {table}"
                    ));
                }
            } else if let Some(inspected) = inspected {
                if !inspected.fields.iter().any(|f| f.name == "score") {
                    return Err(format!(
                        "score field missing after version bump sync on {table}"
                    ));
                }
            }
            let after = backend
                .read_schema_version(table)
                .await
                .map_err(|e| e.to_string())?;
            if sql_stamp {
                if after.as_deref() != Some("1.1.0") {
                    return Err(format!(
                        "expected stamp 1.1.0 on SQL after bump, got {after:?}"
                    ));
                }
            } else if after.is_some() && after.as_deref() != Some("1.1.0") {
                return Err(format!("expected stamp 1.1.0 after bump, got {after:?}"));
            }
        }
        ScenarioStep::SchemaVersionSqliteNullabilityRefuse { table } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            let backend = valence.active_backend().map_err(|e| e.to_string())?;
            if backend.engine_id() != valence_core::KnownEngines::SQLITE {
                return Ok(());
            }
            let live = StorageLayout {
                table: table.clone(),
                fields: vec![
                    LayoutField {
                        name: "id".into(),
                        storage: FieldStorage::String,
                        primary_key: true,
                        nullable: false,
                        unique: true,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                    LayoutField {
                        name: "name".into(),
                        storage: FieldStorage::String,
                        primary_key: false,
                        nullable: false,
                        unique: false,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                ],
            };
            backend
                .ensure_typed_table(&live)
                .await
                .map_err(|e| e.to_string())?;
            let desired = StorageLayout {
                table: table.clone(),
                fields: vec![
                    LayoutField {
                        name: "id".into(),
                        storage: FieldStorage::String,
                        primary_key: true,
                        nullable: false,
                        unique: true,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                    LayoutField {
                        name: "name".into(),
                        storage: FieldStorage::String,
                        primary_key: false,
                        nullable: true,
                        unique: false,
                        indexed: false,
                        default: None,
                    record_table: None,
                    },
                ],
            };
            match backend.sync_typed_table(&desired).await {
                Ok(()) => {
                    return Err("expected Validation refusing SQLite nullability change".into());
                }
                Err(e) => {
                    let msg = e.to_string();
                    if !msg.to_ascii_lowercase().contains("nullability") {
                        return Err(format!("expected nullability Validation, got: {msg}"));
                    }
                }
            }
        }
        other => return Err(format!("crud step mismatch: {other:?}")),
    }
    Ok(())
}
