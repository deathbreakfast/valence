//! Model contracts generated from schema DSL.
//!
//! Generated model types implement [`Model`] via `valence-codegen`. See the
//! `valence-codegen` crate README and `examples/codegen-host` for the build pipeline.

use crate::data_use::DataUsePurpose;
use crate::error::Result;
use crate::runtime::Valence;
use async_trait::async_trait;

/// Core trait that all generated models implement.
///
/// CRUD methods route through the active [`Valence`] backend, applying privacy and ownership
/// hooks defined in the source schema.
///
/// Prefer `*_used` methods with `use_!(...)` so declared data uses
/// appear in the transparency catalog. Bare methods remain for migration (warn-only in v1).
///
/// # Examples
///
/// Generated models (from `valence-codegen`) implement this trait. After including
/// `$OUT_DIR/generated_models.rs`:
///
/// ```ignore
/// use valence::{use_, Model};
///
/// let created = Widget::create_used(widget, &valence, use_!("Seed demo widget.")).await?;
/// let loaded = Widget::get_used(created.id(), &valence, use_!("Reload after create.")).await?;
/// Widget::update_used(created.id(), updated, &valence, use_!("Apply edits.")).await?;
/// Widget::delete_used(created.id(), &valence, use_!("Remove demo row.")).await?;
/// ```
///
/// See workspace `examples/codegen-host` and `examples/product-model-host`.
#[async_trait]
pub trait Model: Sized + Send + Sync {
    /// Generated schema metadata type for this model.
    type Schema;
    /// Field-level change set type used by update/merge paths.
    type FieldChanges: Send + Sync;

    /// Physical table name from the schema DSL `table:` key.
    fn table_name() -> &'static str;
    /// Schema version string from the DSL `version:` key.
    fn schema_version() -> &'static str;

    /// Fetch one row by primary key; returns `Ok(None)` when absent **or** when
    /// entity read privacy denies the viewer (uniform not-found).
    #[deprecated(note = "use get_used with use_!(...) for declared data-use transparency")]
    async fn get(id: &str, valence: &Valence) -> Result<Option<Self>>;

    /// Declared read: same as [`Self::get`], with a catalog purpose.
    async fn get_used(
        id: &str,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Option<Self>> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::get(id, valence).await
    }

    /// Insert a new row.
    #[deprecated(note = "use create_used with use_!(...) for declared data-use transparency")]
    async fn create(data: Self, valence: &Valence) -> Result<Self>;

    /// Declared create: same as [`Self::create`], with a catalog purpose.
    async fn create_used(data: Self, valence: &Valence, purpose: DataUsePurpose) -> Result<Self> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::create(data, valence).await
    }

    /// Replace an existing row by id.
    #[deprecated(note = "use update_used with use_!(...) for declared data-use transparency")]
    async fn update(id: &str, data: Self, valence: &Valence) -> Result<Self>;

    /// Declared update: same as [`Self::update`], with a catalog purpose.
    async fn update_used(
        id: &str,
        data: Self,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Self> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::update(id, data, valence).await
    }

    /// Queue a durable deletion run (or hard-delete for deletion-skip platform tables).
    #[deprecated(note = "use delete_used with use_!(...) for declared data-use transparency")]
    async fn delete(id: &str, valence: &Valence) -> Result<()>;

    /// Declared delete: same as [`Self::delete`], with a catalog purpose.
    async fn delete_used(id: &str, valence: &Valence, purpose: DataUsePurpose) -> Result<()> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::delete(id, valence).await
    }

    /// Physically delete this row and its deletion DAG in the current future.
    ///
    /// Authorizes the full DAG under the requesting actor, then applies every node
    /// before returning. Missing rows succeed. A root already owned by a queued
    /// deletion returns [`crate::Error::PendingDeletion`].
    ///
    /// Intentionally unbounded: use only for bounded request workloads. Prefer
    /// [`Self::delete`] for large or retry-heavy graphs.
    ///
    /// # Errors
    ///
    /// Privacy, Restrict validation, pending coordination, or apply failures.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use valence::{use_, Model};
    ///
    /// Project::delete_now_used(
    ///     "project-42",
    ///     &session_valence,
    ///     use_!("Erase the project and its deletion DAG for the user request."),
    /// )
    /// .await?;
    /// assert!(Project::get_used("project-42", &session_valence, use_!("Confirm gone.")).await?.is_none());
    /// ```
    async fn delete_now(id: &str, valence: &Valence) -> Result<()> {
        crate::deletion::delete_entity_now(Self::table_name(), id, valence).await
    }

    /// Declared immediate delete: same as [`Self::delete_now`], with a catalog purpose.
    async fn delete_now_used(id: &str, valence: &Valence, purpose: DataUsePurpose) -> Result<()> {
        let _ = purpose;
        Self::delete_now(id, valence).await
    }

    /// Create or replace a row by explicit id.
    ///
    /// Privacy: when the row is absent, **create** policies apply; when it exists, **update**
    /// policies apply to both the existing row and the proposed payload (after an authorized read).
    #[deprecated(note = "use upsert_used with use_!(...) for declared data-use transparency")]
    async fn upsert(id: &str, data: Self, valence: &Valence) -> Result<Self>;

    /// Declared upsert: same as [`Self::upsert`], with a catalog purpose.
    async fn upsert_used(
        id: &str,
        data: Self,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Self> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::upsert(id, data, valence).await
    }

    /// Patch an existing row with a partial JSON object when the backend supports merge.
    #[deprecated(note = "use merge_used with use_!(...) for declared data-use transparency")]
    async fn merge(id: &str, patch: serde_json::Value, valence: &Valence) -> Result<Self>;

    /// Declared merge: same as [`Self::merge`], with a catalog purpose.
    async fn merge_used(
        id: &str,
        patch: serde_json::Value,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Self> {
        let _ = purpose;
        #[allow(deprecated)]
        Self::merge(id, patch, valence).await
    }
}

/// Field access direction for privacy checks.
#[derive(Debug, Clone, Copy)]
pub enum FieldOperation {
    /// Read path (get, list, query projection).
    Read,
    /// Write path (create, update, merge).
    Write,
}

/// Error returned when a privacy rule blocks field access.
#[derive(Debug, Clone)]
pub struct PrivacyError {
    /// Schema field name that failed the check.
    pub field: String,
    /// Whether the operation was a read or write.
    pub operation: FieldOperation,
    /// Human-readable denial reason.
    pub message: String,
}

impl std::fmt::Display for PrivacyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Privacy violation on field '{}' for {:?} operation: {}",
            self.field, self.operation, self.message
        )
    }
}

impl std::error::Error for PrivacyError {}

/// Compile-time schema metadata access for generated models (trait; struct is [`crate::schema::SchemaMetadata`]).
pub trait SchemaMetadata: Model {
    /// Static metadata type emitted by codegen.
    type SchemaMetadata;

    /// Return the process-global metadata instance for this model.
    fn schema_metadata() -> &'static Self::SchemaMetadata;

    /// Convenience accessor for instance callers.
    fn get_schema_metadata(&self) -> &'static Self::SchemaMetadata {
        Self::schema_metadata()
    }
}
