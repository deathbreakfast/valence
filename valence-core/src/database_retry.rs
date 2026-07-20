//! Retry database operations when the engine reports retryable transaction contention.

use crate::error::{Error, Result};
use crate::instrumentation::{self, timing::MutationTimer};
use std::future::Future;
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 12;
const BASE_DELAY_MS: u64 = 4;

/// Retry an async Valence operation when [`is_retryable_transaction_contention`](crate::error::Error::is_retryable_transaction_contention) applies.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn retry_on_database_tx_conflict<F, Fut, T>(
    operation: &'static str,
    mut f: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let timer = MutationTimer::start(operation);
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        match f().await {
            Ok(v) => {
                timer.finish();
                return Ok(v);
            }
            Err(e) => {
                if e.is_retryable_transaction_contention() && attempt < MAX_ATTEMPTS {
                    let pow = attempt.min(8);
                    let ms = (BASE_DELAY_MS.saturating_mul(1u64 << pow)).min(400);
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    continue;
                }
                if operation == "Model::__assert_unique_field_value" {
                    if let Error::Validation(ref msg) = e {
                        if let Some((table, field)) = parse_unique_violation(msg) {
                            instrumentation::record_unique_violation(table, field);
                            return Err(e);
                        }
                    }
                }
                if e.is_retryable_transaction_contention() {
                    instrumentation::record_retry_error(operation, "unknown", &e.to_string());
                }
                return Err(e);
            }
        }
    }
}

fn parse_unique_violation(msg: &str) -> Option<(&str, &str)> {
    // "Unique constraint violation on table.field"
    let rest = msg.strip_prefix("Unique constraint violation on ")?;
    let (table, field) = rest.split_once('.')?;
    Some((table, field))
}
