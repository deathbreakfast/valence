//! Process-global [`TelemetrySink`] dispatch for instrumentation hooks.

use std::sync::{Arc, RwLock};

use serde_json::Value;

use crate::{NoOpSink, TelemetrySink};

static INSTALLED: RwLock<Option<Arc<dyn TelemetrySink>>> = RwLock::new(None);

/// Install the process-wide Valence telemetry sink (tests and host boot).
///
/// Recovers from a poisoned lock by taking the inner value (same policy as router resolve
/// callers that map poison to errors — telemetry prefers continuing over aborting the host).
pub fn install_telemetry_sink(sink: Arc<dyn TelemetrySink>) {
    *INSTALLED
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(sink);
}

/// Current sink, or [`NoOpSink`] when none is installed.
///
/// Recovers from a poisoned lock by taking the inner value.
pub fn telemetry_sink() -> Arc<dyn TelemetrySink> {
    INSTALLED
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
        .unwrap_or_else(|| Arc::new(NoOpSink))
}

/// Emit a counter when a sink is installed.
pub fn try_record_counter(name: &str, labels: &[(&str, &str)], delta: u64) {
    telemetry_sink().record_counter(name, labels, delta);
}

/// Emit a gauge when a sink is installed.
pub fn try_record_gauge(name: &str, labels: &[(&str, &str)], value: f64) {
    telemetry_sink().record_gauge(name, labels, value);
}

/// Emit a structured event from flat field tuples.
pub fn try_log_event(schema: &str, fields: &[(&str, &str)]) {
    telemetry_sink().log_event(schema, fields);
}

/// Emit a structured event from a JSON payload (instrumentation default).
pub fn try_log_event_value(schema: &str, payload: &Value) {
    telemetry_sink().log_event_value(schema, payload);
}

/// Flatten a JSON object into string field pairs for sinks that only store tuples.
pub fn json_value_to_fields(value: &Value) -> Vec<(String, String)> {
    let Some(obj) = value.as_object() else {
        return vec![("payload".into(), value.to_string())];
    };
    obj.iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect()
}
