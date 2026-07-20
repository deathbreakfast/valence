impl QueryCore {
    /// Create a new query core for a table
    #[must_use]
    pub fn new(table: String) -> Self {
        Self {
            table,
            model_type: None,
            projection: None,
            where_clauses: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            search_fields: Vec::new(),
            search_term: None,
            hop_source: None,
            or_groups: Vec::new(),
            group_by: Vec::new(),
        }
    }

    /// Create a QueryCore with model type information
    #[must_use]
    pub fn with_model_type(table: impl Into<String>, model_type: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            model_type: Some(model_type.into()),
            projection: None,
            where_clauses: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            search_fields: Vec::new(),
            search_term: None,
            hop_source: None,
            or_groups: Vec::new(),
            group_by: Vec::new(),
        }
    }
}
