impl QueryCore {
    /// Add an ORDER BY clause
    #[must_use]
    pub fn order_by(mut self, field: String, direction: SortDirection) -> Self {
        self.order_by.push(OrderBy { field, direction });
        self
    }

    /// Add a GROUP BY field
    #[must_use]
    pub fn group_by(mut self, field: String) -> Self {
        self.group_by.push(field);
        self
    }

    /// Set the LIMIT
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the OFFSET
    #[must_use]
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Set search fields for full-text search
    #[must_use]
    pub fn set_search_fields(mut self, fields: Vec<String>) -> Self {
        self.search_fields = fields;
        self
    }

    /// Add a search term (expands to OR clause across search_fields)
    #[must_use]
    pub fn search(mut self, term: String) -> Self {
        self.search_term = Some(term);
        self
    }
}
