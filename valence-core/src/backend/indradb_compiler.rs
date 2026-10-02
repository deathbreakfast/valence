//! IndraDB query [`QueryCompiler`] for [`crate::query::QueryCore`].

use crate::compiled_query::CompiledQuery;
use crate::error::Result;
use crate::query::QueryCore;
use crate::query_compiler::QueryCompiler;

/// Compiles [`QueryCore`] to parameterized SQL (document-row subset) for IndraDB.
///
/// IndraDB applies [`crate::query::apply_equality_where`] / order-limit in the adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct IndraQueryCompiler;

impl QueryCompiler for IndraQueryCompiler {
    fn compile(&self, core: &QueryCore) -> Result<CompiledQuery> {
        let (query_string, params) = core.to_sql()?;
        Ok(CompiledQuery::new(query_string, params))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{QueryCore, StringPredicate};

    #[test]
    fn compiles_filtered_select() {
        let core = QueryCore::new("project".to_string())
            .where_string("name".into(), StringPredicate::Equals("alpha".into()));
        let cq = IndraQueryCompiler.compile(&core).expect("compile");
        assert!(cq.query_string.contains("WHERE"));
        assert_ne!(cq.params, Vec::<(String, serde_json::Value)>::new());
    }
}
