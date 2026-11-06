//! Run versioned data migrations behind security gates and record each run in a ledger.
//!
//! Boot sync ([`crate::Valence::sync_typed_tables_from_registry`]) changes the physical
//! layout of a table. A data migration rewrites the rows themselves: backfilling a field,
//! splitting a column, re-encoding stored values. This module runs those rewrites in
//! version order, refuses to start unless every gate passes, and writes an audit row for
//! each run to [`DATA_MIGRATION_LEDGER_TABLE`].
//!
//! # Running migrations
//!
//! ```no_run
//! use std::sync::Arc;
//! use valence_core::data_migration::{
//!     DataMigration, MigrationApproval, MigrationReport, MigrationRunner,
//! };
//! use valence_core::{Actor, Result, Valence};
//!
//! struct BackfillDisplayName;
//!
//! #[async_trait::async_trait]
//! impl DataMigration for BackfillDisplayName {
//!     fn id(&self) -> &str { "backfill_display_name" }
//!     fn version(&self) -> u32 { 1 }
//!     fn description(&self) -> &str { "Copy handle into display_name where it is empty" }
//!     fn tables(&self) -> &[&str] { &["profile"] }
//!     fn touches_sensitive_data(&self) -> bool { false }
//!     async fn apply(&self, _valence: &Valence, _dry_run: bool) -> Result<MigrationReport> {
//!         Ok(MigrationReport { rows_touched: 0 })
//!     }
//! }
//!
//! # async fn run(valence: Valence) -> Result<()> {
//! // The job itself starts as System; the runner never elevates the handle it is given.
//! let valence = valence.with_actor(Actor::System { operation: "data_migration".into() });
//! let outcomes = MigrationRunner::new(&valence)
//!     .register(Arc::new(BackfillDisplayName))
//!     .approve(MigrationApproval {
//!         approved_by: "user:ops-lead".into(),
//!         reason: "Profiles created before 0.4 have no display name".into(),
//!         ticket: Some("OPS-112".into()),
//!     })
//!     .run()
//!     .await?;
//! # let _ = outcomes;
//! # Ok(())
//! # }
//! ```
//!
//! # Gates
//!
//! [`MigrationRunner::run`] checks every gate before it calls any [`DataMigration::apply`]:
//!
//! 1. **Actor.** The handle's actor must be [`Actor::System`] or [`Actor::ServiceUser`].
//!    A user or anonymous handle gets [`Error::Privacy`]. The runner does not call
//!    [`crate::Valence::with_actor`], so a migration job has to start as a privileged actor;
//!    a request handler cannot raise itself into one through this API.
//! 2. **Approval.** A [`MigrationApproval`] with a non-blank `approved_by` and `reason`.
//! 3. **Classification.** A migration that reports [`DataMigration::touches_sensitive_data`]
//!    runs only after [`MigrationRunner::acknowledge_sensitive`] names its id. Every
//!    migration must list at least one table.
//! 4. **Registration.** Ids are safe identifiers (ASCII alphanumeric and `_`), and ids and
//!    versions are unique.
//!
//! # Ledger
//!
//! The ledger lives on the active backend in the schemaless table
//! [`DATA_MIGRATION_LEDGER_TABLE`]. A successful run writes one row keyed by the
//! migration id; a failed run writes its own row keyed `{id}__failed__{nanos}` with the
//! error text (credentials redacted), so a failure never blocks a retry. For a migration
//! that touches sensitive data, the failed row keeps only the error kind (`database`,
//! `validation`, ...), since the message could quote a field value. The runner only
//! creates rows and refuses to overwrite one that exists. Each row records the migration
//! id, version, description, tables, sensitive flag, actor, approver, reason, ticket,
//! start and finish time, rows touched and outcome.
//!
//! A migration whose applied row exists is skipped. If that row's version or description
//! no longer matches the registered migration, the run stops with [`Error::Validation`]
//! before anything is applied.
//!
//! A dry run passes the same gates, calls `apply(.., true)` and writes no ledger rows.
//!
//! # Gotchas
//!
//! - Migrations run one at a time in version order, and the run stops at the first failure.
//!   Migrations are not wrapped in a transaction; make `apply` safe to rerun.
//! - Run only one runner per deployment at a time. The ledger check and write are not
//!   atomic, so two runners started together can both apply the same migration.
//! - Encrypted transport and at-rest protection for migrated data are not checked here yet.

use std::collections::HashSet;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;

use crate::actor::Actor;
use crate::error::{Error, Result};
use crate::redact::redact_credentials_in_text;
use crate::runtime::Valence;
use crate::safe_ident::assert_safe_ident;

/// Schemaless table that holds one audit row per migration run.
pub const DATA_MIGRATION_LEDGER_TABLE: &str = "valence_data_migrations";

/// One versioned data rewrite.
#[async_trait::async_trait]
pub trait DataMigration: Send + Sync {
    /// Stable id, also the ledger row id. ASCII alphanumeric and `_` only.
    fn id(&self) -> &str;

    /// Run order; lower versions run first. Unique across registered migrations.
    fn version(&self) -> u32;

    /// What the migration does, stored in the ledger and compared on later runs.
    fn description(&self) -> &str;

    /// Tables the migration reads or writes. Must not be empty.
    fn tables(&self) -> &[&str];

    /// Whether the migration reads or writes sensitive data (credentials, personal data).
    fn touches_sensitive_data(&self) -> bool;

    /// Apply the migration. With `dry_run` set, report what would change without writing.
    async fn apply(&self, valence: &Valence, dry_run: bool) -> Result<MigrationReport>;
}

/// What a migration reports back from [`DataMigration::apply`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MigrationReport {
    /// Rows changed (or, in a dry run, rows that would change).
    pub rows_touched: u64,
}

/// Who approved a migration run, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationApproval {
    /// The approving person or principal, for example a user id.
    pub approved_by: String,
    /// Why the migration runs.
    pub reason: String,
    /// Optional change or incident reference.
    pub ticket: Option<String>,
}

/// What happened to one registered migration during [`MigrationRunner::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    /// Applied in this run and recorded in the ledger.
    Applied,
    /// Already recorded as applied; not run again.
    Skipped,
    /// Ran with `dry_run` set; nothing recorded.
    DryRun,
}

/// Per-migration result of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationOutcome {
    /// Migration id.
    pub id: String,
    /// Migration version.
    pub version: u32,
    /// What happened.
    pub status: MigrationStatus,
    /// Rows the migration reported; zero when skipped.
    pub rows_touched: u64,
}

/// Applies registered [`DataMigration`]s in version order once every gate passes.
pub struct MigrationRunner<'a> {
    valence: &'a Valence,
    migrations: Vec<Arc<dyn DataMigration>>,
    approval: Option<MigrationApproval>,
    acknowledged_sensitive: HashSet<String>,
    dry_run: bool,
}

impl<'a> MigrationRunner<'a> {
    /// Runner over `valence`, whose actor must be System or a service user.
    #[must_use]
    pub fn new(valence: &'a Valence) -> Self {
        Self {
            valence,
            migrations: Vec::new(),
            approval: None,
            acknowledged_sensitive: HashSet::new(),
            dry_run: false,
        }
    }

    /// Add a migration to the run.
    #[must_use]
    pub fn register(mut self, migration: Arc<dyn DataMigration>) -> Self {
        self.migrations.push(migration);
        self
    }

    /// Record who approved the run. Required.
    #[must_use]
    pub fn approve(mut self, approval: MigrationApproval) -> Self {
        self.approval = Some(approval);
        self
    }

    /// Allow the sensitive migration `id` to run.
    #[must_use]
    pub fn acknowledge_sensitive(mut self, id: impl Into<String>) -> Self {
        self.acknowledged_sensitive.insert(id.into());
        self
    }

    /// Run every gate and `apply(.., true)` without writing ledger rows.
    #[must_use]
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    /// Check the gates, then apply pending migrations in version order.
    ///
    /// # Errors
    ///
    /// - [`Error::Privacy`] when the actor is a user or anonymous.
    /// - [`Error::Validation`] for a missing or blank approval, an unacknowledged sensitive
    ///   migration, a bad or duplicate id, a duplicate version, an empty table list, an
    ///   existing ledger row that would be overwritten, or a ledger row that no longer
    ///   matches its migration.
    /// - The migration's own error when `apply` fails; the failure is recorded in the ledger
    ///   (outside dry runs) and later migrations do not run.
    /// - Backend errors from reading or writing the ledger.
    pub async fn run(mut self) -> Result<Vec<MigrationOutcome>> {
        check_actor(self.valence.actor())?;
        let approval = check_approval(self.approval.as_ref())?;
        self.migrations.sort_by_key(|m| m.version());
        check_registrations(&self.migrations, &self.acknowledged_sensitive)?;

        let backend = self.valence.active_backend()?;
        backend
            .ensure_schemaless_table(DATA_MIGRATION_LEDGER_TABLE)
            .await?;

        // Read the whole ledger state first so drift stops the run before anything applies.
        let mut pending = Vec::with_capacity(self.migrations.len());
        for migration in &self.migrations {
            let applied = backend
                .get_record(DATA_MIGRATION_LEDGER_TABLE, migration.id())
                .await?;
            match applied {
                Some(row) => check_ledger_matches(migration.as_ref(), &row)?,
                None => pending.push(Arc::clone(migration)),
            }
        }

        let actor = serde_json::to_value(self.valence.actor())?;
        let mut outcomes = Vec::with_capacity(self.migrations.len());
        for migration in &self.migrations {
            if !pending.iter().any(|p| p.id() == migration.id()) {
                outcomes.push(MigrationOutcome {
                    id: migration.id().to_string(),
                    version: migration.version(),
                    status: MigrationStatus::Skipped,
                    rows_touched: 0,
                });
                continue;
            }

            let started_at = Utc::now();
            let result = migration.apply(self.valence, self.dry_run).await;
            if self.dry_run {
                let report = result?;
                outcomes.push(MigrationOutcome {
                    id: migration.id().to_string(),
                    version: migration.version(),
                    status: MigrationStatus::DryRun,
                    rows_touched: report.rows_touched,
                });
                continue;
            }

            let finished_at = Utc::now();
            let mut row = json!({
                "migration_id": migration.id(),
                "version": migration.version(),
                "description": migration.description(),
                "tables": migration.tables(),
                "sensitive": migration.touches_sensitive_data(),
                "actor": actor,
                "approved_by": approval.approved_by,
                "reason": approval.reason,
                "ticket": approval.ticket,
                "started_at": started_at.to_rfc3339(),
                "finished_at": finished_at.to_rfc3339(),
            });
            let (row_id, err) = match result {
                Ok(report) => {
                    row["outcome"] = json!("applied");
                    row["rows_touched"] = json!(report.rows_touched);
                    outcomes.push(MigrationOutcome {
                        id: migration.id().to_string(),
                        version: migration.version(),
                        status: MigrationStatus::Applied,
                        rows_touched: report.rows_touched,
                    });
                    (migration.id().to_string(), None)
                }
                Err(e) => {
                    row["outcome"] = json!("failed");
                    row["rows_touched"] = json!(0);
                    row["error"] = json!(failure_text(&e, migration.touches_sensitive_data()));
                    let nanos = started_at.timestamp_nanos_opt().unwrap_or_default();
                    (format!("{}__failed__{nanos}", migration.id()), Some(e))
                }
            };
            row["id"] = json!(row_id);
            write_ledger_row(self.valence, &row_id, row).await?;
            if let Some(e) = err {
                return Err(e);
            }
        }
        Ok(outcomes)
    }
}

/// Create a ledger row, refusing to replace one that already exists.
async fn write_ledger_row(valence: &Valence, row_id: &str, row: serde_json::Value) -> Result<()> {
    let backend = valence.active_backend()?;
    if backend
        .get_record(DATA_MIGRATION_LEDGER_TABLE, row_id)
        .await?
        .is_some()
    {
        return Err(Error::Validation(format!(
            "data migration ledger row {row_id} already exists; ledger rows are never overwritten"
        )));
    }
    backend
        .create_record(DATA_MIGRATION_LEDGER_TABLE, row)
        .await?;
    Ok(())
}

/// Error text for a failed ledger row: the error kind alone for a sensitive migration,
/// otherwise the message with URL credentials removed.
fn failure_text(err: &Error, sensitive: bool) -> String {
    if sensitive {
        return error_kind(err).to_string();
    }
    redact_credentials_in_text(&err.to_string())
}

fn error_kind(err: &Error) -> &'static str {
    match err {
        Error::Database { .. } => "database",
        Error::NotFound(_) => "not_found",
        Error::Validation(_) => "validation",
        Error::Privacy(_) => "privacy",
        Error::Internal(_) => "internal",
        Error::Serialization { .. } => "serialization",
        Error::PendingDeletion(_) => "pending_deletion",
        Error::Identity(_) => "identity",
    }
}

fn check_actor(actor: &Actor) -> Result<()> {
    match actor {
        Actor::System { .. } | Actor::ServiceUser { .. } => Ok(()),
        Actor::User { .. } | Actor::Anonymous => Err(Error::Privacy(
            "data migrations run only as System or a service user; start the migration job \
             as one instead of elevating a request handle"
                .into(),
        )),
    }
}

fn check_approval(approval: Option<&MigrationApproval>) -> Result<&MigrationApproval> {
    let approval = approval.ok_or_else(|| {
        Error::Validation("data migrations need an approval (approved_by and reason)".into())
    })?;
    if approval.approved_by.trim().is_empty() {
        return Err(Error::Validation(
            "data migration approval has a blank approved_by".into(),
        ));
    }
    if approval.reason.trim().is_empty() {
        return Err(Error::Validation(
            "data migration approval has a blank reason".into(),
        ));
    }
    Ok(approval)
}

/// Check ids, versions, tables and sensitive acknowledgments. `migrations` must be sorted
/// by version.
fn check_registrations(
    migrations: &[Arc<dyn DataMigration>],
    acknowledged_sensitive: &HashSet<String>,
) -> Result<()> {
    let mut ids = HashSet::new();
    for (i, migration) in migrations.iter().enumerate() {
        let id = migration.id();
        assert_safe_ident(id).map_err(|_| {
            Error::Validation(format!("data migration id {id:?} is not a safe identifier"))
        })?;
        if !ids.insert(id) {
            return Err(Error::Validation(format!(
                "data migration id {id} is registered twice"
            )));
        }
        if i > 0 && migrations[i - 1].version() == migration.version() {
            return Err(Error::Validation(format!(
                "data migrations {} and {id} share version {}",
                migrations[i - 1].id(),
                migration.version()
            )));
        }
        if migration.tables().is_empty() {
            return Err(Error::Validation(format!(
                "data migration {id} lists no tables"
            )));
        }
        if migration.touches_sensitive_data() && !acknowledged_sensitive.contains(id) {
            return Err(Error::Validation(format!(
                "data migration {id} touches sensitive data; call acknowledge_sensitive(\"{id}\") \
                 to run it"
            )));
        }
    }
    Ok(())
}

fn check_ledger_matches(migration: &dyn DataMigration, row: &serde_json::Value) -> Result<()> {
    let version = row.get("version").and_then(serde_json::Value::as_u64);
    let description = row.get("description").and_then(serde_json::Value::as_str);
    if version == Some(u64::from(migration.version()))
        && description == Some(migration.description())
    {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "data migration {} was applied as version {version:?} ({description:?}) but is \
             registered as version {} ({:?}); add a new migration instead of editing an \
             applied one",
            migration.id(),
            migration.version(),
            migration.description()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Stub {
        id: &'static str,
        version: u32,
        tables: &'static [&'static str],
        sensitive: bool,
    }

    #[allow(clippy::unnecessary_literal_bound)]
    #[async_trait::async_trait]
    impl DataMigration for Stub {
        fn id(&self) -> &str {
            self.id
        }
        fn version(&self) -> u32 {
            self.version
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn tables(&self) -> &[&str] {
            self.tables
        }
        fn touches_sensitive_data(&self) -> bool {
            self.sensitive
        }
        async fn apply(&self, _valence: &Valence, _dry_run: bool) -> Result<MigrationReport> {
            Ok(MigrationReport::default())
        }
    }

    fn stub(id: &'static str, version: u32) -> Arc<dyn DataMigration> {
        Arc::new(Stub {
            id,
            version,
            tables: &["profile"],
            sensitive: false,
        })
    }

    fn approval() -> MigrationApproval {
        MigrationApproval {
            approved_by: "user:ops".into(),
            reason: "backfill".into(),
            ticket: None,
        }
    }

    #[test]
    fn actor_gate_allows_system_and_service_user() {
        assert!(check_actor(&Actor::System {
            operation: "migrate".into()
        })
        .is_ok());
        assert!(check_actor(&Actor::ServiceUser {
            service_name: "migrator".into()
        })
        .is_ok());
    }

    #[test]
    fn actor_gate_refuses_user_and_anonymous() {
        let user = Actor::User {
            user_id: "u1".into(),
        };
        assert!(matches!(check_actor(&user), Err(Error::Privacy(_))));
        assert!(matches!(
            check_actor(&Actor::Anonymous),
            Err(Error::Privacy(_))
        ));
    }

    #[test]
    fn approval_gate_requires_approver_and_reason() {
        assert!(check_approval(None).is_err());
        let mut a = approval();
        a.approved_by = "  ".into();
        assert!(check_approval(Some(&a)).is_err());
        let mut a = approval();
        a.reason = String::new();
        assert!(check_approval(Some(&a)).is_err());
        assert!(check_approval(Some(&approval())).is_ok());
    }

    #[test]
    fn registrations_refuse_duplicate_ids_and_versions() {
        let none = HashSet::new();
        assert!(check_registrations(&[stub("a", 1), stub("a", 2)], &none).is_err());
        assert!(check_registrations(&[stub("a", 1), stub("b", 1)], &none).is_err());
        assert!(check_registrations(&[stub("a", 1), stub("b", 2)], &none).is_ok());
    }

    #[test]
    fn registrations_refuse_unsafe_id_and_empty_tables() {
        let none = HashSet::new();
        assert!(check_registrations(&[stub("a:b", 1)], &none).is_err());
        let empty: Arc<dyn DataMigration> = Arc::new(Stub {
            id: "a",
            version: 1,
            tables: &[],
            sensitive: false,
        });
        assert!(check_registrations(&[empty], &none).is_err());
    }

    #[test]
    fn sensitive_migration_needs_acknowledgment() {
        let sensitive: Arc<dyn DataMigration> = Arc::new(Stub {
            id: "rekey",
            version: 1,
            tables: &["secret"],
            sensitive: true,
        });
        let mut acked = HashSet::new();
        let err = check_registrations(std::slice::from_ref(&sensitive), &acked);
        assert!(matches!(err, Err(Error::Validation(m)) if m.contains("acknowledge_sensitive")));
        acked.insert("rekey".to_string());
        assert!(check_registrations(&[sensitive], &acked).is_ok());
    }

    #[test]
    fn failure_text_redacts_credentials() {
        let err = Error::Internal("lost postgres://admin:hunter2@db.internal/app".into());
        let text = failure_text(&err, false);
        assert!(!text.contains("hunter2"), "{text}");
        assert!(text.contains("db.internal"), "{text}");
    }

    #[test]
    fn failure_text_keeps_only_the_kind_for_sensitive_migrations() {
        let err = Error::Validation("bad email alice@example.com".into());
        assert_eq!(failure_text(&err, true), "validation");
    }

    #[test]
    fn ledger_match_detects_drift() {
        let m = Stub {
            id: "a",
            version: 2,
            tables: &["t"],
            sensitive: false,
        };
        assert!(check_ledger_matches(&m, &json!({"version": 2, "description": "stub"})).is_ok());
        assert!(check_ledger_matches(&m, &json!({"version": 1, "description": "stub"})).is_err());
        assert!(check_ledger_matches(&m, &json!({"version": 2, "description": "old"})).is_err());
    }
}
