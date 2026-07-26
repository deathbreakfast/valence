//! SurrealDB ↔ Valence error boundary (keeps `surrealdb` out of `valence-core`).

use valence_core::error::Error;

#[allow(clippy::needless_pass_by_value)] // map_err adapter; keeps surrealdb out of valence-core
pub fn db_err(e: surrealdb::Error) -> Error {
    let message = e.to_string();
    Error::database_with_source(message, e)
}
