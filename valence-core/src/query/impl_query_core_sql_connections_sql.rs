impl QueryCore {
    fn connection_exists_clause_sql_dialect(
        from_field: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_sql()?;
        let sub_params_len = sub_params.len();
        let (renamed, params) = Self::rename_subquery_params(sub_query, sub_params, prefix);
        *param_counter += sub_params_len;
        let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
        // Outer row FK holds `tbl:id` (or a bare id) pointing at a matching row in `{tbl}`.
        Ok((
            format!(
                "({from_field} IN (SELECT id FROM {tbl} WHERE {where_sql}) \
                 OR {from_field} IN (SELECT '{tbl}:' || id FROM {tbl} WHERE {where_sql}))"
            ),
            params,
        ))
    }

    fn connection_exists_reverse_clause_sql_dialect(
        reverse_field: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
        outer_table: &str,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_sql()?;
        let sub_params_len = sub_params.len();
        let (renamed, params) = Self::rename_subquery_params(sub_query, sub_params, prefix);
        *param_counter += sub_params_len;
        let (tbl, where_sql) = parse_select_star_subquery_for_exists(&renamed)?;
        let alias = format!("_{prefix}_row");
        // Qualify bare column refs in the subquery WHERE onto the child alias.
        let where_sql = qualify_typed_where_for_alias(&where_sql, &alias);
        // Child rows in `{tbl}` reference the outer parent via `{reverse_field}` (`tbl:id` or bare id).
        Ok((
            format!(
                "EXISTS (SELECT 1 FROM {tbl} AS {alias} WHERE ({where_sql}) AND (\
                    {alias}.{reverse_field} = {outer_table}.id \
                    OR {alias}.{reverse_field} = ('{outer_table}:' || {outer_table}.id)\
                 ))"
            ),
            params,
        ))
    }

    fn connection_exists_m2m_clause_sql_dialect(
        edge_table: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_sql()?;
        let inner = sub_query
            .replacen("SELECT id, body", "SELECT id", 1)
            .replacen("SELECT *", "SELECT id", 1);
        let sub_params_len = sub_params.len();
        let (rewritten, params) = Self::rename_subquery_params(inner, sub_params, prefix);
        *param_counter += sub_params_len;
        Ok((
            format!(
                "id IN (SELECT from_id FROM valence_edges WHERE edge_type = '{edge_table}' \
                 AND to_id IN ({rewritten}))"
            ),
            params,
        ))
    }

    fn connection_contains_m2m_clause_sql_dialect(
        edge_table: &str,
        target: &crate::RecordId,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let tb_key = Self::next_param_key(param_counter);
        let id_key = Self::next_param_key(param_counter);
        let params = vec![
            (
                tb_key.clone(),
                serde_json::Value::String(target.table().to_string()),
            ),
            (
                id_key.clone(),
                serde_json::Value::String(target.id().to_string()),
            ),
        ];
        (
            format!(
                "id IN (SELECT from_id FROM valence_edges WHERE edge_type = '{edge_table}' \
                 AND to_table = ${tb_key} AND to_id = ${id_key})"
            ),
            params,
        )
    }

    fn hop_clause_sql_dialect(
        hop: &HopSource,
        outer_table: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        Self::hop_source_where_sql_dialect(hop, outer_table, param_counter)
    }

    fn hop_source_where_sql_dialect(
        hop: &HopSource,
        outer_table: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (source_sql, source_params) = hop.source_query.to_sql()?;
        let source_params_len = source_params.len();
        let prefix = format!("hop_{}", *param_counter);

        let (condition, params) = match &hop.hop_type {
            HopType::HasOneForward { fk_field } => {
                // Outer is target; source rows hold `{fk_field}` → outer.id.
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
                // Source FK holds `outer_table:id` (or a bare id) → match either shape.
                let sql = format!(
                    "(id IN (SELECT {fk_field} FROM {tbl} WHERE {where_sql}) \
                     OR ('{outer_table}:' || id) IN (SELECT {fk_field} FROM {tbl} WHERE {where_sql}))"
                );
                (sql, p)
            }
            HopType::HasManyForward { reverse_field } => {
                // Outer is child; source is filtered parent. Child FK must match parent id.
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
                let sql = format!(
                    "({reverse_field} IN (SELECT id FROM {tbl} WHERE {where_sql}) \
                     OR {reverse_field} IN (SELECT '{tbl}:' || id FROM {tbl} WHERE {where_sql}))"
                );
                (sql, p)
            }
            HopType::ManyToManyForward { edge_table } => {
                let inner = source_sql
                    .replacen("SELECT id, body", "SELECT id", 1)
                    .replacen("SELECT *", "SELECT id", 1);
                let (rewritten, p) =
                    Self::rename_subquery_params(inner, source_params, &prefix);
                let sql = format!(
                    "id IN (SELECT to_id FROM valence_edges WHERE edge_type = '{edge_table}' \
                     AND from_id IN ({rewritten}))"
                );
                (sql, p)
            }
        };
        *param_counter += source_params_len;
        Ok((condition, params))
    }
}

/// Best-effort qualify simple `col =` predicates with `{alias}.` for EXISTS subqueries.
fn qualify_typed_where_for_alias(where_sql: &str, alias: &str) -> String {
    // Legacy document path.
    where_sql
        .replace("json_extract(body,", &format!("json_extract({alias}.body,"))
        .replace("(body,", &format!("({alias}.body,"))
}
