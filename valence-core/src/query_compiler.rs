//! Query compilation from [`crate::query::QueryCore`] to [`crate::compiled_query::CompiledQuery`].

use crate::compiled_query::CompiledQuery;
use crate::error::Result;
use crate::query::QueryCore;

/// Compiles [`QueryCore`] for one database dialect.
pub trait QueryCompiler: Send + Sync {
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    fn compile(&self, core: &QueryCore) -> Result<CompiledQuery>;
}
