impl QueryCore {
    /// Add a WHERE clause for an integer field
    #[must_use]
    pub fn where_int(mut self, field: String, predicate: IntPredicate) -> Self {
        self.where_clauses.push(WhereClause::Int(field, predicate));
        self
    }

    /// Add a WHERE clause for a string field
    #[must_use]
    pub fn where_string(mut self, field: String, predicate: StringPredicate) -> Self {
        self.where_clauses
            .push(WhereClause::String(field, predicate));
        self
    }

    /// Add a WHERE clause for a datetime field
    #[must_use]
    pub fn where_datetime(mut self, field: String, predicate: DateTimePredicate) -> Self {
        self.where_clauses
            .push(WhereClause::DateTime(field, predicate));
        self
    }

    /// Add a WHERE clause for a record field
    #[must_use]
    pub fn where_record(mut self, field: String, predicate: RecordPredicate) -> Self {
        self.where_clauses
            .push(WhereClause::Record(field, predicate));
        self
    }

    /// Add a WHERE clause for null/not-null check on optional fields
    #[must_use]
    pub fn where_null(mut self, field: String, predicate: NullPredicate) -> Self {
        self.where_clauses.push(WhereClause::Null(field, predicate));
        self
    }

    /// Add a correlated semi-join (SurrealQL `SELECT … LIMIT 1` with `$parent`, not SQL `EXISTS`).
    #[must_use]
    pub fn where_connection_exists(
        mut self,
        from_field: String,
        to_table: String,
        subquery: QueryCore,
    ) -> Self {
        self.where_clauses.push(WhereClause::ConnectionExists {
            from_field,
            to_table,
            subquery: Box::new(subquery),
        });
        self
    }

    /// Correlated semi-join for reverse/HasMany: `to_table` rows with `reverse_field = $parent.id`.
    #[must_use]
    pub fn where_connection_exists_reverse(
        mut self,
        to_table: String,
        reverse_field: String,
        subquery: QueryCore,
    ) -> Self {
        self.where_clauses
            .push(WhereClause::ConnectionExistsReverse {
                to_table,
                reverse_field,
                subquery: Box::new(subquery),
            });
        self
    }

    /// Semi-join filter for ManyToMany connections.
    #[must_use]
    pub fn where_connection_exists_many_to_many(
        mut self,
        edge_table: String,
        to_table: String,
        subquery: QueryCore,
    ) -> Self {
        self.where_clauses
            .push(WhereClause::ConnectionExistsManyToMany {
                edge_table,
                to_table,
                subquery: Box::new(subquery),
            });
        self
    }

    /// Add a direct edge-membership filter for ManyToMany connections.
    #[must_use]
    pub fn where_connection_contains_many_to_many(
        mut self,
        edge_table: String,
        target: crate::RecordId,
    ) -> Self {
        self.where_clauses
            .push(WhereClause::ConnectionContainsManyToMany { edge_table, target });
        self
    }
}
