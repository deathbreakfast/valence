//! Persistence helpers for `valence_deletion_run` (host platform table).

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::actor::Actor;
use crate::error::{Error, Result};
use crate::query::{QueryCore, SortDirection, StringPredicate};
use crate::runtime::Valence;

fn system_valence(v: &Valence) -> Valence {
    v.with_actor(Actor::System {
        operation: "valence_deletion_run".to_string(),
    })
}

pub struct DeletionService;

impl DeletionService {
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn create_run(
        root_table: &str,
        root_record_id: &str,
        actor_json: Value,
        v: &Valence,
    ) -> Result<String> {
        let run_id = Uuid::new_v4().to_string();
        let requested_by = actor_json.to_string();
        let sys = system_valence(v);
        let backend = sys.backend_for_table("valence_deletion_run")?;
        let row = json!({
            "id": run_id,
            "root_table": root_table,
            "root_record_id": root_record_id,
            "status": "queued",
            "total_steps": 0,
            "completed_steps": 0,
            "failed_steps": 0,
            "requested_by": requested_by,
            "requested_at": Utc::now(),
        });
        backend
            .create_record("valence_deletion_run", row)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(run_id)
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_run_json(run_id: &str, v: &Valence) -> Result<Option<Value>> {
        let sys = system_valence(v);
        QueryCore::get_record_json("valence_deletion_run", run_id, &sys)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn merge_run(run_id: &str, patch: Value, v: &Valence) -> Result<()> {
        let sys = system_valence(v);
        let backend = sys.backend_for_table("valence_deletion_run")?;
        backend
            .merge_record("valence_deletion_run", run_id, patch)
            .await
            .map_err(|e| Error::Database(e.to_string()))
            .map(|_| ())
    }

    /// Runs for a specific root entity (most recent first).
    ///
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn list_runs_for_record(
        root_table: &str,
        root_record_id: &str,
        v: &Valence,
    ) -> Result<Vec<Value>> {
        let sys = system_valence(v);
        QueryCore::new("valence_deletion_run".to_string())
            .where_string(
                "root_table".to_string(),
                StringPredicate::Equals(root_table.to_string()),
            )
            .where_string(
                "root_record_id".to_string(),
                StringPredicate::Equals(root_record_id.to_string()),
            )
            .order_by("requested_at".to_string(), SortDirection::Desc)
            .limit(50)
            .execute(&sys)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    /// Recent runs for a logical schema (matches `root_table`).
    ///
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn list_runs_for_schema(schema_table: &str, v: &Valence) -> Result<Vec<Value>> {
        let sys = system_valence(v);
        QueryCore::new("valence_deletion_run".to_string())
            .where_string(
                "root_table".to_string(),
                StringPredicate::Equals(schema_table.to_string()),
            )
            .order_by("requested_at".to_string(), SortDirection::Desc)
            .limit(50)
            .execute(&sys)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }

    /// Count in-flight runs requested by the given actor JSON (`requested_by` field).
    ///
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "backend count values may be represented as floating-point JSON"
    )]
    pub async fn count_active_runs_for_requester(requested_by: &str, v: &Valence) -> Result<u64> {
        let sys = system_valence(v);
        let backend = sys.backend_for_table("valence_deletion_run")?;
        let compiled = crate::compiled_query_factory::count_active_deletion_runs_for_requester(
            backend.engine_id(),
            requested_by,
        )?;
        let rows = backend
            .execute_compiled_query(&compiled)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(rows
            .first()
            .and_then(|row| {
                row.as_u64()
                    .or_else(|| row.as_i64().map(|n| n as u64))
                    .or_else(|| row.as_f64().map(|f| f as u64))
                    .or_else(|| {
                        row.get("n")
                            .or_else(|| row.get("count"))
                            .and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)))
                    })
            })
            .unwrap_or(0))
    }

    /// Paged listing for admin UI (newest first).
    ///
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn list_runs_recent(limit: u32, v: &Valence) -> Result<Vec<Value>> {
        let sys = system_valence(v);
        QueryCore::new("valence_deletion_run".to_string())
            .order_by("requested_at".to_string(), SortDirection::Desc)
            .limit(limit)
            .execute(&sys)
            .await
            .map_err(|e| Error::Database(e.to_string()))
    }
}
