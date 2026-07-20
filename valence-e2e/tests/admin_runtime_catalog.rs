//! Admin runtime catalog — all available storage adapters.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence_testkit::{
    all_storage_adapters, run_admin_contract_for, run_admin_contract_mem,
    run_admin_contract_surreal_mem,
};

#[test]
fn admin_runtime_mem() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_admin_contract_mem())
        .expect("mem admin contract");
}

#[test]
#[cfg(feature = "surreal-mem")]
fn admin_runtime_surreal_mem() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_admin_contract_surreal_mem())
        .expect("surreal admin contract");
}

#[test]
fn admin_runtime_all_enabled_adapters() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    for storage in all_storage_adapters() {
        rt.block_on(run_admin_contract_for(storage))
            .unwrap_or_else(|e| panic!("admin contract for {}: {e}", storage.slug()));
    }
}
