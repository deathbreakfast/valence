//! Low-cardinality TTL ensure counters (no table / user labels).

use valence_telemetry::try_record_counter;

pub(crate) fn record_ensure(capability: &str, engine_id: &str) {
    try_record_counter(
        "valence_ttl_ensure_total",
        &[("capability", capability), ("engine_id", engine_id)],
        1,
    );
}

pub(crate) fn record_non_native_warn(capability: &str, engine_id: &str) {
    try_record_counter(
        "valence_ttl_non_native_warn_total",
        &[("capability", capability), ("engine_id", engine_id)],
        1,
    );
}

pub(crate) fn record_ensure_all(ttl_schema_count: usize) {
    let _ = ttl_schema_count;
    try_record_counter("valence_ttl_ensure_all_total", &[], 1);
}
