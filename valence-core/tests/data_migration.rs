//! Data migration runner against the in-memory backend: gates, ordering, ledger rows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde_json::json;
use valence_backend_mem::InMemoryBackend;
use valence_core::data_migration::{
    DataMigration, MigrationApproval, MigrationReport, MigrationRunner, MigrationStatus,
    DATA_MIGRATION_LEDGER_TABLE,
};
use valence_core::{Actor, DatabaseBackend, Error, Result, Valence};

struct Recording {
    id: &'static str,
    version: u32,
    description: &'static str,
    sensitive: bool,
    fail: bool,
    calls: Arc<AtomicU32>,
    order: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

#[async_trait::async_trait]
impl DataMigration for Recording {
    fn id(&self) -> &str {
        self.id
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn description(&self) -> &str {
        self.description
    }
    fn tables(&self) -> &[&str] {
        &["profile"]
    }
    fn touches_sensitive_data(&self) -> bool {
        self.sensitive
    }
    async fn apply(&self, _valence: &Valence, dry_run: bool) -> Result<MigrationReport> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.order.lock().unwrap().push(self.id);
        if self.fail {
            return Err(Error::database(
                "write failed at postgres://admin:hunter2@db.internal/app",
            ));
        }
        Ok(MigrationReport {
            rows_touched: if dry_run { 7 } else { 3 },
        })
    }
}

struct Fixture {
    calls: Arc<AtomicU32>,
    order: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl Fixture {
    fn new() -> Self {
        Self {
            calls: Arc::new(AtomicU32::new(0)),
            order: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn migration(&self, id: &'static str, version: u32) -> Recording {
        Recording {
            id,
            version,
            description: "backfill",
            sensitive: false,
            fail: false,
            calls: Arc::clone(&self.calls),
            order: Arc::clone(&self.order),
        }
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

fn valence_as(actor: Actor) -> (Valence, Arc<InMemoryBackend>) {
    let backend = Arc::new(InMemoryBackend::new());
    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend) as Arc<dyn DatabaseBackend>)
        .build()
        .expect("build")
        .with_actor(actor);
    (valence, backend)
}

fn system() -> Actor {
    Actor::System {
        operation: "data_migration".into(),
    }
}

fn approval() -> MigrationApproval {
    MigrationApproval {
        approved_by: "user:ops-lead".into(),
        reason: "profiles before 0.4 lack display names".into(),
        ticket: Some("OPS-112".into()),
    }
}

async fn ledger_row(backend: &InMemoryBackend, id: &str) -> Option<serde_json::Value> {
    backend
        .get_record(DATA_MIGRATION_LEDGER_TABLE, id)
        .await
        .expect("ledger read")
}

#[tokio::test]
async fn applies_in_version_order_and_records_ledger() {
    let fx = Fixture::new();
    let (valence, backend) = valence_as(system());
    let outcomes = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("second", 2)))
        .register(Arc::new(fx.migration("first", 1)))
        .approve(approval())
        .run()
        .await
        .expect("run");

    assert_eq!(*fx.order.lock().unwrap(), vec!["first", "second"]);
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .all(|o| o.status == MigrationStatus::Applied && o.rows_touched == 3));

    let row = ledger_row(&backend, "first").await.expect("ledger row");
    assert_eq!(row["outcome"], json!("applied"));
    assert_eq!(row["version"], json!(1));
    assert_eq!(row["approved_by"], json!("user:ops-lead"));
    assert_eq!(row["ticket"], json!("OPS-112"));
    assert_eq!(row["tables"], json!(["profile"]));
    assert_eq!(row["sensitive"], json!(false));
    assert_eq!(row["rows_touched"], json!(3));
    assert_eq!(row["actor"]["System"]["operation"], json!("data_migration"));
    assert!(row["started_at"].is_string() && row["finished_at"].is_string());
}

#[tokio::test]
async fn second_run_skips_applied_migrations() {
    let fx = Fixture::new();
    let (valence, _backend) = valence_as(system());
    for _ in 0..2 {
        MigrationRunner::new(&valence)
            .register(Arc::new(fx.migration("first", 1)))
            .approve(approval())
            .run()
            .await
            .expect("run");
    }
    assert_eq!(fx.calls(), 1);

    let outcomes = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .register(Arc::new(fx.migration("second", 2)))
        .approve(approval())
        .run()
        .await
        .expect("run");
    assert_eq!(outcomes[0].status, MigrationStatus::Skipped);
    assert_eq!(outcomes[1].status, MigrationStatus::Applied);
    assert_eq!(fx.calls(), 2);
}

#[tokio::test]
async fn service_user_may_run_migrations() {
    let fx = Fixture::new();
    let (valence, _backend) = valence_as(Actor::ServiceUser {
        service_name: "migrator".into(),
    });
    MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .approve(approval())
        .run()
        .await
        .expect("run");
    assert_eq!(fx.calls(), 1);
}

#[tokio::test]
async fn dry_run_applies_nothing_to_the_ledger() {
    let fx = Fixture::new();
    let (valence, backend) = valence_as(system());
    let outcomes = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .approve(approval())
        .dry_run(true)
        .run()
        .await
        .expect("dry run");
    assert_eq!(outcomes[0].status, MigrationStatus::DryRun);
    assert_eq!(outcomes[0].rows_touched, 7);
    assert!(ledger_row(&backend, "first").await.is_none());
}

#[tokio::test]
async fn user_and_anonymous_actors_are_refused() {
    for actor in [
        Actor::User {
            user_id: "u1".into(),
        },
        Actor::Anonymous,
    ] {
        let fx = Fixture::new();
        let (valence, backend) = valence_as(actor);
        let err = MigrationRunner::new(&valence)
            .register(Arc::new(fx.migration("first", 1)))
            .approve(approval())
            .run()
            .await
            .expect_err("refused");
        assert!(matches!(err, Error::Privacy(_)), "{err:?}");
        assert_eq!(fx.calls(), 0);
        assert!(ledger_row(&backend, "first").await.is_none());
    }
}

#[tokio::test]
async fn missing_or_blank_approval_is_refused() {
    let fx = Fixture::new();
    let (valence, _backend) = valence_as(system());
    let err = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .run()
        .await
        .expect_err("no approval");
    assert!(matches!(err, Error::Validation(_)));

    let mut blank = approval();
    blank.reason = "   ".into();
    let err = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .approve(blank)
        .run()
        .await
        .expect_err("blank reason");
    assert!(matches!(err, Error::Validation(_)));
    assert_eq!(fx.calls(), 0);
}

#[tokio::test]
async fn sensitive_migration_needs_acknowledgment() {
    let fx = Fixture::new();
    let (valence, backend) = valence_as(system());
    let mut rekey = fx.migration("rekey", 1);
    rekey.sensitive = true;
    let rekey = Arc::new(rekey);

    let err = MigrationRunner::new(&valence)
        .register(Arc::clone(&rekey) as Arc<dyn DataMigration>)
        .register(Arc::new(fx.migration("plain", 2)))
        .approve(approval())
        .run()
        .await
        .expect_err("unacknowledged");
    assert!(
        matches!(err, Error::Validation(ref m) if m.contains("rekey")),
        "{err:?}"
    );
    assert_eq!(fx.calls(), 0, "no migration runs when any gate fails");

    MigrationRunner::new(&valence)
        .register(rekey)
        .approve(approval())
        .acknowledge_sensitive("rekey")
        .run()
        .await
        .expect("acknowledged");
    let row = ledger_row(&backend, "rekey").await.expect("ledger row");
    assert_eq!(row["sensitive"], json!(true));
}

#[tokio::test]
async fn failed_apply_is_recorded_and_retry_succeeds() {
    let fx = Fixture::new();
    let (valence, backend) = valence_as(system());
    let mut broken = fx.migration("first", 1);
    broken.fail = true;

    let err = MigrationRunner::new(&valence)
        .register(Arc::new(broken))
        .register(Arc::new(fx.migration("second", 2)))
        .approve(approval())
        .run()
        .await
        .expect_err("apply fails");
    assert!(matches!(err, Error::Database { .. }));
    assert_eq!(
        *fx.order.lock().unwrap(),
        vec!["first"],
        "run stops at the failure"
    );
    assert!(ledger_row(&backend, "first").await.is_none());

    MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .register(Arc::new(fx.migration("second", 2)))
        .approve(approval())
        .run()
        .await
        .expect("retry");
    assert_eq!(
        ledger_row(&backend, "first").await.unwrap()["outcome"],
        json!("applied")
    );
}

#[tokio::test]
async fn edited_applied_migration_is_reported_as_drift() {
    let fx = Fixture::new();
    let (valence, _backend) = valence_as(system());
    MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .approve(approval())
        .run()
        .await
        .expect("run");

    let mut edited = fx.migration("first", 1);
    edited.description = "backfill, now with trimming";
    let err = MigrationRunner::new(&valence)
        .register(Arc::new(edited))
        .register(Arc::new(fx.migration("second", 2)))
        .approve(approval())
        .run()
        .await
        .expect_err("drift");
    assert!(
        matches!(err, Error::Validation(ref m) if m.contains("first")),
        "{err:?}"
    );
    assert_eq!(fx.calls(), 1, "drift stops the run before anything applies");
}

#[tokio::test]
async fn existing_ledger_row_is_never_overwritten() {
    let fx = Fixture::new();
    let (valence, backend) = valence_as(system());
    // A row with the migration's id exists but records a different version: the runner
    // treats it as drift and leaves it in place.
    backend
        .create_record(
            DATA_MIGRATION_LEDGER_TABLE,
            json!({"id": "first", "version": 9, "description": "other", "outcome": "applied"}),
        )
        .await
        .expect("seed");
    let err = MigrationRunner::new(&valence)
        .register(Arc::new(fx.migration("first", 1)))
        .approve(approval())
        .run()
        .await
        .expect_err("refused");
    assert!(matches!(err, Error::Validation(_)));
    assert_eq!(fx.calls(), 0);
    assert_eq!(
        ledger_row(&backend, "first").await.unwrap()["version"],
        json!(9)
    );
}
