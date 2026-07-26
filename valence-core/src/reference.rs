//! Reference placeholders for batch entity creation.

use std::sync::{Arc, Mutex};

use crate::RecordId;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct Reference<T> {
    id: String,
    resolved: Arc<Mutex<Option<RecordId>>>,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Reference<T> {
    pub fn new() -> Self {
        Self {
            id: format!("ref_{}", Uuid::new_v4()),
            resolved: Arc::new(Mutex::new(None)),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn resolve(&self) -> Option<RecordId> {
        self.resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Store the resolved record id for this placeholder reference.
    pub fn resolve_to(&self, id: RecordId) {
        *self
            .resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(id);
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}

impl<T> Default for Reference<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait WithReference: Sized {
    fn with_reference(self, reference: Reference<Self>) -> ReferencedEntity<Self> {
        ReferencedEntity {
            reference,
            entity: self,
        }
    }
}

pub struct ReferencedEntity<T> {
    pub(crate) reference: Reference<T>,
    pub(crate) entity: T,
}

impl<T> ReferencedEntity<T> {
    pub fn new(reference: Reference<T>, entity: T) -> Self {
        Self { reference, entity }
    }

    pub fn reference(&self) -> &Reference<T> {
        &self.reference
    }

    pub fn entity(&self) -> &T {
        &self.entity
    }

    pub fn into_entity(self) -> T {
        self.entity
    }
}
