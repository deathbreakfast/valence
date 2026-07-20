//! Model runtime catalog — generated CRUD across storage adapters.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence_testkit::{
    all_storage_adapters, run_model_contract_acme_stub, run_model_contract_for,
    run_model_contract_mem, run_model_contract_surreal_mem, StorageAdapter,
};

#[test]
fn model_runtime_mem() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_mem())
        .expect("mem model contract");
}

#[test]
#[cfg(feature = "sqlite")]
fn model_runtime_sqlite() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_for(StorageAdapter::Sqlite))
        .expect("sqlite model contract");
}

#[test]
#[cfg(feature = "mongodb")]
fn model_runtime_mongodb() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_for(StorageAdapter::MongoDb))
        .expect("mongodb model contract");
}

#[test]
#[cfg(feature = "indradb")]
fn model_runtime_indradb() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_for(StorageAdapter::IndraDb))
        .expect("indradb model contract");
}

#[test]
#[cfg(feature = "redis")]
fn model_runtime_redis() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_for(StorageAdapter::Redis))
        .expect("redis model contract");
}

#[test]
#[ignore = "requires DATABASE_URL"]
#[cfg(feature = "postgres")]
fn model_runtime_postgres() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_for(StorageAdapter::Postgres))
        .expect("postgres model contract");
}

#[test]
#[cfg(feature = "surreal-mem")]
fn model_runtime_surreal_mem() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_surreal_mem())
        .expect("surreal model contract");
}

#[test]
#[cfg(feature = "acme-stub")]
#[ignore = "acme stub does not implement deletion DAG compiled queries"]
fn model_runtime_acme_stub() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_model_contract_acme_stub())
        .expect("acme model contract");
}

#[test]
fn model_runtime_all_enabled_adapters() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    for storage in all_storage_adapters() {
        if !storage.supports_model_runtime() {
            continue;
        }
        rt.block_on(run_model_contract_for(storage))
            .unwrap_or_else(|e| panic!("model contract for {}: {e}", storage.slug()));
    }
}
