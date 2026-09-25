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
/// Every CRUD entry point takes a [`DataUsePurpose`] (pass `use_!(...)`) so declared data
/// uses appear in the transparency catalog. There is no purpose-free public overload.
///
/// # Examples
///
/// Generated models (from `valence-codegen`) implement this trait. After including
/// `$OUT_DIR/generated_models.rs`:
///
/// ```ignore
/// use valence::{use_, Model};
///
/// let created = Widget::create(widget, &valence, use_!(r#"In the **Model trait demo**, we **create a demo widget** so later reload and update examples have a generated row to work with. Developers reading the crate docs use this example."#)).await?;
/// let loaded = Widget::get(created.id(), &valence, use_!(r#"After create in the **Model trait demo**, we **reload the widget by id** so readers can confirm the declared get path returned the row. Developers reading the crate docs use this result."#)).await?;
/// Widget::update(created.id(), updated, &valence, use_!(r#"In the **Model trait demo**, we **replace the widget with updated fields** so readers can see a full-row update on a generated model. Developers reading the crate docs use this result."#)).await?;
/// Widget::delete(created.id(), &valence, use_!(r#"At the end of the **Model trait demo**, we **queue widget deletion** so readers can see how declared delete starts durable removal. Developers reading the crate docs use this result."#)).await?;
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
    ///
    /// Pass `use_!(...)` so this read appears in the transparency catalog.
    async fn get(id: &str, valence: &Valence, purpose: DataUsePurpose) -> Result<Option<Self>>;

    /// Insert a new row.
    ///
    /// Pass `use_!(...)` so this create appears in the transparency catalog.
    async fn create(data: Self, valence: &Valence, purpose: DataUsePurpose) -> Result<Self>;

    /// Replace an existing row by id.
    ///
    /// Pass `use_!(...)` so this update appears in the transparency catalog.
    async fn update(
        id: &str,
        data: Self,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Self>;

    /// Queue a durable deletion run (or hard-delete for deletion-skip platform tables).
    ///
    /// Pass `use_!(...)` so this delete appears in the transparency catalog.
    async fn delete(id: &str, valence: &Valence, purpose: DataUsePurpose) -> Result<()>;

    /// Physically delete this row and its deletion DAG in the current future.
    ///
    /// Authorizes the full DAG under the requesting actor, then applies every node
    /// before returning. Missing rows succeed. A root already owned by a queued
    /// deletion returns [`crate::Error::PendingDeletion`].
    ///
    /// Intentionally unbounded: use only for bounded request workloads. Prefer
    /// [`Self::delete`] for large or retry-heavy graphs.
    ///
    /// Pass `use_!(...)` so this delete appears in the transparency catalog.
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
    /// Project::delete_now(
    ///     "project-42",
    ///     &session_valence,
    ///     use_!(r#"When a user asks to **remove a project**, we **erase that project and its deletion graph immediately** so related rows are gone in the same request. The signed-in operator who requested removal uses this outcome."#),
    /// )
    /// .await?;
    /// assert!(Project::get("project-42", &session_valence, use_!(r#"After immediate deletion, we **load the project by id again** so we can confirm the row is gone before continuing. The same request path uses this check only."#)).await?.is_none());
    /// ```
    async fn delete_now(id: &str, valence: &Valence, purpose: DataUsePurpose) -> Result<()> {
        let _ = purpose;
        crate::deletion::delete_entity_now(Self::table_name(), id, valence).await
    }

    /// Create or replace a row by explicit id.
    ///
    /// Privacy: when the row is absent, **create** policies apply; when it exists, **update**
    /// policies apply to both the existing row and the proposed payload (after an authorized read).
    ///
    /// Pass `use_!(...)` so this upsert appears in the transparency catalog.
    async fn upsert(
        id: &str,
        data: Self,
        valence: &Valence,
        purpose: DataUsePurpose,
    ) -> Result<Self>;
}

/// Field access direction for privacy checks.
#[derive(Debug, Clone, Copy)]
pub enum FieldOperation {
    /// Read path (get, list, query projection).
    Read,
    /// Write path (create, update).
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
