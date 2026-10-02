//! Process-wide catalog of every declared data use in a deployment.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::DataUse;

static INSTALLED: OnceLock<DataUseCatalog> = OnceLock::new();

/// Every declared data use a deployment makes, built at compile time by the host
/// and installed once at boot.
///
/// A host build script generates a `data_use_catalog()` function with
/// `uf-valence-data-use-scan`, then calls [`DataUseCatalog::install`] before it
/// starts serving. Readers such as the Valence ops UI call
/// [`DataUseCatalog::global`] and treat `None` as a host wiring defect, never as
/// "no declared uses".
///
/// # Examples
///
/// ```
/// use valence_core::data_use::{DataOp, DataUse, DataUseCatalog, DataUseTarget};
///
/// let catalog = DataUseCatalog::from_entries(vec![DataUse {
///     purpose: "We **load your account** to sign you in.".into(),
///     file: "counter-app-worker/src/session.rs".into(),
///     line: 12,
///     crate_name: "counter-app-worker".into(),
///     repository: "https://github.com/unified-field-dev/counter-app".into(),
///     target: DataUseTarget::Schema("user".into()),
///     op: DataOp::Read,
///     method: "get".into(),
///     connection: None,
///     referenced_schema: None,
///     via_trait: None,
/// }]);
/// assert_eq!(catalog.len(), 1);
/// assert!(catalog.crate_names().contains("counter-app-worker"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataUseCatalog {
    entries: Vec<DataUse>,
}

/// Failure installing the process-wide [`DataUseCatalog`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogInstallError {
    /// A catalog was already installed in this process. The first catalog stays
    /// in place; the rejected one is dropped.
    #[error(
        "data-use catalog already installed ({existing_rows} rows); rejected second catalog with {rejected_rows} rows"
    )]
    AlreadyInstalled {
        /// Row count of the catalog that stays installed.
        existing_rows: usize,
        /// Row count of the catalog that was rejected.
        rejected_rows: usize,
    },
}

impl DataUseCatalog {
    /// Build a catalog from scanned rows, keeping their order.
    #[must_use]
    pub fn from_entries(entries: Vec<DataUse>) -> Self {
        Self { entries }
    }

    /// Install this catalog for the whole process and return the installed copy.
    ///
    /// Call once at host boot, before serving requests, so every reader sees the
    /// same deployment catalog.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogInstallError::AlreadyInstalled`] when a catalog is already
    /// installed. The first catalog keeps serving; a second install is a host
    /// wiring bug (two boot paths), not something to retry.
    pub fn install(self) -> Result<&'static Self, CatalogInstallError> {
        let mut candidate = Some(self);
        let installed = INSTALLED.get_or_init(|| candidate.take().unwrap_or_default());
        match candidate {
            None => Ok(installed),
            Some(rejected) => Err(CatalogInstallError::AlreadyInstalled {
                existing_rows: installed.len(),
                rejected_rows: rejected.len(),
            }),
        }
    }

    /// The catalog installed by [`DataUseCatalog::install`], or `None` when the
    /// host never installed one.
    #[must_use]
    pub fn global() -> Option<&'static Self> {
        INSTALLED.get()
    }

    /// Every declared use, in scan order.
    #[must_use]
    pub fn entries(&self) -> &[DataUse] {
        &self.entries
    }

    /// Number of declared uses.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the catalog has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Distinct Cargo package names that declared at least one use.
    ///
    /// Host tests use this to assert every deployed component made it into the
    /// catalog.
    #[must_use]
    pub fn crate_names(&self) -> BTreeSet<&str> {
        self.entries.iter().map(|e| e.crate_name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_use::{DataOp, DataUseTarget};

    fn row(crate_name: &str, line: u32) -> DataUse {
        DataUse {
            purpose: format!("purpose {line}"),
            file: "src/lib.rs".into(),
            line,
            crate_name: crate_name.into(),
            repository: String::new(),
            target: DataUseTarget::Unscoped,
            op: DataOp::Read,
            method: "execute".into(),
            connection: None,
            referenced_schema: None,
            via_trait: None,
        }
    }

    #[test]
    fn from_entries_preserves_order_and_len() {
        let catalog = DataUseCatalog::from_entries(vec![row("b", 2), row("a", 1)]);
        assert_eq!(catalog.len(), 2);
        assert!(!catalog.is_empty());
        let lines: Vec<u32> = catalog.entries().iter().map(|e| e.line).collect();
        assert_eq!(lines, vec![2, 1]);
        assert!(DataUseCatalog::default().is_empty());
    }

    #[test]
    fn crate_names_dedupes() {
        let catalog = DataUseCatalog::from_entries(vec![
            row("worker", 1),
            row("server", 2),
            row("worker", 3),
        ]);
        let names: Vec<&str> = catalog.crate_names().into_iter().collect();
        assert_eq!(names, vec!["server", "worker"]);
    }
}
