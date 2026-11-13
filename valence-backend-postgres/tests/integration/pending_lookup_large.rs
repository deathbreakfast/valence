//! The pending-deletion lookup answers for more than 1000 ids on Postgres, in chunks.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::expect_used, clippy::print_stderr)]

use std::collections::HashSet;
use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_core::actor::Actor;
use valence_core::{DatabaseBackend, OwnershipService, Valence};

#[tokio::test]
async fn pending_lookup_over_1000_ids_happy() {
    let backend = match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("postgres connect failed: {e} — skipping");
            return;
        }
    };
    let backend: Arc<dyn DatabaseBackend> = Arc::new(backend);
    let v = Valence::builder()
        .add_backend("default", backend)
        .with_actor(Actor::User {
            user_id: "alice".into(),
        })
        .build()
        .expect("build");
    // A model name of its own, so reruns against a long-lived database don't collide.
    let model = format!(
        "pending_large_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let ids: Vec<String> = (0..1500).map(|i| format!("r{i:05}")).collect();
    let pending = [7, 999, 1499];
    for &i in &pending {
        OwnershipService::mark_pending_deletion(&model, &ids[i], &v)
            .await
            .expect("mark pending");
    }
    let found = OwnershipService::pending_deletion_bare_ids_subset(&model, &ids, &v)
        .await
        .expect("lookup over 1500 ids");
    let want: HashSet<String> = pending.iter().map(|&i| ids[i].clone()).collect();
    assert_eq!(found, want);
}
