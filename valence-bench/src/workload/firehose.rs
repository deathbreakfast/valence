//! Sustained concurrent write firehose.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Result};
use valence_core::DatabaseBackend;

use crate::stats::MetricStats;

/// Result of a timed write firehose.
#[derive(Debug, Clone, Copy)]
pub struct FirehoseResult {
    pub achieved_write_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: usize,
    pub error_rate: f64,
    pub duration_secs: f64,
}

/// Result of a timed read firehose.
#[derive(Debug, Clone, Copy)]
pub struct ReadFirehoseResult {
    pub achieved_read_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: usize,
    pub error_rate: f64,
    pub duration_secs: f64,
    pub op_ms: MetricStats,
}

/// Run concurrent `create_record` loops for `duration_secs`.
pub async fn run_write_firehose(
    backend: Arc<dyn DatabaseBackend>,
    table: &str,
    duration_secs: u64,
    concurrency: usize,
) -> Result<FirehoseResult> {
    backend.ensure_schemaless_table(table).await?;
    let ok = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let seq = Arc::new(AtomicU64::new(0));
    // Per-run nonce so adapters sharing a physical store (hybrid ⇄ postgres primary) do
    // not collide on `id` and inflate the error rate with duplicate-key failures.
    let nonce = Instant::now().elapsed().as_nanos();
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(nonce, |d| d.as_nanos())
    );
    let deadline = Instant::now() + Duration::from_secs(duration_secs);

    let mut handles = Vec::with_capacity(concurrency);
    for _ in 0..concurrency {
        let backend = Arc::clone(&backend);
        let ok = Arc::clone(&ok);
        let errors = Arc::clone(&errors);
        let seq = Arc::clone(&seq);
        let table = table.to_string();
        let nonce = nonce.clone();
        handles.push(tokio::spawn(async move {
            while Instant::now() < deadline {
                let n = seq.fetch_add(1, Ordering::Relaxed);
                let id = format!("fh-{nonce}-{n}");
                match backend
                    .create_record(&table, serde_json::json!({"id": id, "n": n}))
                    .await
                {
                    Ok(_) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let elapsed = duration_secs as f64;
    let total = ok.load(Ordering::Relaxed);
    let error_count = errors.load(Ordering::Relaxed);
    let attempts = total + error_count as u64;
    let error_rate = if attempts == 0 {
        0.0
    } else {
        error_count as f64 / attempts as f64
    };

    Ok(FirehoseResult {
        achieved_write_ops_per_sec: total as f64 / elapsed.max(f64::EPSILON),
        total_ops: total,
        error_count,
        error_rate,
        duration_secs: elapsed,
    })
}

/// Run bounded concurrent `get_record` loops against a small set of hot records.
///
/// Latency is sampled every 64 reads so collecting p95 does not serialize the
/// throughput path or retain one sample per operation.
pub async fn run_read_firehose(
    backend: Arc<dyn DatabaseBackend>,
    table: &str,
    duration_secs: u64,
    concurrency: usize,
) -> Result<ReadFirehoseResult> {
    ensure!(duration_secs > 0, "read firehose duration must be positive");
    ensure!(
        concurrency > 0,
        "read firehose concurrency must be positive"
    );

    const HOT_RECORDS: usize = 64;
    const LATENCY_SAMPLE_EVERY: u64 = 64;

    backend.ensure_schemaless_table(table).await?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let ids = (0..HOT_RECORDS)
        .map(|index| format!("read-fh-{}-{nonce}-{index}", std::process::id()))
        .collect::<Vec<_>>();
    for (index, id) in ids.iter().enumerate() {
        backend
            .create_record(table, serde_json::json!({"id": id, "n": index}))
            .await?;
    }

    let ids = Arc::new(ids);
    let ok = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicUsize::new(0));
    let seq = Arc::new(AtomicU64::new(0));
    let deadline = Instant::now() + Duration::from_secs(duration_secs);
    let mut handles = Vec::with_capacity(concurrency);

    for _ in 0..concurrency {
        let backend = Arc::clone(&backend);
        let ids = Arc::clone(&ids);
        let ok = Arc::clone(&ok);
        let errors = Arc::clone(&errors);
        let seq = Arc::clone(&seq);
        let table = table.to_string();
        handles.push(tokio::spawn(async move {
            let mut latency_samples = Vec::new();
            while Instant::now() < deadline {
                let operation = seq.fetch_add(1, Ordering::Relaxed);
                let id = &ids[operation as usize % ids.len()];
                let sampled = operation.is_multiple_of(LATENCY_SAMPLE_EVERY);
                let started = sampled.then(Instant::now);
                match backend.get_record(&table, id).await {
                    Ok(Some(_)) => {
                        ok.fetch_add(1, Ordering::Relaxed);
                        if let Some(started) = started {
                            latency_samples.push(started.elapsed().as_secs_f64() * 1000.0);
                        }
                    }
                    Ok(None) | Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            latency_samples
        }));
    }

    let mut latency_samples = Vec::new();
    for handle in handles {
        latency_samples.extend(handle.await?);
    }

    let elapsed = duration_secs as f64;
    let total = ok.load(Ordering::Relaxed);
    let error_count = errors.load(Ordering::Relaxed);
    let attempts = total + error_count as u64;
    let error_rate = if attempts == 0 {
        0.0
    } else {
        error_count as f64 / attempts as f64
    };

    Ok(ReadFirehoseResult {
        achieved_read_ops_per_sec: total as f64 / elapsed,
        total_ops: total,
        error_count,
        error_rate,
        duration_secs: elapsed,
        op_ms: MetricStats::summarize(latency_samples),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use valence_backend_mem::InMemoryBackend;

    #[tokio::test]
    async fn read_firehose_reports_throughput_and_latency_happy() {
        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        let result = run_read_firehose(backend, "read_firehose_test", 1, 2)
            .await
            .expect("read firehose");

        assert!(result.achieved_read_ops_per_sec > 0.0);
        assert!(result.total_ops > 0);
        assert_eq!(result.error_count, 0);
        assert_eq!(result.error_rate, 0.0);
        assert!(result.op_ms.count > 0);
        assert!(result.op_ms.p95 >= 0.0);
    }

    #[tokio::test]
    async fn read_firehose_rejects_zero_bounds_sad() {
        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        let error = run_read_firehose(backend, "read_firehose_test", 0, 0)
            .await
            .expect_err("zero duration and concurrency must be rejected");

        assert!(error.to_string().contains("duration must be positive"));
    }
}
