//! Read-path span timing for [`super::InstrumentedBackend`].

use std::time::Instant;

use crate::compiled_query::CompiledQuery;
use crate::error::Result;
use crate::record_id::RecordId;

use super::InstrumentedBackend;
use crate::instrumentation::labels::ReadOp;
use crate::instrumentation::metrics;

impl InstrumentedBackend {
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        let label = self.telemetry_label();
        metrics::record_read("_compiled", label, ReadOp::Query);
        let start = Instant::now();
        match self.inner.execute_compiled_query(compiled).await {
            Ok(v) => {
                let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
                self.record_io_timing(
                    "execute_compiled_query",
                    "_compiled",
                    ReadOp::Query.as_str(),
                    wall_ms,
                    None,
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("execute_compiled_query", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_get_record(
        &self,
        table: &str,
        id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let label = self.telemetry_label();
        metrics::record_read(table, label, ReadOp::Get);
        let start = Instant::now();
        match self.inner.get_record(table, id).await {
            Ok(v) => {
                self.record_io_timing(
                    "get_record",
                    table,
                    ReadOp::Get.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    Some(id),
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("get_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_get_edge_targets(
        &self,
        from: &RecordId,
        edge_table: &str,
    ) -> Result<Vec<RecordId>> {
        let label = self.telemetry_label();
        metrics::record_edge_read(edge_table, label);
        let start = Instant::now();
        match self.inner.get_edge_targets(from, edge_table).await {
            Ok(v) => {
                self.record_io_timing(
                    "get_edge_targets",
                    edge_table,
                    "edge_read",
                    start.elapsed().as_secs_f64() * 1000.0,
                    None,
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("get_edge_targets", &e);
                Err(e)
            }
        }
    }
}
