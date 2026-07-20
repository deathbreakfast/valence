//! Wall-time helpers for mutation and query instrumentation.

use std::time::Instant;

use super::metrics;

/// Timer for mutation wall time (L1).
pub struct MutationTimer {
    operation: &'static str,
    start: Instant,
}

impl MutationTimer {
    #[must_use]
    pub fn start(operation: &'static str) -> Self {
        Self {
            operation,
            start: Instant::now(),
        }
    }

    pub fn finish(self) {
        let ms = self.start.elapsed().as_secs_f64() * 1000.0;
        metrics::record_mutation_wall_ms(self.operation, ms);
    }
}

pub struct QueryTimer {
    start: Instant,
}

impl QueryTimer {
    #[must_use]
    pub fn start(_primary_table: impl Into<String>, _query_target: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "telemetry duration is intentionally bounded by i64 milliseconds"
    )]
    pub fn elapsed_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }
}
