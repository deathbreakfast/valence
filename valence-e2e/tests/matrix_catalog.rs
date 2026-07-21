//! Matrix-driven catalog tests for Valence e2e.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
#[cfg(feature = "surreal-inventory")]
mod support;

use valence_testkit::{
    catalog_for_storage, embedded_catalog, run_catalog_entry, MatrixSpec, StorageAdapter, Topology,
};

fn run_storage_catalog(storage: StorageAdapter) {
    for entry in catalog_for_storage(storage) {
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        rt.block_on(run_catalog_entry(entry, storage));
    }
}

#[test]
fn matrix_mem_embedded_catalog() {
    run_storage_catalog(StorageAdapter::Mem);
}

#[test]
#[cfg(feature = "sqlite")]
fn matrix_sqlite_catalog() {
    run_storage_catalog(StorageAdapter::Sqlite);
}

#[test]
#[cfg(feature = "mongodb")]
fn matrix_mongodb_catalog() {
    run_storage_catalog(StorageAdapter::MongoDb);
}

#[test]
#[cfg(feature = "indradb")]
fn matrix_indradb_catalog() {
    run_storage_catalog(StorageAdapter::IndraDb);
}

#[test]
#[cfg(feature = "redis")]
fn matrix_redis_catalog() {
    run_storage_catalog(StorageAdapter::Redis);
}

#[test]
#[cfg(feature = "surreal-mem")]
fn matrix_surreal_mem_catalog() {
    run_storage_catalog(StorageAdapter::SurrealMem);
}

#[test]
#[cfg(feature = "surreal-rocksdb")]
fn matrix_surreal_rocksdb_catalog() {
    use valence_testkit::extended_store_available;
    if !extended_store_available(StorageAdapter::SurrealRocksdb) {
        eprintln!("VALENCE_BENCH_ROCKSDB not set — skipping");
        return;
    }
    run_storage_catalog(StorageAdapter::SurrealRocksdb);
}

#[test]
#[cfg(feature = "postgres")]
fn matrix_postgres_catalog() {
    // Entries skip individually with a reason when DATABASE_URL is not configured.
    run_storage_catalog(StorageAdapter::Postgres);
}

#[test]
#[cfg(feature = "hybrid")]
fn matrix_hybrid_catalog() {
    // Entries skip individually with a reason when DATABASE_URL is not configured.
    run_storage_catalog(StorageAdapter::HybridIndraPg);
}

#[test]
#[cfg(feature = "acme-stub")]
fn matrix_acme_stub_catalog() {
    run_storage_catalog(StorageAdapter::AcmeStub);
}

#[test]
fn catalog_has_minimum_scenarios() {
    assert!(embedded_catalog().len() >= 12);
}

#[test]
#[ignore = "remote topology owned by host wiring"]
fn matrix_remote_stub_skipped() {
    let matrix = MatrixSpec {
        storage: StorageAdapter::Mem,
        telemetry: valence_testkit::TelemetryAdapter::Off,
        topology: Topology::RemoteStub,
    };
    let mut session = valence_testkit::BootstrapSession::new(matrix);
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    assert!(rt.block_on(session.spawn()).is_err());
}
