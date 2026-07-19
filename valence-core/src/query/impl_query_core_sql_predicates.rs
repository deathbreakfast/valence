impl QueryCore {
    fn int_clause_sql(
        field: &str,
        pred: &IntPredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let param_key = Self::next_param_key(param_counter);
        let (op, value) = match pred {
            IntPredicate::Equals(v) => ("=", serde_json::Value::Number((*v).into())),
            IntPredicate::GreaterThan(v) => (">", serde_json::Value::Number((*v).into())),
            IntPredicate::GreaterThanOrEqual(v) => (">=", serde_json::Value::Number((*v).into())),
            IntPredicate::LessThan(v) => ("<", serde_json::Value::Number((*v).into())),
            IntPredicate::LessThanOrEqual(v) => ("<=", serde_json::Value::Number((*v).into())),
        };
        let params = vec![(param_key.clone(), value)];
        (format!("{field} {op} ${param_key}"), params)
    }

    fn string_equals_id_clause_sql(
        table: &str,
        s: &str,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let bare = s
            .rsplit_once(':').map_or_else(|| s.to_string(), |(_, id)| id.to_string());
        let table_key = Self::next_param_key(param_counter);
        let id_key = Self::next_param_key(param_counter);
        let params = vec![
            (table_key.clone(), serde_json::Value::String(table.to_string())),
            (id_key.clone(), serde_json::Value::String(bare)),
        ];
        let sql = format!(
            "(id = type::record(${table_key}, ${id_key}) OR id = ${id_key} OR <string> id = ${id_key})"
        );
        (sql, params)
    }

    fn string_field_clause_sql(
        field: &str,
        pred: &StringPredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let param_key = Self::next_param_key(param_counter);
        // SurrealQL has no SQL LIKE; use native string functions / CONTAINS.
        match pred {
            StringPredicate::Equals(s) => {
                let params = vec![(param_key.clone(), serde_json::Value::String(s.clone()))];
                (format!("{field} = ${param_key}"), params)
            }
            StringPredicate::Contains(s) => {
                let params = vec![(param_key.clone(), serde_json::Value::String(s.clone()))];
                (format!("{field} CONTAINS ${param_key}"), params)
            }
            StringPredicate::StartsWith(s) => {
                let params = vec![(param_key.clone(), serde_json::Value::String(s.clone()))];
                (
                    format!("string::starts_with({field}, ${param_key})"),
                    params,
                )
            }
            StringPredicate::EndsWith(s) => {
                let params = vec![(param_key.clone(), serde_json::Value::String(s.clone()))];
                (
                    format!("string::ends_with({field}, ${param_key})"),
                    params,
                )
            }
        }
    }

    fn string_clause_sql(
        field: &str,
        pred: &StringPredicate,
        table: &str,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        if field == "id" {
            if let StringPredicate::Equals(s) = pred {
                return Self::string_equals_id_clause_sql(table, s, param_counter);
            }
        }
        Self::string_field_clause_sql(field, pred, param_counter)
    }

    fn datetime_clause_sql(
        field: &str,
        pred: &DateTimePredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let param_key = Self::next_param_key(param_counter);
        let (op, value) = match pred {
            DateTimePredicate::Equals(dt) => ("=", serde_json::Value::String(dt.to_rfc3339())),
            DateTimePredicate::After(dt) => (">", serde_json::Value::String(dt.to_rfc3339())),
            DateTimePredicate::Before(dt) => ("<", serde_json::Value::String(dt.to_rfc3339())),
        };
        let params = vec![(param_key.clone(), value)];
        (format!("{field} {op} ${param_key}"), params)
    }

    fn record_clause_sql(
        field: &str,
        pred: &RecordPredicate,
        param_counter: &mut usize,
    ) -> (String, Vec<(String, serde_json::Value)>) {
        let param_key = Self::next_param_key(param_counter);
        match pred {
            RecordPredicate::Equals(rid) => {
                let tb_key = format!("{param_key}_tb");
                let id_key = format!("{param_key}_id");
                let rid_key = format!("{param_key}_rid");
                let params = vec![
                    (tb_key.clone(), serde_json::json!(rid.table())),
                    (id_key.clone(), serde_json::json!(rid.id())),
                    (
                        rid_key.clone(),
                        serde_json::json!(format!("{}:{}", rid.table(), rid.id())),
                    ),
                ];
                (
                    format!(
                        "({field} = type::record(${tb_key}, ${id_key}) \
                         OR {field} = ${rid_key} \
                         OR {field} = ${id_key} \
                         OR {field}.id = ${id_key})"
                    ),
                    params,
                )
            }
        }
    }

    fn null_clause_sql(field: &str, pred: NullPredicate) -> String {
        match pred {
            NullPredicate::IsNone => format!("({field} IS NONE OR {field} IS NULL)"),
            NullPredicate::IsSome => {
                format!("({field} IS NOT NONE AND {field} IS NOT NULL)")
            }
        }
    }
}
