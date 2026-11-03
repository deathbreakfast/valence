//! Spectra event builders for Valence instrumentation.

use serde_json::{json, Value};

// Event payloads name the table `table_name`: Spectra's schema macro emits its own `table`
// field, so a payload key `table` cannot be registered as a schema field.

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
        "table_name": table,
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
        "table_name": table,
        "op": op,
        "database_type": telemetry_label,
        "wall_ms": wall_ms,
        "record_id": record_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_log_uses_table_name_key() {
        let v = error_log_fields("database", "read", "user", "mem", "boom");
        assert_eq!(v["table_name"], "user");
        assert!(v.get("table").is_none());
    }

    #[test]
    fn slow_op_uses_table_name_key() {
        let v = slow_op_fields("query", "user", "read", "mem", 12.5, "u1");
        assert_eq!(v["table_name"], "user");
        assert_eq!(v["record_id"], "u1");
        assert!(v.get("table").is_none());
    }
}
