impl QueryCore {
    fn connection_exists_clause_sql(
        from_field: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_surrealql()?;
        let sub_params_len = sub_params.len();
        let (renamed, params) = Self::rename_subquery_params(sub_query, sub_params, prefix);
        *param_counter += sub_params_len;
        let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
        let parent_fk = format!("$parent.{from_field}");
        let rhs = surreal_type_record_from_colon_strand(&parent_fk);
        Ok((
            format!(
                "(SELECT id FROM {tbl} WHERE ({where_sql}) AND (id = {parent_fk} OR id = {rhs}) LIMIT 1)"
            ),
            params,
        ))
    }

    fn connection_exists_reverse_clause_sql(
        reverse_field: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_surrealql()?;
        let sub_params_len = sub_params.len();
        let (renamed, params) = Self::rename_subquery_params(sub_query, sub_params, prefix);
        *param_counter += sub_params_len;
        let (tbl, where_sql) = parse_select_star_subquery_for_exists(&renamed)?;
        let lhs = surreal_type_record_from_colon_strand(reverse_field);
        Ok((
            format!(
                "(SELECT id FROM {tbl} WHERE ({where_sql}) AND ({reverse_field} = $parent.id OR {lhs} = $parent.id) LIMIT 1)"
            ),
            params,
        ))
    }

    fn connection_exists_m2m_clause_sql(
        edge_table: &str,
        subquery: &QueryCore,
        prefix: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (sub_query, sub_params) = subquery.to_surrealql()?;
        let inner_target = sub_query.replacen("SELECT *", "SELECT VALUE id", 1);
        let sub_params_len = sub_params.len();
        let (rewritten, params) = Self::rename_subquery_params(inner_target, sub_params, prefix);
        *param_counter += sub_params_len;
        Ok((
            format!(
                "id IN (SELECT VALUE `in` FROM {edge_table} WHERE `out` IN ({rewritten}))"
            ),
            params,
        ))
    }

    fn connection_contains_m2m_clause_sql(
        edge_table: &str,
        target: &crate::RecordId,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let tb_key = Self::next_param_key(param_counter);
        let id_key = Self::next_param_key(param_counter);
        let params = vec![
            (tb_key.clone(), serde_json::Value::String(target.table().to_string())),
            (id_key.clone(), serde_json::Value::String(target.id().to_string())),
        ];
        (
            format!(
                "id IN (SELECT VALUE `in` FROM {edge_table} WHERE `out` = type::record(${tb_key}, ${id_key}))"
            ),
            params,
        )
    }

    fn hop_clause_sql(
        hop: &HopSource,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (source_sql, source_params) = hop.source_query.to_surrealql()?;
        let source_params_len = source_params.len();
        let prefix = format!("hop_{}", *param_counter);

        let (condition, params) = match &hop.hop_type {
            HopType::HasOneForward { fk_field } => {
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let sql = hasone_forward_exists_sql(&renamed, fk_field, "$parent.id")?;
                (sql, p)
            }
            HopType::HasManyForward { reverse_field } => {
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
                let parent_fk = format!("$parent.{reverse_field}");
                let rhs = surreal_type_record_from_colon_strand(&parent_fk);
                let sql = format!(
                    "(SELECT id FROM {tbl} WHERE ({where_sql}) AND (id = {parent_fk} OR id = {rhs}) LIMIT 1)"
                );
                (sql, p)
            }
            HopType::ManyToManyForward { edge_table } => {
                let inner = source_sql.replacen("SELECT *", "SELECT VALUE id", 1);
                let (rewritten, p) =
                    Self::rename_subquery_params(inner, source_params, &prefix);
                let sql = format!(
                    "id IN (SELECT VALUE `out` FROM {edge_table} WHERE `in` IN ({rewritten}))"
                );
                (sql, p)
            }
        };
        *param_counter += source_params_len;
        Ok((condition, params))
    }

    fn hop_source_where_sql(
        hop: &HopSource,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        let (source_sql, source_params) = hop.source_query.to_surrealql()?;
        let source_params_len = source_params.len();
        let prefix = format!("hop_{}", *param_counter);

        let (sql, params) = match &hop.hop_type {
            HopType::HasOneForward { fk_field } => {
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let sql = hasone_forward_exists_sql(&renamed, fk_field, "$parent.id")?;
                (sql, p)
            }
            HopType::HasManyForward { reverse_field } => {
                let (renamed, p) =
                    Self::rename_subquery_params(source_sql, source_params, &prefix);
                let (tbl, where_sql) = parse_select_from_subquery(&renamed)?;
                let parent_fk = format!("$parent.{reverse_field}");
                let rhs = surreal_type_record_from_colon_strand(&parent_fk);
                let sql = format!(
                    "(SELECT id FROM {tbl} WHERE ({where_sql}) AND (id = {parent_fk} OR id = {rhs}) LIMIT 1)"
                );
                (sql, p)
            }
            HopType::ManyToManyForward { edge_table } => {
                let inner = source_sql.replacen("SELECT *", "SELECT VALUE id", 1);
                let (rewritten, p) =
                    Self::rename_subquery_params(inner, source_params, &prefix);
                let sql = format!(
                    "id IN (SELECT VALUE `out` FROM {edge_table} WHERE `in` IN ({rewritten}))"
                );
                (sql, p)
            }
        };
        *param_counter += source_params_len;
        Ok((sql, params))
    }
}
