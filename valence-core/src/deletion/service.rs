//! Persistence helpers for `valence_deletion_run` (host platform table).

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::actor::Actor;
use crate::error::{Error, Result};
use crate::query::QueryCore;
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
}
