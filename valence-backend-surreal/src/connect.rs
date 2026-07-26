//! Generic embedded Surreal connection helpers.

use std::path::Path;

use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;
use valence_core::error::{Error, Result};

#[cfg(feature = "embedded-rocksdb")]
use surrealdb::engine::local::RocksDb;

use crate::embedded::SDb;

/// Embedded Surreal engine selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedEngine {
    /// In-process memory engine.
    Mem,
    /// On-disk RocksDB engine.
    RocksDb,
}

impl EmbeddedEngine {
    /// Parse from env-style tokens (`mem`, `rocksdb`, `rocks`, …).
    pub fn parse_env(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "mem" | "memory" => Some(Self::Mem),
            "rocksdb" | "rocks" | "disk" => Some(Self::RocksDb),
            _ => None,
        }
    }
}

/// Remove a stale Surreal `LOCK` file when present.
pub fn remove_stale_lock(lock_path: impl AsRef<Path>) {
    let lock_path = lock_path.as_ref();
    if lock_path.exists() {
        let _ = std::fs::remove_file(lock_path);
    }
}

/// Open an embedded Surreal database at `path`, pin namespace/database, optionally clear stale lock.
///
/// # Errors
///
/// Returns [`Error::Database`] when Surreal fails to open or select ns/db, and
/// [`Error::Validation`] when RocksDB is requested without the `embedded-rocksdb` feature.
///
/// # Examples
///
/// ```
/// # async fn example() -> valence_core::Result<()> {
/// use valence_backend_surreal::{connect_embedded_at_path, EmbeddedEngine};
///
/// let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "demo", "demo").await?;
/// # let _ = db;
/// # Ok(())
/// # }
/// ```
pub async fn connect_embedded_at_path(
    engine: EmbeddedEngine,
    path: &str,
    ns: &str,
    db_name: &str,
) -> Result<SDb> {
    if matches!(engine, EmbeddedEngine::RocksDb) {
        if let Some(parent) = Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        remove_stale_lock(format!("{path}/LOCK"));
    }

    let db: Surreal<Db> = match engine {
        EmbeddedEngine::Mem => Surreal::new::<Mem>(())
            .await
            .map_err(|e| Error::database(format!("Failed to open embedded Mem store: {e}")))?,
        #[cfg(feature = "embedded-rocksdb")]
        EmbeddedEngine::RocksDb => Surreal::new::<RocksDb>(path).await.map_err(|e| {
            Error::database(format!("Failed to open embedded RocksDB at {path}: {e}"))
        })?,
        #[cfg(not(feature = "embedded-rocksdb"))]
        EmbeddedEngine::RocksDb => {
            return Err(Error::Validation(
                "embedded-rocksdb feature required for RocksDb engine".into(),
            ));
        }
    };

    db.use_ns(ns)
        .use_db(db_name)
        .await
        .map_err(|e| Error::database(format!("Failed to select ns/db {ns}/{db_name}: {e}")))?;
    Ok(db)
}

#[cfg(feature = "connect-env")]
/// Read embedded engine from `VALENCE_EMBEDDED_ENGINE` (default RocksDB).
pub fn embedded_engine_from_env() -> EmbeddedEngine {
    std::env::var("VALENCE_EMBEDDED_ENGINE")
        .ok()
        .and_then(|v| EmbeddedEngine::parse_env(&v))
        .unwrap_or(EmbeddedEngine::RocksDb)
}

#[cfg(feature = "connect-env")]
/// Read store path from `VALENCE_EMBEDDED_PATH` (default `surreal/data`).
pub fn embedded_path_from_env() -> String {
    std::env::var("VALENCE_EMBEDDED_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "surreal/data".to_string())
}

#[cfg(feature = "connect-env")]
/// Read namespace/database from `VALENCE_NS` / `VALENCE_DB` (default `prod`/`prod`).
pub fn namespace_from_env() -> String {
    std::env::var("VALENCE_NS")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "prod".to_string())
}

#[cfg(feature = "connect-env")]
/// Read database name from `VALENCE_DB` (default `prod`).
pub fn database_from_env() -> String {
    std::env::var("VALENCE_DB")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "prod".to_string())
}

#[cfg(feature = "connect-env")]
/// Open embedded Surreal using neutral env vars.
///
/// # Errors
///
/// Propagates failures from [`connect_embedded_at_path`].
pub async fn connect_embedded_from_env() -> Result<SDb> {
    let engine = embedded_engine_from_env();
    let path = embedded_path_from_env();
    let ns = namespace_from_env();
    let db = database_from_env();
    match engine {
        EmbeddedEngine::Mem => connect_embedded_at_path(engine, "", &ns, &db).await,
        EmbeddedEngine::RocksDb => connect_embedded_at_path(engine, &path, &ns, &db).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_engine_tokens() {
        assert_eq!(EmbeddedEngine::parse_env("mem"), Some(EmbeddedEngine::Mem));
        assert_eq!(
            EmbeddedEngine::parse_env("rocksdb"),
            Some(EmbeddedEngine::RocksDb)
        );
        assert_eq!(EmbeddedEngine::parse_env("unknown"), None);
    }

    #[tokio::test]
    async fn connect_mem_engine() {
        let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "test", "test")
            .await
            .expect("connect mem");
        let _ = db;
    }
}
