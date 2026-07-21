//! Wiring / router / factory / endpoint steps.

use crate::bootstrap::BootstrapSession;
use crate::runner::RunMode;
use crate::scenario::ScenarioStep;

pub(super) async fn run(
    session: &mut BootstrapSession,
    step: &ScenarioStep,
    mode: RunMode,
) -> Result<(), String> {
    match step {
        ScenarioStep::BuildValence => {
            session.build_valence(None).map_err(|e| e.to_string())?;
        }
        ScenarioStep::AssertActiveBackend => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let valence = session
                .valence()
                .ok_or_else(|| "BuildValence must run first".to_string())?;
            valence.active_backend().map_err(|e| e.to_string())?;
        }
        ScenarioStep::AssertRouterResolve { key } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let router = session
                .router()
                .ok_or_else(|| "missing router".to_string())?;
            router.resolve(key).map_err(|e| e.to_string())?;
        }
        ScenarioStep::AssertRouterResolveFails { key } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let router = session
                .router()
                .ok_or_else(|| "missing router".to_string())?;
            if router.resolve(key).is_ok() {
                return Err(format!("expected router resolve to fail for key {key}"));
            }
        }
        ScenarioStep::AssertRouterLen { min } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let router = session
                .router()
                .ok_or_else(|| "missing router".to_string())?;
            let len = router.len().map_err(|e| e.to_string())?;
            if len < *min {
                return Err(format!("router len {len} < min {min}"));
            }
        }
        ScenarioStep::AssertRouterSharedBackend { key_a, key_b } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let router = session
                .router()
                .ok_or_else(|| "missing router".to_string())?;
            let a = router.resolve(key_a).map_err(|e| e.to_string())?;
            let b = router.resolve(key_b).map_err(|e| e.to_string())?;
            if !std::sync::Arc::ptr_eq(&a, &b) {
                return Err(format!(
                    "expected {key_a} and {key_b} to share one backend instance"
                ));
            }
        }
        ScenarioStep::CrudAcrossRouterKeys {
            create_key,
            read_key,
            table,
            id,
        } => {
            let router = session
                .router()
                .ok_or_else(|| "missing router".to_string())?;
            let writer = router.resolve(create_key).map_err(|e| e.to_string())?;
            let reader = router.resolve(read_key).map_err(|e| e.to_string())?;
            // Wire stores are shared across matrix rows; clear leftovers from prior runs.
            let _ = writer.delete_record(table, id).await;
            writer
                .create_record(table, serde_json::json!({"id": id, "name": "cross-key"}))
                .await
                .map_err(|e| e.to_string())?;
            let fetched = reader
                .get_record(table, id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| {
                    format!("record created via {create_key} not visible via {read_key}")
                })?;
            if mode == RunMode::Correctness
                && fetched.get("name").and_then(|v| v.as_str()) != Some("cross-key")
            {
                return Err(format!(
                    "record read via {read_key} lost content written via {create_key}"
                ));
            }
            // Delete via the *other* key and verify the write path is shared too.
            reader
                .delete_record(table, id)
                .await
                .map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness {
                let gone = writer
                    .get_record(table, id)
                    .await
                    .map_err(|e| e.to_string())?;
                if gone.is_some() {
                    return Err(format!(
                        "record deleted via {read_key} still visible via {create_key}"
                    ));
                }
            }
        }
        ScenarioStep::BuildValenceFromFactory { actor_json } => {
            let factory = session
                .factory()
                .ok_or_else(|| "missing factory".to_string())?;
            let valence = factory.build(actor_json).map_err(|e| e.to_string())?;
            if mode == RunMode::Correctness {
                valence.active_backend().map_err(|e| e.to_string())?;
            }
            session.valence = Some(valence);
        }
        ScenarioStep::SetEnv { key, value } => {
            let key_static: &'static str = match key.as_str() {
                "VALENCE_ENDPOINT_DEFAULT" => "VALENCE_ENDPOINT_DEFAULT",
                other => {
                    return Err(format!("unsupported env key for scenario: {other}"));
                }
            };
            session.set_env(key_static, value);
        }
        ScenarioStep::AssertEndpointResolve {
            logical,
            expect_url,
        } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let resolved = session
                .resolve_env_endpoint(logical)
                .map_err(|e| e.to_string())?;
            if resolved.as_deref() != Some(expect_url.as_str()) {
                return Err(format!(
                    "endpoint {logical}: expected {expect_url}, got {resolved:?}"
                ));
            }
        }
        ScenarioStep::AssertEndpointUnresolved { logical } => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let resolved = session
                .resolve_env_endpoint(logical)
                .map_err(|e| e.to_string())?;
            if resolved.is_some() {
                return Err(format!(
                    "endpoint {logical}: expected unresolved, got {resolved:?}"
                ));
            }
        }
        ScenarioStep::AssertBuilderEmptyFails => {
            if mode == RunMode::Benchmark {
                return Ok(());
            }
            let built = valence_core::runtime::Valence::builder().build();
            if built.is_ok() {
                return Err("empty builder should fail".into());
            }
        }
        other => {
            return Err(format!("wiring step mismatch: {other:?}"));
        }
    }
    Ok(())
}
