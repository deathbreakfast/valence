//! Embedded Surreal router bootstrap — register logical names on a [`DatabaseRouter`].

use std::collections::BTreeSet;
use std::sync::Arc;

use valence_core::backend::DatabaseBackend;
use valence_core::error::Result;
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;

use crate::embedded::{SDb, SurrealEmbeddedBackend, ENGINE_ID};

#[cfg(feature = "inventory")]
use crate::inventory::collect_distinct_embedded_surreal_logical_names;

/// Options for embedded logical name registration.
#[derive(Clone, Copy, Debug, Default)]
pub struct RegisterEmbeddedLogicalNamesOptions {
    /// When set, also register each logical name under this alternate engine id (host migration shim).
    pub register_alias_engine_id: Option<&'static str>,
}

fn wrap_backend(db: SDb) -> Arc<dyn DatabaseBackend> {
    let backend: Arc<dyn DatabaseBackend> = Arc::new(SurrealEmbeddedBackend::new(db));
    #[cfg(feature = "instrumentation")]
    {
        valence_core::wrap_backend(backend)
    }
    #[cfg(not(feature = "instrumentation"))]
    {
        backend
    }
}

fn register_backend_for_logical(
    router: &mut DatabaseRouter,
    logical_name: &str,
    backend: Arc<dyn DatabaseBackend>,
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    let key = router_key(logical_name, ENGINE_ID);
    router.register(key, Arc::clone(&backend));
    if let Some(alias) = options.register_alias_engine_id {
        let alias_key = router_key(logical_name, alias);
        router.register(alias_key, backend);
    }
}

/// Register one embedded Surreal handle under each logical name.
pub fn register_embedded_logical_names(
    router: &mut DatabaseRouter,
    db: SDb,
    logical_names: &[&str],
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    let backend = wrap_backend(db);
    for &name in logical_names {
        register_backend_for_logical(router, name, Arc::clone(&backend), options);
    }
}

/// Register `db` under every distinct logical name in `groups` (deduplicated).
pub fn register_embedded_logical_names_slices(
    router: &mut DatabaseRouter,
    db: SDb,
    groups: &[&[&str]],
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    let mut seen = BTreeSet::<&str>::new();
    for group in groups {
        for &name in *group {
            seen.insert(name);
        }
    }
    let backend = wrap_backend(db);
    for name in seen {
        register_backend_for_logical(router, name, Arc::clone(&backend), options);
    }
}

/// Register a distinct embedded Surreal handle under a single logical name.
pub fn register_embedded_logical_handle(
    router: &mut DatabaseRouter,
    logical_name: &str,
    db: SDb,
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    register_embedded_logical_handles(router, &[(logical_name, db)], options);
}

/// Register distinct embedded Surreal handles — one backend per `(logical_name, db)` pair.
pub fn register_embedded_logical_handles(
    router: &mut DatabaseRouter,
    handles: &[(&str, SDb)],
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    let mut seen = BTreeSet::<&str>::new();
    for &(name, ref db) in handles {
        if !seen.insert(name) {
            continue;
        }
        let backend = wrap_backend(db.clone());
        register_backend_for_logical(router, name, backend, options);
    }
}

#[cfg(feature = "inventory")]
/// Register embedded logical names discovered from schema inventory.
pub fn register_embedded_logical_names_from_inventory(
    router: &mut DatabaseRouter,
    db: SDb,
    options: RegisterEmbeddedLogicalNamesOptions,
) {
    let names = collect_distinct_embedded_surreal_logical_names();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    register_embedded_logical_names_slices(router, db, &[refs.as_slice()], options);
}

/// Build a router with embedded Surreal registered for each logical name.
pub fn shared_router_with_embedded_logical_names(
    db: SDb,
    logical_names: &[&str],
    options: RegisterEmbeddedLogicalNamesOptions,
) -> Arc<DatabaseRouter> {
    let mut router = DatabaseRouter::new();
    register_embedded_logical_names(&mut router, db, logical_names, options);
    Arc::new(router)
}

/// Bootstrap an embedded router from explicit logical names.
///
/// # Examples
///
/// ```
/// # async fn example() -> valence_core::Result<()> {
/// use valence_backend_surreal::{
///     bootstrap_embedded_router, connect_embedded_at_path, EmbeddedEngine,
///     RegisterEmbeddedLogicalNamesOptions, ENGINE_ID,
/// };
/// use valence_core::router_key;
///
/// let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "demo", "demo").await?;
/// let router = bootstrap_embedded_router(
///     db,
///     &["default", "billing"],
///     RegisterEmbeddedLogicalNamesOptions::default(),
/// )?;
/// assert!(router.resolve(&router_key("billing", ENGINE_ID)).is_ok());
/// # Ok(())
/// # }
/// ```
///
/// # Errors
///
/// Currently infallible; `Result` reserved for future registration failures.
pub fn bootstrap_embedded_router(
    db: SDb,
    logical_names: &[&str],
    options: RegisterEmbeddedLogicalNamesOptions,
) -> Result<Arc<DatabaseRouter>> {
    Ok(shared_router_with_embedded_logical_names(
        db,
        logical_names,
        options,
    ))
}

#[cfg(feature = "inventory")]
/// Bootstrap an embedded router using schema inventory discovery.
///
/// # Errors
///
/// Currently infallible; `Result` reserved for future registration failures.
pub fn bootstrap_embedded_router_from_inventory(
    db: SDb,
    options: RegisterEmbeddedLogicalNamesOptions,
) -> Result<Arc<DatabaseRouter>> {
    let mut router = DatabaseRouter::new();
    register_embedded_logical_names_from_inventory(&mut router, db, options);
    Ok(Arc::new(router))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;
    use surrealdb::engine::local::Mem;

    async fn mem_db() -> SDb {
        let db = SDb::init();
        db.connect::<Mem>(()).await.unwrap();
        db.use_ns("test").use_db("test").await.unwrap();
        db
    }

    #[tokio::test]
    async fn register_multiple_logical_names_shares_backend() {
        let db = mem_db().await;
        let mut router = DatabaseRouter::new();
        register_embedded_logical_names(
            &mut router,
            db,
            &["default", "billing"],
            RegisterEmbeddedLogicalNamesOptions::default(),
        );
        assert_eq!(router.len().unwrap(), 2);
        let k1 = router_key("default", ENGINE_ID);
        let k2 = router_key("billing", ENGINE_ID);
        assert!(router.resolve(&k1).is_ok());
        assert!(router.resolve(&k2).is_ok());
    }

    #[tokio::test]
    async fn alias_engine_registers_second_key() {
        let db = mem_db().await;
        let mut router = DatabaseRouter::new();
        register_embedded_logical_names(
            &mut router,
            db,
            &["default"],
            RegisterEmbeddedLogicalNamesOptions {
                register_alias_engine_id: Some("legacy_surreal"),
            },
        );
        assert_eq!(router.len().unwrap(), 2);
    }
}
