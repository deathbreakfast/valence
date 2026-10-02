//! Redis [`QueryCompiler`] for [`crate::query::QueryCore`].
//!
//! Emits parameterized SQL (same shape as [`super::SqlQueryCompiler`]) so the
//! Redis backend can full-scan then apply WHERE/ORDER/LIMIT in-process until
//! RediSearch pushdown lands.

use crate::compiled_query::CompiledQuery;
use crate::error::Result;
use crate::query::QueryCore;
use crate::query_compiler::QueryCompiler;

/// Compiles [`QueryCore`] for the Redis adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct RedisQueryCompiler;

impl QueryCompiler for RedisQueryCompiler {
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
    fn compiles_equality_where_to_sql() {
        let core = QueryCore::new("project".to_string()).where_string(
            "name".to_string(),
            StringPredicate::Equals("alpha".to_string()),
        );
        let cq = RedisQueryCompiler.compile(&core).expect("compile");
        assert!(cq.query_string.to_uppercase().contains(" WHERE "));
        assert_ne!(cq.params, Vec::<(String, serde_json::Value)>::new());
    }
}
