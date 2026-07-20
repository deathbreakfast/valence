//! Cross-backend hop contract — Cartesian pairs + nested chains.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence_testkit::{
    directed_pairs, hop_quads_representative, hop_triples_representative,
    run_cross_backend_hop_contract, run_hop_chain_contract, run_hop_pair_contract,
    run_hop_quad_contract, CrossBackendLayout,
};

#[test]
fn cross_backend_hops_mem_mem() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_cross_backend_hop_contract(CrossBackendLayout::MemMem))
        .expect("mem-mem hops");
}

#[test]
#[cfg(feature = "cross-backend-hops")]
fn cross_backend_hops_mem_sqlite_legacy() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_cross_backend_hop_contract(
        CrossBackendLayout::MemSqlite,
    ))
    .expect("mem-sqlite hops");
}

#[test]
fn cross_backend_hop_pairs_cartesian() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    for pair in directed_pairs() {
        rt.block_on(run_hop_pair_contract(pair, None))
            .unwrap_or_else(|e| panic!("hop pair {}: {e}", pair.slug()));
    }
}

#[test]
fn cross_backend_hop_triples_representative() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    for triple in hop_triples_representative() {
        rt.block_on(run_hop_chain_contract(triple, None))
            .unwrap_or_else(|e| panic!("hop triple {}: {e}", triple.slug()));
    }
}

#[test]
fn cross_backend_hop_quads_representative() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    for quad in hop_quads_representative() {
        rt.block_on(run_hop_quad_contract(quad, None))
            .unwrap_or_else(|e| panic!("hop quad {}: {e}", quad.slug()));
    }
}

#[test]
#[ignore = "requires DATABASE_URL and postgres feature"]
#[cfg(feature = "postgres")]
fn cross_backend_hops_postgres_sqlite() {
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(run_cross_backend_hop_contract(
        CrossBackendLayout::PostgresSqlite,
    ))
    .expect("postgres-sqlite hops");
}
