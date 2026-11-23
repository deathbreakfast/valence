//! Privacy-checked model batches ([`ModelBatch`]) and the batch create hooks for codegen
//! `BatchCreatable` impls.

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::backend::WriteOp;
use crate::error::Result;
use crate::record_id::RecordId;
use crate::runtime::Valence;

/// Work a generated model queues to run once its row has committed.
type AfterCommit<'v> = Box<
    dyn FnOnce(serde_json::Value) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'v>>
        + Send
        + 'v,
>;

/// Several model writes applied all-or-nothing, each checked against its model's privacy
/// policies under the batch's actor.
///
/// Generated models add rows with `batch_create` and `batch_update`. Those run the same
/// privacy, required-field and unique-constraint checks as `Model::create` and
/// `Model::update` when the row is added, so a denied row fails before anything is written.
/// [`Self::commit`] then applies every write through [`Valence::apply_batch`] and, once the
/// batch has committed, runs each row's ownership hook and side effects in order.
///
/// Limits:
///
/// - Every table must route to one backend that supports transactions (see
///   [`Valence::apply_batch`]).
/// - Unique constraints are checked against stored rows only, not between rows of the same
///   batch.
/// - Ownership and side effects run after commit, as they do for `Model::create`; a failure
///   there doesn't roll the batch back.
///
/// # Examples
///
/// ```rust,ignore
/// use valence::{use_, ModelBatch};
///
/// let mut batch = ModelBatch::new(&session_valence);
/// JournalLine::batch_create(debit, &mut batch, use_!(r#"..."#)).await?;
/// JournalLine::batch_create(credit, &mut batch, use_!(r#"..."#)).await?;
/// Account::batch_update(account.id_str(), account, &mut batch, use_!(r#"..."#)).await?;
/// batch.commit().await?;
/// ```
pub struct ModelBatch<'v> {
    valence: &'v Valence,
    ops: Vec<WriteOp>,
    after: Vec<AfterCommit<'v>>,
}

impl<'v> ModelBatch<'v> {
    /// Start an empty batch that writes as `valence`'s actor.
    #[must_use]
    pub fn new(valence: &'v Valence) -> Self {
        Self {
            valence,
            ops: Vec::new(),
            after: Vec::new(),
        }
    }

    /// The handle (and so the actor) every row is checked and written under.
    #[must_use]
    pub fn valence(&self) -> &'v Valence {
        self.valence
    }

    /// Number of queued writes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether no writes are queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Queue an already-authorized write. Generated `batch_*` methods call this after their
    /// privacy checks; product code should use those instead.
    #[doc(hidden)]
    pub fn push_checked<F, Fut>(&mut self, op: WriteOp, after: F)
    where
        F: FnOnce(serde_json::Value) -> Fut + Send + 'v,
        Fut: Future<Output = Result<()>> + Send + 'v,
    {
        self.ops.push(op);
        self.after.push(Box::new(move |row| Box::pin(after(row))));
    }

    /// Apply every queued write as one unit, then run each row's post-commit hooks.
    ///
    /// Returns the stored rows in the order they were added.
    ///
    /// # Errors
    ///
    /// Any error from [`Valence::apply_batch`], in which case none of the writes are visible,
    /// or the first error a post-commit hook (ownership or deserializing the row) returns.
    pub async fn commit(self) -> Result<Vec<serde_json::Value>> {
        let rows = self.valence.apply_batch(self.ops).await?;
        for (hook, row) in self.after.into_iter().zip(rows.iter().cloned()) {
            hook(row).await?;
        }
        Ok(rows)
    }
}

/// Marker trait for models participating in batch create codegen.
#[async_trait]
pub trait BatchCreatable: Serialize + DeserializeOwned + Send + 'static {
    fn table_name() -> &'static str;

    fn set_id(&mut self, id: RecordId);

    /// Checks a row before the host inserts it.
    ///
    /// The host's batch runtime must call this on every row it serializes. Generated
    /// models override it to refuse a row that leaves out a required field whose
    /// `read` policy can hide it, with the same `<table>.<field> is required`
    /// validation error `Model::create` returns.
    ///
    /// # Errors
    ///
    /// [`Error::Validation`](crate::error::Error::Validation) when a required field is
    /// missing or null.
    fn validate_batch_row(_row: &serde_json::Value) -> Result<()> {
        Ok(())
    }

    async fn ensure_ownership_after_batch_create(
        _created_row: serde_json::Value,
        _valence: &Valence,
    ) -> Result<()> {
        Ok(())
    }
}
