//! `QueryCore::sum_values` on the in-memory backend sums past one query window.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::data_use::DataUsePurpose;
use valence_core::query::QueryCore;
use valence_core::{DatabaseBackend, Valence};

const TABLE: &str = "mem_sum_line";

fn purpose() -> DataUsePurpose {
    DataUsePurpose::new(
        r"**Test:** Sums seeded rows so the suite can check totals against values computed in the test. CI and developers running the suite only.",
        file!(),
        line!(),
    )
}

#[tokio::test]
async fn sum_values_groups_every_row_past_the_query_cap() {
    let backend = Arc::new(InMemoryBackend::new());
    for i in 0..1500_i64 {
        let mut row = json!({"id": format!("r{i:04}"), "kind": format!("k{}", i % 2), "n": i});
        if i == 7 {
            // No value: skipped, not an error.
            row["n"] = serde_json::Value::Null;
        }
        backend.create_record(TABLE, row).await.unwrap();
    }
    let v = Valence::builder()
        .add_backend("default", backend)
        .build()
        .expect("build");

    let mut groups = QueryCore::new(TABLE.into())
        .sum_values("n", Some("kind"), &v, purpose())
        .await
        .unwrap();
    groups.sort_by(|a, b| a.key.cmp(&b.key));
    let expected = |k: i64| (0..1500_i64).filter(|i| i % 2 == k && *i != 7).sum::<i64>();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].key.as_deref(), Some("k0"));
    assert_eq!(groups[0].sum, expected(0));
    assert_eq!(groups[0].rows, 750);
    assert_eq!(groups[1].sum, expected(1));
    assert_eq!(groups[1].rows, 749);

    let empty = QueryCore::new("mem_sum_missing".into())
        .sum_values("n", None, &v, purpose())
        .await
        .unwrap();
    assert_eq!((empty.len(), empty[0].sum, empty[0].rows), (1, 0, 0));
}
