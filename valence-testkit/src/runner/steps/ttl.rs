//! Schema TTL matrix contracts (native expire vs Deferred linger vs Unsupported).

use std::time::Duration;

use valence_core::ttl::{
    reset_ttl_warn_state_for_tests, ttl_warn_emit_count_for_tests, BackendTtlCapability,
    EXPIRE_AT_FIELD,
};

use crate::bootstrap::BootstrapSession;
use crate::fixtures::{CATALOG_TTL_PROBE_SECONDS, CATALOG_TTL_PROBE_TABLE};
use crate::matrix::StorageAdapter;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) fn expected_ttl_capability(storage: StorageAdapter) -> BackendTtlCapability {
    match storage {
        StorageAdapter::Redis | StorageAdapter::MongoDb => BackendTtlCapability::SupportedNative,
        StorageAdapter::IndraDb | StorageAdapter::AcmeStub => BackendTtlCapability::Unsupported,
        StorageAdapter::Mem
        | StorageAdapter::Sqlite
        | StorageAdapter::Postgres
        | StorageAdapter::HybridIndraPg
        | StorageAdapter::SurrealMem
        | StorageAdapter::SurrealRocksdb => BackendTtlCapability::Deferred,
    }
}

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    if mode == RunMode::Benchmark {
        return Ok(());
    }
    match step {
        ScenarioStep::EnsureTtlForAll => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            valence
                .ensure_ttl_for_all()
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        ScenarioStep::EnsureTtlForTable => {
            let valence = session.ensure_valence().map_err(|e| e.to_string())?;
            valence
                .ensure_ttl_for_table(CATALOG_TTL_PROBE_TABLE)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        ScenarioStep::TtlNativeOrLingerContract { id } => ttl_native_or_linger(session, id).await,
        ScenarioStep::TtlCreateOnlyNoRefresh { id } => {
            ttl_create_only_no_refresh(session, id).await
        }
        ScenarioStep::TtlNonNativeWarnOnce => ttl_non_native_warn_once(session).await,
        ScenarioStep::TtlDeferredSweepDelete { id } => ttl_deferred_sweep_delete(session, id).await,
        ScenarioStep::IterScanComplete => iter_scan_complete(session).await,
        _ => Err("ttl step mismatch".into()),
    }
}

async fn ttl_deferred_sweep_delete(session: &mut BootstrapSession, id: &str) -> Result<(), String> {
    let storage = session.matrix().storage;
    let expected = expected_ttl_capability(storage);
    if !matches!(expected, BackendTtlCapability::Deferred) {
        return Err(format!(
            "ttl-deferred-sweep-delete not applicable to {} ({expected:?})",
            storage.slug()
        ));
    }
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    valence
        .ensure_ttl_for_table(CATALOG_TTL_PROBE_TABLE)
        .await
        .map_err(|e| e.to_string())?;
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let _ = backend.delete_record(CATALOG_TTL_PROBE_TABLE, id).await;
    let created = backend
        .create_record(
            CATALOG_TTL_PROBE_TABLE,
            serde_json::json!({"id": id, "n": 1}),
        )
        .await
        .map_err(|e| e.to_string())?;
    if created.get(EXPIRE_AT_FIELD).is_none() {
        return Err(format!("deferred create missing {EXPIRE_AT_FIELD}"));
    }
    backend
        .merge_record(
            CATALOG_TTL_PROBE_TABLE,
            id,
            serde_json::json!({ EXPIRE_AT_FIELD: "2020-01-01T00:00:00+00:00" }),
        )
        .await
        .map_err(|e| e.to_string())?;
    // Storage-level delete after expiry stamp (platform budgeted sweeper covered in valence-platform).
    backend
        .delete_record(CATALOG_TTL_PROBE_TABLE, id)
        .await
        .map_err(|e| e.to_string())?;
    let fetched = backend
        .get_record(CATALOG_TTL_PROBE_TABLE, id)
        .await
        .map_err(|e| e.to_string())?;
    if fetched.is_some() {
        return Err("deferred expired row must be gone after sweep-delete".into());
    }
    Ok(())
}

async fn iter_scan_complete(session: &mut BootstrapSession) -> Result<(), String> {
    use crate::fixtures::CATALOG_ITER_PROBE_TABLE;
    use valence_core::compiled_query::CompiledQuery;

    const N: usize = 1001;
    const PAGE: usize = 1000;

    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let _ = backend
        .ensure_schemaless_table(CATALOG_ITER_PROBE_TABLE)
        .await;
    for i in 0..N {
        let id = format!("r{i:04}");
        let _ = backend.delete_record(CATALOG_ITER_PROBE_TABLE, &id).await;
        backend
            .create_record(
                CATALOG_ITER_PROBE_TABLE,
                serde_json::json!({"id": id, "n": i}),
            )
            .await
            .map_err(|e| e.to_string())?;
    }

    let mut seen = Vec::new();
    let mut offset = 0usize;
    let engine = backend.engine_id();
    let surreal = engine.contains("surreal");
    loop {
        // Surreal uses START, not SQL OFFSET (see QueryCore surreal emit).
        let window = if surreal {
            format!("LIMIT {PAGE} START {offset}")
        } else {
            format!("LIMIT {PAGE} OFFSET {offset}")
        };
        let q = if engine.contains("postgres")
            || engine.contains("sqlite")
            || engine.contains("hybrid")
        {
            CompiledQuery::new(
                format!("SELECT id FROM {CATALOG_ITER_PROBE_TABLE} ORDER BY id ASC {window}"),
                vec![],
            )
        } else {
            CompiledQuery::new(
                format!("SELECT VALUE id FROM {CATALOG_ITER_PROBE_TABLE} ORDER BY id ASC {window}"),
                vec![],
            )
        };
        let rows = backend
            .execute_compiled_query(&q)
            .await
            .map_err(|e| e.to_string())?;
        if rows.is_empty() {
            break;
        }
        for r in &rows {
            let bare = r
                .as_str()
                .map(str::to_string)
                .or_else(|| r.get("id").and_then(|v| v.as_str()).map(str::to_string))
                .unwrap_or_else(|| r.to_string());
            let bare = bare
                .rsplit(':')
                .next()
                .unwrap_or(&bare)
                .trim()
                .trim_matches('"')
                .to_string();
            if !bare.is_empty() {
                seen.push(bare);
            }
        }
        if rows.len() < PAGE {
            break;
        }
        offset = offset.saturating_add(PAGE);
        if offset > N + PAGE {
            return Err("iter-scan-complete did not terminate".into());
        }
    }
    let mut uniq = seen.clone();
    uniq.sort();
    uniq.dedup();
    if uniq.len() != N {
        return Err(format!(
            "iter-scan-complete expected {N} unique ids, got {} (raw {})",
            uniq.len(),
            seen.len()
        ));
    }
    Ok(())
}

async fn ttl_native_or_linger(session: &mut BootstrapSession, id: &str) -> Result<(), String> {
    let storage = session.matrix().storage;
    let expected = expected_ttl_capability(storage);
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let backend = valence.active_backend().map_err(|e| e.to_string())?;
    let actual = backend.ttl_capability();
    if actual != expected {
        return Err(format!(
            "ttl capability mismatch for {}: expected {expected:?}, got {actual:?}",
            storage.slug()
        ));
    }

    let _ = backend.delete_record(CATALOG_TTL_PROBE_TABLE, id).await;
    let created = backend
        .create_record(
            CATALOG_TTL_PROBE_TABLE,
            serde_json::json!({"id": id, "n": 1}),
        )
        .await
        .map_err(|e| e.to_string())?;

    match expected {
        BackendTtlCapability::SupportedNative => match storage {
            StorageAdapter::Redis => {
                let fetched = backend
                    .get_record(CATALOG_TTL_PROBE_TABLE, id)
                    .await
                    .map_err(|e| e.to_string())?;
                if fetched.is_none() {
                    return Err("redis row missing immediately after create".into());
                }
                poll_until_absent(backend.as_ref(), id, Duration::from_secs(8)).await?;
            }
            StorageAdapter::MongoDb => {
                if created.get(EXPIRE_AT_FIELD).is_none() {
                    return Err(format!("mongo create missing {EXPIRE_AT_FIELD}"));
                }
                assert_mongo_ttl_index().await?;
            }
            other => {
                return Err(format!(
                    "ttl-native-expire not applicable to {}",
                    other.slug()
                ));
            }
        },
        BackendTtlCapability::Deferred => {
            if created.get(EXPIRE_AT_FIELD).is_none() {
                return Err(format!("deferred create missing {EXPIRE_AT_FIELD}"));
            }
            tokio::time::sleep(Duration::from_millis(
                CATALOG_TTL_PROBE_SECONDS * 1000 + 500,
            ))
            .await;
            let fetched = backend
                .get_record(CATALOG_TTL_PROBE_TABLE, id)
                .await
                .map_err(|e| e.to_string())?;
            if fetched.is_none() {
                return Err(
                    "deferred row deleted by engine after TTL — expected linger without sweeper"
                        .into(),
                );
            }
        }
        BackendTtlCapability::Unsupported => {
            if created.get(EXPIRE_AT_FIELD).is_some() {
                return Err("unsupported backend must not stamp expire field".into());
            }
            tokio::time::sleep(Duration::from_millis(
                CATALOG_TTL_PROBE_SECONDS * 1000 + 500,
            ))
            .await;
            let fetched = backend
                .get_record(CATALOG_TTL_PROBE_TABLE, id)
                .await
                .map_err(|e| e.to_string())?;
            if fetched.is_none() {
                return Err("unsupported row unexpectedly missing after wait".into());
            }
        }
    }
    Ok(())
}

async fn ttl_create_only_no_refresh(
    session: &mut BootstrapSession,
    id: &str,
) -> Result<(), String> {
    let storage = session.matrix().storage;
    let expected = expected_ttl_capability(storage);
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    let backend = valence.active_backend().map_err(|e| e.to_string())?;

    let _ = backend.delete_record(CATALOG_TTL_PROBE_TABLE, id).await;
    let created = backend
        .create_record(
            CATALOG_TTL_PROBE_TABLE,
            serde_json::json!({"id": id, "n": 1}),
        )
        .await
        .map_err(|e| e.to_string())?;

    if matches!(expected, BackendTtlCapability::Unsupported) {
        return Err("ttl-create-only-no-refresh not applicable to Unsupported".into());
    }

    if matches!(storage, StorageAdapter::Redis) {
        tokio::time::sleep(Duration::from_millis(400)).await;
        backend
            .update_record(
                CATALOG_TTL_PROBE_TABLE,
                id,
                serde_json::json!({"id": id, "n": 2}),
            )
            .await
            .map_err(|e| e.to_string())?;
        return poll_until_absent(backend.as_ref(), id, Duration::from_secs(8)).await;
    }

    // Deferred + Mongo: stamp must survive merge/update.
    let first = created
        .get(EXPIRE_AT_FIELD)
        .cloned()
        .ok_or_else(|| format!("missing {EXPIRE_AT_FIELD} after create"))?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    if backend.capabilities().supports_merge {
        let merged = backend
            .merge_record(CATALOG_TTL_PROBE_TABLE, id, serde_json::json!({"n": 2}))
            .await
            .map_err(|e| e.to_string())?;
        if merged.get(EXPIRE_AT_FIELD) != Some(&first) {
            return Err("merge refreshed create-only expire stamp".into());
        }
    } else {
        let updated = backend
            .update_record(
                CATALOG_TTL_PROBE_TABLE,
                id,
                serde_json::json!({"id": id, "n": 2, EXPIRE_AT_FIELD: first}),
            )
            .await
            .map_err(|e| e.to_string())?;
        if updated.get(EXPIRE_AT_FIELD) != Some(&first) {
            return Err("update changed create-only expire stamp".into());
        }
    }
    Ok(())
}

async fn ttl_non_native_warn_once(session: &mut BootstrapSession) -> Result<(), String> {
    let storage = session.matrix().storage;
    let expected = expected_ttl_capability(storage);
    if matches!(expected, BackendTtlCapability::SupportedNative) {
        return Err("ttl-non-native-warn not applicable to native adapters".into());
    }
    reset_ttl_warn_state_for_tests();
    let before = ttl_warn_emit_count_for_tests();
    let valence = session.ensure_valence().map_err(|e| e.to_string())?;
    valence
        .ensure_ttl_for_table(CATALOG_TTL_PROBE_TABLE)
        .await
        .map_err(|e| e.to_string())?;
    valence
        .ensure_ttl_for_table(CATALOG_TTL_PROBE_TABLE)
        .await
        .map_err(|e| e.to_string())?;
    let after = ttl_warn_emit_count_for_tests();
    if after <= before {
        return Err(format!(
            "expected non-native TTL warn on {} ({expected:?})",
            storage.slug()
        ));
    }
    if after - before != 1 {
        return Err(format!("expected warn-once, got {} emits", after - before));
    }
    Ok(())
}

async fn poll_until_absent(
    backend: &dyn valence_core::DatabaseBackend,
    id: &str,
    budget: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        let fetched = backend
            .get_record(CATALOG_TTL_PROBE_TABLE, id)
            .await
            .map_err(|e| e.to_string())?;
        if fetched.is_none() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(format!(
                "redis/native row {id} still present after {budget:?}"
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[cfg(feature = "mongodb")]
async fn assert_mongo_ttl_index() -> Result<(), String> {
    use futures::TryStreamExt;
    use mongodb::bson::Document;

    let uri = std::env::var(valence_backend_mongodb::TEST_URI_ENV)
        .or_else(|_| std::env::var(valence_backend_mongodb::URI_ENV))
        .map_err(|_| "mongodb URI unset for TTL index assert".to_string())?;
    let db_name =
        std::env::var(valence_backend_mongodb::DATABASE_ENV).unwrap_or_else(|_| "valence".into());
    let client = mongodb::Client::with_uri_str(&uri)
        .await
        .map_err(|e| e.to_string())?;
    let coll = client
        .database(&db_name)
        .collection::<Document>(CATALOG_TTL_PROBE_TABLE);
    let mut cursor = coll.list_indexes().await.map_err(|e| e.to_string())?;
    let mut found = false;
    while let Some(model) = cursor.try_next().await.map_err(|e| e.to_string())? {
        if model.options.as_ref().and_then(|o| o.name.as_deref()) == Some("valence_ttl_expire_at") {
            found = true;
            break;
        }
    }
    if !found {
        return Err("mongo TTL index valence_ttl_expire_at missing".into());
    }
    Ok(())
}

#[cfg(not(feature = "mongodb"))]
async fn assert_mongo_ttl_index() -> Result<(), String> {
    Err("mongodb feature disabled".into())
}
