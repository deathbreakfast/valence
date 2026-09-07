impl QueryCore {
    fn clause_to_condition(
        clause: &WhereClause,
        table: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        match clause {
            WhereClause::Int(field, pred) => Ok(Self::int_clause_sql(field, pred, param_counter)),
            WhereClause::String(field, pred) => {
                Ok(Self::string_clause_sql(field, pred, table, param_counter))
            }
            WhereClause::DateTime(field, pred) => {
                Ok(Self::datetime_clause_sql(field, pred, param_counter))
            }
            WhereClause::Record(field, pred) => Ok(Self::record_clause_sql(field, pred, param_counter)),
            WhereClause::Null(field, pred) => Ok((Self::null_clause_sql(field, *pred), Vec::new())),
            WhereClause::ConnectionExists {
                from_field,
                subquery,
                ..
            } => Self::connection_exists_clause_sql(from_field, subquery, "ce", param_counter),
            WhereClause::ConnectionExistsReverse {
                reverse_field,
                subquery,
                ..
            } => Self::connection_exists_reverse_clause_sql(
                reverse_field,
                subquery,
                "cer",
                param_counter,
            ),
            WhereClause::ConnectionExistsManyToMany {
                edge_table,
                subquery,
                ..
            } => {
                crate::safe_ident::assert_registered_edge_ident(edge_table)?;
                Self::connection_exists_m2m_clause_sql(edge_table, subquery, "cemtm", param_counter)
            }
            WhereClause::ConnectionContainsManyToMany { edge_table, target } => {
                crate::safe_ident::assert_registered_edge_ident(edge_table)?;
                Ok(Self::connection_contains_m2m_clause_sql(
                    edge_table,
                    target,
                    param_counter,
                ))
            }
            WhereClause::Hop(hop) => Self::hop_clause_sql(hop, param_counter),
        }
    }

    /// Convert a slice of WHERE clauses into SQL condition strings and params.
    fn clauses_to_conditions(
        clauses: &[WhereClause],
        table: &str,
        param_counter: &mut usize,
    ) -> Result<(Vec<String>, Vec<(String, serde_json::Value)>)> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        for clause in clauses {
            let (condition, mut clause_params) =
                Self::clause_to_condition(clause, table, param_counter)?;
            params.append(&mut clause_params);
            conditions.push(condition);
        }

        Ok((conditions, params))
    }
}
