//! Sustained concurrent write firehose.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use valence_core::DatabaseBackend;

/// Result of a timed write firehose.
#[derive(Debug, Clone, Copy)]
pub struct FirehoseResult {
    pub achieved_write_ops_per_sec: f64,
    pub total_ops: u64,
    pub error_count: usize,
    pub error_rate: f64,
    pub duration_secs: f64,
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
