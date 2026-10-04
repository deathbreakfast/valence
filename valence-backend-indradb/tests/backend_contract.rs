#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_indradb::IndradbBackend;
use valence_core::{DatabaseBackend, RecordId};
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn indradb_backend_passes_port_contract() {
    let backend = Arc::new(IndradbBackend::new());
    run_backend_contract(backend)
        .await
        .expect("backend contract");
}

#[tokio::test]
async fn edge_targets_keep_ids_of_rows_stored_elsewhere_happy_path() {
    let backend = IndradbBackend::new();
    let group = RecordId::new("permission_group", "super_user_group");
    let principal = RecordId::new("permission_user_principal", "user:d866ffa6");

    backend
        .relate_edge(&group, "permission_group_member_principal", &principal)
        .await
        .expect("relate");

    let targets = backend
        .get_edge_targets(&group, "permission_group_member_principal")
        .await
        .expect("targets");
    assert_eq!(targets, vec![principal]);
}

#[tokio::test]
async fn edge_only_endpoint_is_not_a_readable_row_sad() {
    let backend = IndradbBackend::new();
    let group = RecordId::new("permission_group", "g1");
    let principal = RecordId::new("permission_user_principal", "p1");

    backend
        .relate_edge(&group, "permission_group_member_principal", &principal)
        .await
        .expect("relate");

    let row = backend
        .get_record("permission_user_principal", "p1")
        .await
        .expect("get");
    assert!(
        row.is_none(),
        "edge stub must not surface as a record: {row:?}"
    );
}
