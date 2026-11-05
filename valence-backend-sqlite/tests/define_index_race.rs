//! `define_index` racing the first write to a table with no registry layout must not
//! leave the table without the write's columns.
#![allow(clippy::expect_used)]

use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;

const TABLE: &str = "valence_data_ownership";
const ROUNDS: usize = 300;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn define_index_racing_first_write_keeps_owner_column() {
    let dir = tempfile::tempdir().expect("tempdir");
    for round in 0..ROUNDS {
        let path = dir.path().join(format!("race-{round}.db"));
        let backend = SqliteBackend::connect(path.to_str().expect("utf8 path"))
            .await
            .expect("connect sqlite");

        let (index, write) = tokio::join!(
            backend.define_index(TABLE, &["valence_model", "status"]),
            backend.upsert_record(
                TABLE,
                "o1",
                serde_json::json!({
                    "valence_model": "team",
                    "record_id": "t1",
                    "owner_id": "u1",
                    "owner_type": "user",
                    "status": "active",
                }),
            ),
        );
        index.unwrap_or_else(|e| panic!("round {round}: define_index failed: {e}"));
        write.unwrap_or_else(|e| panic!("round {round}: first write failed: {e}"));

        let row = backend
            .get_record(TABLE, "o1")
            .await
            .expect("read")
            .expect("row");
        assert_eq!(row["owner_id"], "u1", "round {round}");
    }
}
