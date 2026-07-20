impl QueryCore {
    /// Convert the query to parameterized SQL (SQLite/Postgres/mem SQL subset).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub(crate) fn to_sql(&self) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let select_clause = super::sql_document::sql_select_clause(self.projection.as_ref());
        let mut query = format!("SELECT {select_clause} FROM {}", self.table);
        let mut params = Vec::new();
        let mut param_counter = 0;
        let mut where_parts = Vec::new();

        if let Some(ref hop) = self.hop_source {
            let (sql, hop_params) = Self::hop_source_where_sql_dialect(hop, &mut param_counter)?;
            params.extend(hop_params);
            where_parts.push(sql);
        }

        self.append_search_where_sql(&mut where_parts, &mut params, &mut param_counter);
        self.append_or_group_where_sql(&mut where_parts, &mut params, &mut param_counter)?;
        self.append_flat_where_sql(&mut where_parts, &mut params, &mut param_counter)?;

        if !where_parts.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_parts.join(" AND "));
        }

        self.append_group_order_limit_sql(&mut query);
        Ok((query, params))
    }

    fn append_or_group_where_sql(
        &self,
        where_parts: &mut Vec<String>,
        params: &mut Vec<(String, serde_json::Value)>,
        param_counter: &mut usize,
    ) -> Result<()> {
        if self.or_groups.is_empty() {
            return Ok(());
        }
        let mut group_sqls = Vec::new();
        for group in &self.or_groups {
            let (conditions, group_params) =
                Self::clauses_to_conditions_sql(group, &self.table, param_counter)?;
            params.extend(group_params);
            if conditions.is_empty() {
                continue;
            }
            let joined = conditions.join(" AND ");
            if conditions.len() > 1 {
                group_sqls.push(format!("({joined})"));
            } else {
                group_sqls.push(joined);
            }
        }
        if !group_sqls.is_empty() {
            if group_sqls.len() == 1 {
                if let Some(group_sql) = group_sqls.into_iter().next() {
                    where_parts.push(group_sql);
                }
            } else {
                where_parts.push(format!("({})", group_sqls.join(" OR ")));
            }
        }
        Ok(())
    }

    fn append_search_where_sql(
        &self,
        where_parts: &mut Vec<String>,
        params: &mut Vec<(String, serde_json::Value)>,
        param_counter: &mut usize,
    ) {
        let Some(term) = self.search_term.as_ref() else {
            return;
        };
        if self.search_fields.is_empty() {
            return;
        }
        let search_conditions: Vec<String> = self
            .search_fields
            .iter()
            .map(|field| {
                let param_key = Self::next_param_key(param_counter);
                params.push((
                    param_key.clone(),
                    serde_json::Value::String(format!("%{term}%")),
                ));
                format!(
                    "{} LIKE ${param_key}",
                    super::sql_document::sql_doc_column(field)
                )
            })
            .collect();
        where_parts.push(format!("({})", search_conditions.join(" OR ")));
    }

    fn append_flat_where_sql(
        &self,
        where_parts: &mut Vec<String>,
        params: &mut Vec<(String, serde_json::Value)>,
        param_counter: &mut usize,
    ) -> Result<()> {
        if self.where_clauses.is_empty() {
            return Ok(());
        }
        let (conditions, clause_params) =
            Self::clauses_to_conditions_sql(&self.where_clauses, &self.table, param_counter)?;
        params.extend(clause_params);
        where_parts.extend(conditions);
        Ok(())
    }

    fn append_group_order_limit_sql(&self, query: &mut String) {
        if !self.group_by.is_empty() {
            query.push_str(" GROUP BY ");
            query.push_str(
                &self
                    .group_by
                    .iter()
                    .map(|f| super::sql_document::sql_doc_column(f))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !self.order_by.is_empty() {
            query.push_str(" ORDER BY ");
            let order_parts: Vec<String> = self
                .order_by
                .iter()
                .map(|ob| {
                    let dir = match ob.direction {
                        SortDirection::Asc => "ASC",
                        SortDirection::Desc => "DESC",
                    };
                    format!(
                        "{} {}",
                        super::sql_document::sql_doc_column(&ob.field),
                        dir
                    )
                })
                .collect();
            query.push_str(&order_parts.join(", "));
        }
        if let Some(limit) = self.limit {
            query.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = self.offset {
            query.push_str(&format!(" OFFSET {offset}"));
        }
    }
}
