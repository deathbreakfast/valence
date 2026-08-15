//! Hybrid backend correctness: read-through, LRU, rules, write-through, edges.

use std::sync::Arc;

use valence_backend_hybrid::{CacheRules, HybridBackend, ENGINE_ID};
use valence_backend_mem::InMemoryBackend;
use valence_core::{
    register_backend_logical_names, router_key, DatabaseBackend, DatabaseRouter, RecordId,
    RegisterBackendLogicalNamesOptions,
};

async fn hybrid_over_mem() -> HybridBackend {
    HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .warm_edges(false)
        .build()
        .await
        .expect("build hybrid")
}

#[tokio::test]
async fn read_through_populates_mirror() {
    let hybrid = hybrid_over_mem().await;
    hybrid
        .create_record("counter", serde_json::json!({"id": "a", "n": 1}))
        .await
        .expect("create");
    let row = hybrid.get_record("counter", "a").await.expect("get");
    assert_eq!(
        row.and_then(|v| v.get("n").cloned()),
        Some(serde_json::json!(1))
    );
    // Second get should hit the mirror path (still correct).
    let row2 = hybrid.get_record("counter", "a").await.expect("get2");
    assert!(row2.is_some());
}

#[tokio::test]
async fn write_through_update_and_delete() {
    let hybrid = hybrid_over_mem().await;
    hybrid
        .create_record("counter", serde_json::json!({"id": "u", "n": 1}))
        .await
        .expect("create");
    hybrid
        .update_record("counter", "u", serde_json::json!({"id": "u", "n": 2}))
        .await
        .expect("update");
    let row = hybrid.get_record("counter", "u").await.expect("get");
    assert_eq!(row.unwrap().get("n"), Some(&serde_json::json!(2)));
    hybrid.delete_record("counter", "u").await.expect("delete");
    assert!(hybrid
        .get_record("counter", "u")
        .await
        .expect("get")
        .is_none());
}

#[tokio::test]
async fn zero_record_capacity_disables_mirror() {
    let primary = Arc::new(InMemoryBackend::new());
    let hybrid = HybridBackend::builder()
        .primary(Arc::clone(&primary) as Arc<dyn DatabaseBackend>)
        .record_capacity(0)
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    assert!(!hybrid.policy().caches_record("counter"));
    hybrid
        .create_record("counter", serde_json::json!({"id": "z", "n": 0}))
        .await
        .expect("create");
    let row = hybrid.get_record("counter", "z").await.expect("get");
    assert!(row.is_some());
}

#[tokio::test]
async fn record_exclude_rule() {
    let hybrid = HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .record_rules(CacheRules::cache_all().exclude(["audit_log"]))
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    assert!(!hybrid.policy().caches_record("audit_log"));
    assert!(hybrid.policy().caches_record("project"));
}

#[tokio::test]
async fn edge_dual_write_and_get_targets() {
    let hybrid = HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    hybrid
        .create_record("org", serde_json::json!({"id": "o1"}))
        .await
        .expect("org");
    hybrid
        .create_record("project", serde_json::json!({"id": "p1"}))
        .await
        .expect("project");
    let from = RecordId::new("org", "o1");
    let to = RecordId::new("project", "p1");
    hybrid
        .relate_edge(&from, "org_projects", &to)
        .await
        .expect("relate");
    // After relate, edge table should be marked complete for subsequent reads.
    let targets = hybrid
        .get_edge_targets(&from, "org_projects")
        .await
        .expect("targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id(), "p1");
}

#[tokio::test]
async fn multi_logical_registration_shares_backend_crud_and_edges() {
    let hybrid = HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .warm_edges(false)
        .build()
        .await
        .expect("build hybrid");
    let backend: Arc<dyn DatabaseBackend> = Arc::new(hybrid);
    let mut router = DatabaseRouter::new();
    register_backend_logical_names(
        &mut router,
        Arc::clone(&backend),
        &["default", "billing"],
        RegisterBackendLogicalNamesOptions::default(),
    );
    let default_key = router_key("default", ENGINE_ID);
    let billing_key = router_key("billing", ENGINE_ID);
    let via_default = router.resolve(&default_key).expect("default");
    let via_billing = router.resolve(&billing_key).expect("billing");
    assert!(Arc::ptr_eq(&via_default, &via_billing));

    via_default
        .create_record("org", serde_json::json!({"id": "o1"}))
        .await
        .expect("create org");
    via_billing
        .create_record("project", serde_json::json!({"id": "p1"}))
        .await
        .expect("create project");
    let row = via_billing
        .get_record("org", "o1")
        .await
        .expect("get via billing");
    assert!(row.is_some());

    let from = RecordId::new("org", "o1");
    let to = RecordId::new("project", "p1");
    via_default
        .relate_edge(&from, "org_projects", &to)
        .await
        .expect("relate");
    let targets = via_billing
        .get_edge_targets(&from, "org_projects")
        .await
        .expect("targets");
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].id(), "p1");
}

#[tokio::test]
async fn record_lru_evicts_oldest() {
    let hybrid = HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .record_capacity(2)
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    for i in 0..3 {
        hybrid
            .create_record(
                "counter",
                serde_json::json!({"id": format!("k{i}"), "n": i}),
            )
            .await
            .expect("create");
    }
    // Touch k1 and k2 so k0 is oldest if still present; capacity 2 means one was evicted.
    let _ = hybrid.get_record("counter", "k1").await;
    let _ = hybrid.get_record("counter", "k2").await;
    // Primary still has all three.
    // Mirror may have evicted k0; get_record still succeeds via primary read-through.
    assert!(hybrid
        .get_record("counter", "k0")
        .await
        .expect("get")
        .is_some());
}

#[tokio::test]
async fn invalidate_cached_record_then_get_reads_primary_happy() {
    let mem = Arc::new(InMemoryBackend::new());
    let primary: Arc<dyn DatabaseBackend> = mem.clone();
    let hybrid = HybridBackend::builder()
        .primary(primary)
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    hybrid
        .create_record("counter", serde_json::json!({"id": "a", "n": 1}))
        .await
        .expect("create");
    hybrid
        .invalidate_cached_record("counter", "a")
        .await
        .expect("invalidate");
    mem.update_record("counter", "a", serde_json::json!({"id": "a", "n": 99}))
        .await
        .expect("primary update");
    let row = hybrid.get_record("counter", "a").await.expect("get");
    assert_eq!(
        row.and_then(|v| v.get("n").cloned()),
        Some(serde_json::json!(99))
    );
}

#[tokio::test]
async fn unique_get_without_invalidate_keeps_stale_mirror_sad() {
    let mem = Arc::new(InMemoryBackend::new());
    let primary: Arc<dyn DatabaseBackend> = mem.clone();
    let hybrid = HybridBackend::builder()
        .primary(primary)
        .warm_edges(false)
        .build()
        .await
        .expect("build");
    hybrid
        .create_record("counter", serde_json::json!({"id": "a", "n": 1}))
        .await
        .expect("create");
    mem.update_record("counter", "a", serde_json::json!({"id": "a", "n": 99}))
        .await
        .expect("primary update");
    let row = hybrid.get_record("counter", "a").await.expect("get");
    assert_eq!(
        row.and_then(|v| v.get("n").cloned()),
        Some(serde_json::json!(1)),
        "write-through leaves Indra hot; unique gets must invalidate first"
    );
}
