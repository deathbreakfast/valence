//! Model contracts generated from schema DSL.
//!
//! Generated model types implement [`Model`] via `valence-codegen`. See the
//! `valence-codegen` crate README and `examples/codegen-host` for the build pipeline.

use crate::error::Result;
use crate::runtime::Valence;
use async_trait::async_trait;

/// Core trait that all generated models implement.
///
/// CRUD methods route through the active [`Valence`] backend, applying privacy and ownership
/// hooks defined in the source schema.
///
/// # Examples
///
/// Generated models (from `valence-codegen`) implement this trait. After including
/// `$OUT_DIR/generated_models.rs`:
///
/// ```ignore
/// use valence::Model;
///
/// let created = Widget::create(widget, &valence).await?;
/// let loaded = Widget::get(created.id(), &valence).await?;
/// Widget::update(created.id(), updated, &valence).await?;
/// Widget::delete(created.id(), &valence).await?;
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

    /// Fetch one row by primary key; returns `Ok(None)` when absent.
    async fn get(id: &str, valence: &Valence) -> Result<Option<Self>>;
    /// Insert a new row.
    async fn create(data: Self, valence: &Valence) -> Result<Self>;
    /// Replace an existing row by id.
    async fn update(id: &str, data: Self, valence: &Valence) -> Result<Self>;
    /// Queue a durable deletion run (or hard-delete for deletion-skip platform tables).
    async fn delete(id: &str, valence: &Valence) -> Result<()>;
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
    /// use valence::Model;
    ///
    /// Project::delete_now("project-42", &session_valence).await?;
    /// assert!(Project::get("project-42", &session_valence).await?.is_none());
    /// ```
    async fn delete_now(id: &str, valence: &Valence) -> Result<()> {
        crate::deletion::delete_entity_now(Self::table_name(), id, valence).await
    }
    /// Create or replace a row by explicit id.
    ///
    /// Privacy: when the row is absent, **create** policies apply; when it exists, **update**
    /// policies apply to both the existing row and the proposed payload (after an authorized read).
    async fn upsert(id: &str, data: Self, valence: &Valence) -> Result<Self>;
    /// Patch an existing row with a partial JSON object when the backend supports merge.
    async fn merge(id: &str, patch: serde_json::Value, valence: &Valence) -> Result<Self>;
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
