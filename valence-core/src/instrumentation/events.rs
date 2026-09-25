//! Spectra event builders for Valence instrumentation.

use serde_json::{json, Value};

pub fn error_log_fields(
    source: &str,
    operation: &str,
    table: &str,
    telemetry_label: &str,
    message: &str,
) -> Value {
    json!({
        "source": source,
        "operation": operation,
        "table": table,
        "database_type": telemetry_label,
        "message": message,
    })
}

pub fn slow_op_fields(
    operation: &str,
    table: &str,
    op: &str,
    telemetry_label: &str,
    wall_ms: f64,
    record_id: &str,
) -> Value {
    json!({
        "operation": operation,
        "table": table,
        "op": op,
        "database_type": telemetry_label,
        "wall_ms": wall_ms,
        "record_id": record_id,
    })
}
