//! Side effect types for post-mutation hooks.

use crate::model::Model;
use crate::runtime::Valence;
use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone)]
pub struct FieldChange<T> {
    before: Option<T>,
    after: Option<T>,
}

impl<T> FieldChange<T> {
    pub fn new(before: Option<T>, after: Option<T>) -> Self {
        Self { before, after }
    }

    pub fn before(&self) -> Option<&T> {
        self.before.as_ref()
    }

    pub fn after(&self) -> Option<&T> {
        self.after.as_ref()
    }
}

impl<T: PartialEq> FieldChange<T> {
    pub fn has_changed(&self) -> bool {
        self.before != self.after
    }
}

pub struct Mutation<'v, T: Model> {
    kind: MutationKind,
    before: Option<T>,
    after: Option<T>,
    fields: T::FieldChanges,
    valence: &'v Valence,
}

impl<'v, T: Model> Mutation<'v, T> {
    pub fn new(
        kind: MutationKind,
        before: Option<T>,
        after: Option<T>,
        fields: T::FieldChanges,
        valence: &'v Valence,
    ) -> Self {
        Self {
            kind,
            before,
            after,
            fields,
            valence,
        }
    }

    pub fn kind(&self) -> &MutationKind {
        &self.kind
    }

    pub fn before(&self) -> Option<&T> {
        self.before.as_ref()
    }

    pub fn after(&self) -> Option<&T> {
        self.after.as_ref()
    }

    pub fn fields(&self) -> &T::FieldChanges {
        &self.fields
    }

    pub fn valence(&self) -> &Valence {
        self.valence
    }
}

#[async_trait]
pub trait SideEffect<T: Model>: Send + Sync {
    async fn on_mutation(&self, mutation: &Mutation<'_, T>) -> crate::error::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_change_detects_updates() {
        let change = FieldChange::new(Some(10i64), Some(20i64));
        assert!(change.has_changed());
    }
}
