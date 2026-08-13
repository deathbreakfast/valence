/// Escape `\`, `%`, and `_` so user literals are not LIKE wildcards.
fn escape_like_pattern(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

impl QueryCore {
    fn clause_to_condition_sql(
        clause: &WhereClause,
        table: &str,
        param_counter: &mut usize,
    ) -> Result<(String, Vec<(String, serde_json::Value)>)> {
        match clause {
            WhereClause::Int(field, pred) => {
                Ok(Self::int_clause_sql_doc(field, pred, param_counter))
            }
            WhereClause::String(field, pred) => {
                Ok(Self::string_clause_sql_sql(field, pred, table, param_counter))
            }
            WhereClause::DateTime(field, pred) => {
                Ok(Self::datetime_clause_sql_doc(field, pred, param_counter))
            }
            WhereClause::Record(field, pred) => {
                Ok(Self::record_clause_sql_sql(field, pred, param_counter))
            }
            WhereClause::Null(field, pred) => Ok((
                Self::null_clause_sql_doc(field, *pred),
                Vec::new(),
            )),
            WhereClause::ConnectionExists {
                from_field,
                subquery,
                ..
            } => Self::connection_exists_clause_sql_dialect(
                from_field,
                subquery,
                "ce",
                param_counter,
            ),
            WhereClause::ConnectionExistsReverse {
                reverse_field,
                subquery,
                ..
            } => Self::connection_exists_reverse_clause_sql_dialect(
                reverse_field,
                subquery,
                "cer",
                param_counter,
                table,
            ),
            WhereClause::ConnectionExistsManyToMany {
                edge_table,
                subquery,
                ..
            } => Self::connection_exists_m2m_clause_sql_dialect(
                edge_table,
                subquery,
                "cemtm",
                param_counter,
            ),
            WhereClause::ConnectionContainsManyToMany { edge_table, target } => Ok(
                Self::connection_contains_m2m_clause_sql_dialect(
                    edge_table,
                    target,
                    param_counter,
                ),
            ),
            WhereClause::Hop(hop) => Self::hop_clause_sql_dialect(hop, table, param_counter),
        }
    }

    fn clauses_to_conditions_sql(
        clauses: &[WhereClause],
        table: &str,
        param_counter: &mut usize,
    ) -> Result<(Vec<String>, Vec<(String, serde_json::Value)>)> {
        let mut conditions = Vec::new();
        let mut params = Vec::new();
        for clause in clauses {
            let (condition, mut clause_params) =
                Self::clause_to_condition_sql(clause, table, param_counter)?;
            params.append(&mut clause_params);
            conditions.push(condition);
        }
        Ok((conditions, params))
    }

    fn int_clause_sql_doc(
        field: &str,
        pred: &IntPredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let col = super::sql_document::sql_doc_column(field);
        let param_key = Self::next_param_key(param_counter);
        let (op, value) = match pred {
            IntPredicate::Equals(v) => ("=", serde_json::Value::Number((*v).into())),
            IntPredicate::GreaterThan(v) => (">", serde_json::Value::Number((*v).into())),
            IntPredicate::GreaterThanOrEqual(v) => (">=", serde_json::Value::Number((*v).into())),
            IntPredicate::LessThan(v) => ("<", serde_json::Value::Number((*v).into())),
            IntPredicate::LessThanOrEqual(v) => ("<=", serde_json::Value::Number((*v).into())),
        };
        let params = vec![(param_key.clone(), value)];
        (format!("{col} {op} ${param_key}"), params)
    }

    fn datetime_clause_sql_doc(
        field: &str,
        pred: &DateTimePredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let col = super::sql_document::sql_doc_column(field);
        let param_key = Self::next_param_key(param_counter);
        let (op, value) = match pred {
            DateTimePredicate::Equals(dt) => (
                "=",
                serde_json::Value::Number(dt.timestamp().into()),
            ),
            DateTimePredicate::After(dt) => (
                ">",
                serde_json::Value::Number(dt.timestamp().into()),
            ),
            DateTimePredicate::Before(dt) => (
                "<",
                serde_json::Value::Number(dt.timestamp().into()),
            ),
        };
        let params = vec![(param_key.clone(), value)];
        (format!("{col} {op} ${param_key}"), params)
    }

    fn null_clause_sql_doc(field: &str, pred: NullPredicate) -> String {
        let col = super::sql_document::sql_doc_column(field);
        match pred {
            NullPredicate::IsNone => format!("{col} IS NULL"),
            NullPredicate::IsSome => format!("{col} IS NOT NULL"),
        }
    }

    fn string_clause_sql_sql(
        field: &str,
        pred: &StringPredicate,
        table: &str,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        if field == "id" {
            if let StringPredicate::Equals(s) = pred {
                return Self::string_equals_id_clause_sql_sql(table, s, param_counter);
            }
        }
        let col = super::sql_document::sql_doc_column(field);
        let param_key = Self::next_param_key(param_counter);
        let (op, value, like_escape) = match pred {
            StringPredicate::Equals(s) => ("=", serde_json::Value::String(s.clone()), false),
            StringPredicate::Contains(s) => (
                "LIKE",
                serde_json::Value::String(format!("%{}%", escape_like_pattern(s))),
                true,
            ),
            StringPredicate::StartsWith(s) => (
                "LIKE",
                serde_json::Value::String(format!("{}%", escape_like_pattern(s))),
                true,
            ),
            StringPredicate::EndsWith(s) => (
                "LIKE",
                serde_json::Value::String(format!("%{}", escape_like_pattern(s))),
                true,
            ),
            StringPredicate::LessThan(s) => ("<", serde_json::Value::String(s.clone()), false),
        };
        let params = vec![(param_key.clone(), value)];
        let sql = if like_escape {
            format!("{col} {op} ${param_key} ESCAPE '\\'")
        } else {
            format!("{col} {op} ${param_key}")
        };
        (sql, params)
    }

    fn string_equals_id_clause_sql_sql(
        _table: &str,
        s: &str,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let bare = s
            .rsplit_once(':')
            .map_or_else(|| s.to_string(), |(_, id)| id.to_string());
        let id_key = Self::next_param_key(param_counter);
        let params = vec![(id_key.clone(), serde_json::Value::String(bare))];
        (format!("id = ${id_key}"), params)
    }

    fn record_clause_sql_sql(
        field: &str,
        pred: &RecordPredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let col = super::sql_document::sql_doc_column(field);
        match pred {
            RecordPredicate::Equals(rid) => {
                let rid_key = Self::next_param_key(param_counter);
                let bare_key = Self::next_param_key(param_counter);
                let params = vec![
                    (
                        rid_key.clone(),
                        serde_json::Value::String(format!("{}:{}", rid.table(), rid.id())),
                    ),
                    (
                        bare_key.clone(),
                        serde_json::Value::String(rid.id().to_string()),
                    ),
                ];
                (
                    format!("({col} = ${rid_key} OR {col} = ${bare_key})"),
                    params,
                )
            }
        }
    }
}
