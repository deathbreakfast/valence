//! In-memory databases survive the pool replacing its connection, and each
//! in-memory backend gets its own database.
#![allow(clippy::expect_used)]

use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;

const TABLE: &str = "recycle_demo";

async fn seed(backend: &SqliteBackend, id: &str, name: &str) {
    backend
        .create_record(TABLE, serde_json::json!({"id": id, "name": name}))
        .await
        .expect("create row");
}

async fn name_of(backend: &SqliteBackend, id: &str) -> Option<serde_json::Value> {
    backend
        .get_record(TABLE, id)
        .await
        .expect("read row")
        .map(|row| row["name"].clone())
}

/// Close the pool's only connection so the next query opens a replacement,
/// the same path sqlx takes when a release ping fails or a connection ages out.
async fn replace_pool_connection(backend: &SqliteBackend) {
    let conn = backend.pool().acquire().await.expect("acquire");
    let open = backend.pool().size();
    conn.close().await.expect("close connection");
    assert_eq!(
        backend.pool().size(),
        open - 1,
        "pool connection was not closed"
    );
}

async fn assert_rows_survive_replacement(backend: &SqliteBackend) {
    seed(backend, "r1", "first").await;
    replace_pool_connection(backend).await;

    assert_eq!(name_of(backend, "r1").await, Some("first".into()));
    seed(backend, "r2", "second").await;
    assert_eq!(name_of(backend, "r2").await, Some("second".into()));
}

#[tokio::test]
async fn memory_rows_survive_pool_connection_replacement() {
    for url in [":memory:", "sqlite::memory:", "sqlite://:memory:"] {
        let backend = SqliteBackend::connect(url).await.expect("connect");
        assert_rows_survive_replacement(&backend).await;
    }
}

#[tokio::test]
async fn named_shared_memory_url_survives_pool_connection_replacement() {
    let url = format!(
        "file:valence-recycle-named-{}?mode=memory&cache=shared",
        std::process::id()
    );
    let backend = SqliteBackend::connect(&url).await.expect("connect");
    assert_rows_survive_replacement(&backend).await;
}

#[tokio::test]
async fn memory_backends_do_not_share_rows() {
    let a = SqliteBackend::connect_memory().await.expect("connect a");
    let b = SqliteBackend::connect_memory().await.expect("connect b");

    seed(&a, "r1", "from a").await;
    assert_eq!(name_of(&b, "r1").await, None);

    seed(&b, "r1", "from b").await;
    assert_eq!(name_of(&a, "r1").await, Some("from a".into()));
    assert_eq!(name_of(&b, "r1").await, Some("from b".into()));
}

#[tokio::test]
async fn file_rows_survive_pool_connection_replacement() {
    let path = std::env::temp_dir().join(format!(
        "valence-recycle-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let backend = SqliteBackend::connect(path.to_str().expect("utf8 path"))
        .await
        .expect("connect");
    assert_rows_survive_replacement(&backend).await;
    drop(backend);
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}
