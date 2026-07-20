//! Temp schema file helper for codegen integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SCHEMA_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn write_temp_schema_file(contents: &str, name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let seq = TEMP_SCHEMA_SEQ.fetch_add(1, Ordering::Relaxed);
    let unique = format!(
        "valence_codegen_test_{}_{}_{}_{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        seq,
        std::thread::current().id()
    );
    dir.push(unique);
    fs::create_dir_all(&dir).expect("Failed to create temp dir");

    let path = dir.join(name);
    fs::write(&path, contents).expect("Failed to write schema file");
    path
}
