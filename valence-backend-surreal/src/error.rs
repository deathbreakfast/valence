//! SurrealDB ↔ Valence error boundary (keeps `surrealdb` out of `valence-core`).

use valence_core::error::Error;

#[allow(clippy::needless_pass_by_value)] // map_err adapter; value only Display'd
pub fn db_err(e: surrealdb::Error) -> Error {
    Error::Database(e.to_string())
}
