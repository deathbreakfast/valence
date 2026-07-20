impl QueryCore {
    /// OR-combine another query's WHERE conditions with this one.
    #[must_use]
    pub fn union_with(mut self, other: QueryCore) -> Self {
        if other.where_clauses.is_empty()
            && other.or_groups.is_empty()
            && other.hop_source.is_none()
        {
            return self;
        }

        if let Some(hop) = self.hop_source.take() {
            self.where_clauses.insert(0, WhereClause::Hop(hop));
        }

        let mut other_clauses = other.where_clauses;
        if let Some(hop) = other.hop_source {
            other_clauses.insert(0, WhereClause::Hop(hop));
        }

        if self.or_groups.is_empty() {
            let current = std::mem::take(&mut self.where_clauses);
            if !current.is_empty() {
                self.or_groups.push(current);
            }
        }

        if !other_clauses.is_empty() {
            self.or_groups.push(other_clauses);
        }
        self.or_groups.extend(other.or_groups);
        self
    }

    /// AND-combine another query's WHERE conditions with this one.
    #[must_use]
    pub fn join_with(mut self, other: QueryCore) -> Self {
        if let Some(hop) = other.hop_source {
            self.where_clauses.push(WhereClause::Hop(hop));
        }
        self.where_clauses.extend(other.where_clauses);
        self
    }
}
