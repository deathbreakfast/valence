//! Parameterized statement for [`crate::backend::DatabaseBackend::execute_compiled_query`].

#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub query_string: String,
    pub params: Vec<(String, serde_json::Value)>,
}

impl CompiledQuery {
    #[must_use]
    pub fn new(query_string: String, params: Vec<(String, serde_json::Value)>) -> Self {
        Self {
            query_string,
            params,
        }
    }
}
