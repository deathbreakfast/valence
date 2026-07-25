//! Valence self-telemetry port.
//!
//! **Audience:** integrators installing metrics at boot and adapter authors
//! implementing custom sinks.

#![deny(missing_docs)]

mod console;
mod dispatch;
mod noop;
mod recording;

pub use console::ConsoleSink;
pub use dispatch::{
    install_telemetry_sink, json_value_to_fields, telemetry_sink, try_log_event,
    try_log_event_value, try_record_counter, try_record_gauge,
};
pub use noop::NoOpSink;
pub use recording::{RecordedCounter, RecordedEvent, RecordedGauge, RecordingSink};

/// Host-injectable telemetry sink for Valence ORM metrics and events.
///
/// Install with `ValenceBuilder::telemetry_sink` on the public crate / `valence-core`.
/// Instrumentation calls [`try_record_counter`], [`try_record_gauge`], and
/// [`try_log_event_value`] — never a product SDK from upstream code.
///
/// Reference impls: [`NoOpSink`] (default), [`ConsoleSink`] (`telemetry-console` on the public crate
/// feature). Custom sinks belong in host crates.
///
/// # Examples
///
/// ```
/// use valence_telemetry::{ConsoleSink, TelemetrySink};
///
/// let sink = ConsoleSink;
/// sink.record_counter("valence_queries", &[("backend", "mem")], 1);
/// sink.record_gauge("valence_active_connections", &[], 0.0);
/// sink.log_event("valence.boot", &[("phase", "ready")]);
/// ```
pub trait TelemetrySink: Send + Sync {
    /// Increment a counter metric by `delta` with optional label dimensions.
    fn record_counter(&self, name: &str, labels: &[(&str, &str)], delta: u64);

    /// Record a gauge sample with optional label dimensions.
    fn record_gauge(&self, name: &str, labels: &[(&str, &str)], value: f64);

    /// Emit a structured log event from flat field tuples.
    fn log_event(&self, schema: &str, fields: &[(&str, &str)]);

    /// Structured event with JSON payload (default flattens top-level object keys).
    fn log_event_value(&self, schema: &str, payload: &serde_json::Value) {
        let owned = json_value_to_fields(payload);
        let refs: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.log_event(schema, &refs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_sink_accepts_metrics() {
        let sink = NoOpSink;
        sink.record_counter("valence_queries", &[("backend", "mem")], 1);
        sink.record_gauge("valence_active_connections", &[], 0.0);
        sink.log_event("valence.record.created", &[("table", "user")]);
    }

    #[test]
    fn console_sink_accepts_metrics() {
        let sink = ConsoleSink;
        sink.record_counter("valence_queries", &[], 1);
    }
}
