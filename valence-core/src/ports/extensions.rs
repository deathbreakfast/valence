//! Typed slot for host ports that only privacy evaluators read.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Ports keyed by their concrete type; cheap to clone along with [`crate::Valence`].
///
/// Fill it with [`crate::ValenceBuilder::extension`], read it with [`crate::Valence::extension`].
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use valence_core::PortExtensions;
///
/// let mut ports = PortExtensions::default();
/// ports.insert(Arc::new(7_u32));
/// assert_eq!(*ports.get::<u32>().expect("registered"), 7);
/// assert!(ports.get::<u64>().is_none());
/// ```
#[derive(Clone, Default)]
pub struct PortExtensions {
    ports: Arc<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl PortExtensions {
    /// Register `port` under `T`, replacing any earlier port of that type.
    pub fn insert<T: Send + Sync + 'static>(&mut self, port: Arc<T>) {
        Arc::make_mut(&mut self.ports).insert(TypeId::of::<T>(), port);
    }

    /// The port registered for `T`, if any.
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let port = Arc::clone(self.ports.get(&TypeId::of::<T>())?);
        port.downcast::<T>().ok()
    }
}

impl std::fmt::Debug for PortExtensions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortExtensions")
            .field("len", &self.ports.len())
            .finish()
    }
}
