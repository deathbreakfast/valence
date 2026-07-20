//! Write-path span timing for [`super::InstrumentedBackend`].

use std::time::Instant;

use crate::error::Result;
use crate::record_id::RecordId;

use super::InstrumentedBackend;
use crate::instrumentation::labels::{EdgeWriteOp, WriteOp};
use crate::instrumentation::metrics;

impl InstrumentedBackend {
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let label = self.telemetry_label();
        metrics::record_write(table, label, WriteOp::Create);
        let start = Instant::now();
        match self.inner.create_record(table, content).await {
            Ok(v) => {
                self.record_io_timing(
                    "create_record",
                    table,
                    WriteOp::Create.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    None,
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("create_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let label = self.telemetry_label();
        metrics::record_write(table, label, WriteOp::Update);
        let start = Instant::now();
        match self.inner.update_record(table, id, content).await {
            Ok(v) => {
                self.record_io_timing(
                    "update_record",
                    table,
                    WriteOp::Update.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    Some(id),
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("update_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let label = self.telemetry_label();
        metrics::record_write(table, label, WriteOp::Merge);
        let start = Instant::now();
        match self.inner.merge_record(table, id, patch).await {
            Ok(v) => {
                self.record_io_timing(
                    "merge_record",
                    table,
                    WriteOp::Merge.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    Some(id),
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("merge_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let label = self.telemetry_label();
        metrics::record_write(table, label, WriteOp::Upsert);
        let start = Instant::now();
        match self.inner.upsert_record(table, id, content).await {
            Ok(v) => {
                self.record_io_timing(
                    "upsert_record",
                    table,
                    WriteOp::Upsert.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    Some(id),
                );
                Ok(v)
            }
            Err(e) => {
                self.on_err("upsert_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_delete_record(&self, table: &str, id: &str) -> Result<()> {
        let label = self.telemetry_label();
        metrics::record_write(table, label, WriteOp::Delete);
        let start = Instant::now();
        match self.inner.delete_record(table, id).await {
            Ok(()) => {
                self.record_io_timing(
                    "delete_record",
                    table,
                    WriteOp::Delete.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    Some(id),
                );
                Ok(())
            }
            Err(e) => {
                self.on_err("delete_record", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_relate_edge(
        &self,
        from: &RecordId,
        edge_table: &str,
        to: &RecordId,
    ) -> Result<()> {
        let label = self.telemetry_label();
        metrics::record_edge_write(edge_table, label, EdgeWriteOp::Relate);
        let start = Instant::now();
        match self.inner.relate_edge(from, edge_table, to).await {
            Ok(()) => {
                self.record_io_timing(
                    "relate_edge",
                    edge_table,
                    EdgeWriteOp::Relate.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    None,
                );
                Ok(())
            }
            Err(e) => {
                self.on_err("relate_edge", &e);
                Err(e)
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(super) async fn measured_unrelate_edge(
        &self,
        from: &RecordId,
        edge_table: &str,
        to: &RecordId,
    ) -> Result<()> {
        let label = self.telemetry_label();
        metrics::record_edge_write(edge_table, label, EdgeWriteOp::Unrelate);
        let start = Instant::now();
        match self.inner.unrelate_edge(from, edge_table, to).await {
            Ok(()) => {
                self.record_io_timing(
                    "unrelate_edge",
                    edge_table,
                    EdgeWriteOp::Unrelate.as_str(),
                    start.elapsed().as_secs_f64() * 1000.0,
                    None,
                );
                Ok(())
            }
            Err(e) => {
                self.on_err("unrelate_edge", &e);
                Err(e)
            }
        }
    }
}
