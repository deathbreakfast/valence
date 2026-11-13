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

    /// Set the LIMIT (clamped to [`super::MAX_QUERY_LIMIT`]).
    #[must_use]
    pub fn limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit.min(super::MAX_QUERY_LIMIT));
        self
    }

    /// Set the OFFSET (clamped to [`super::MAX_QUERY_OFFSET`]).
    #[must_use]
    pub fn offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset.min(super::MAX_QUERY_OFFSET));
        self
    }

    /// Start the backend scan at raw row `position`, taken from the previous page's
    /// [`QueryStats::resume`] under the same filters and order.
    ///
    /// `offset` counts rows the caller can read, so paging by offset re-reads every row
    /// before the window on each call. Resuming starts where the last page stopped, so a
    /// page re-reads at most the rest of one backend window. Rows created
    /// between pages can shift raw positions by a few rows, as with offset paging, so a
    /// caller that must not show a row twice should dedupe by id.
    #[must_use]
    pub const fn resume_at(mut self, position: u32) -> Self {
        self.resume_at = Some(position);
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
