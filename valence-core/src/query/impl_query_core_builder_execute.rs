impl QueryCore {
    /// Compile the query for the active backend and execute it
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn execute<T>(self, valence: &Valence) -> Result<Vec<T>>
    where
        T: DeserializeOwned + Serialize,
    {
        let tables: Vec<String> = self
            .table
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if tables.len() > 1 {
            let order_by = self.order_by.clone();
            let limit = self.limit;
            let offset = self.offset;
            let mut branch_core = self;
            branch_core.order_by.clear();
            branch_core.limit = None;
            branch_core.offset = None;

            let mut merged_json = Vec::new();
            for tbl in tables {
                let mut branch = branch_core.clone();
                branch.table = tbl;
                let rows: Vec<T> = Self::execute_on_table(branch, valence).await?;
                for row in rows {
                    merged_json.push(
                        serde_json::to_value(&row)
                            .map_err(|e| Error::Serialization(e.to_string()))?,
                    );
                }
            }

            super::sql_helpers::apply_post_merge_query_window(
                &mut merged_json,
                &order_by,
                offset,
                limit,
            );

            let mut results = Vec::with_capacity(merged_json.len());
            for row in merged_json {
                results.push(
                    serde_json::from_value(row)
                        .map_err(|e| Error::Serialization(e.to_string()))?,
                );
            }
            return Ok(results);
        }

        Self::execute_on_table(self, valence).await
    }

    #[allow(clippy::cast_possible_wrap, reason = "telemetry accepts signed row counts while collection lengths are usize")]
    async fn execute_on_table<T>(self, valence: &Valence) -> Result<Vec<T>>
    where
        T: DeserializeOwned + Serialize,
    {
        use crate::instrumentation::query::{
            classify_query_target, record_compile_error, record_deserialize_error,
            record_query_success, resolve_trait_name,
        };
        use crate::instrumentation::timing::QueryTimer;
        use crate::query_compiler_registry::compile_for_engine;

        let table = self.table.clone();
        let query_target = classify_query_target(&table, false);
        let trait_name = if query_target == crate::instrumentation::query::QueryTarget::TraitUnion {
            resolve_trait_name(&table)
        } else {
            self.model_type.clone().unwrap_or_default()
        };
        let timer = QueryTimer::start(
            table.split(',').next().unwrap_or(&table).trim(),
            query_target.as_str(),
        );

        let schema_table = backend_schema_table(table.as_str());
        let backend = valence.backend_for_table(schema_table)?;
        let engine_id = backend.engine_id();
        let compiled = match compile_for_engine(engine_id, &self) {
            Ok(c) => c,
            Err(e) => {
                record_compile_error(&self, &e);
                return Err(e);
            }
        };
        let json_rows: Vec<serde_json::Value> = backend.execute_compiled_query(&compiled).await?;
        let rows_db = json_rows.len();

        let mut results = Vec::with_capacity(json_rows.len());
        for row in json_rows {
            match serde_json::from_value(row) {
                Ok(v) => results.push(v),
                Err(e) => {
                    let err = Error::Serialization(e.to_string());
                    record_deserialize_error(&table, &err);
                    return Err(err);
                }
            }
        }

        let filtered = self
            .post_filter_connection_privacy(results, valence)
            .await?;
        let rows_after_privacy = filtered.len();
        if rows_db > rows_after_privacy {
            crate::instrumentation::metrics::record_query_rows_filtered(
                table.split(',').next().unwrap_or(&table).trim(),
                "connection_privacy",
                (rows_db - rows_after_privacy) as i64,
            );
        }

        let filtered = self.post_filter_hop_privacy(filtered, valence).await?;
        let rows_after_hop = filtered.len();
        if rows_after_privacy > rows_after_hop {
            crate::instrumentation::metrics::record_query_rows_filtered(
                table.split(',').next().unwrap_or(&table).trim(),
                "hop_privacy",
                (rows_after_privacy - rows_after_hop) as i64,
            );
        }

        let rows_after_pending = filtered.len();
        let wall_ms = timer.elapsed_ms();
        record_query_success(
            &self,
            query_target,
            &trait_name,
            rows_db as i64,
            rows_after_hop as i64,
            rows_after_pending as i64,
            wall_ms,
        );

        Ok(filtered)
    }

    /// Execute as a distinct-value query, returning a flat `Vec<String>`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn distinct_values(mut self, field: &str, valence: &Valence) -> Result<Vec<String>> {
        self.projection = Some(vec![format!("VALUE {}", field)]);
        self.group_by = vec![field.to_string()];
        self.order_by.clear();
        self.limit = None;
        self.offset = None;

        use crate::query_compiler_registry::compile_for_engine;

        let table = self.table.clone();
        let backend = valence.backend_for_table(backend_schema_table(table.as_str()))?;
        let compiled = compile_for_engine(backend.engine_id(), &self)?;
        let json_rows: Vec<serde_json::Value> = backend.execute_compiled_query(&compiled).await?;
        let mut results = Vec::with_capacity(json_rows.len());
        for row in json_rows {
            results.push(
                serde_json::from_value(row).map_err(|e| Error::Serialization(e.to_string()))?,
            );
        }
        Ok(results)
    }

    async fn post_filter_connection_privacy<T>(
        &self,
        results: Vec<T>,
        valence: &Valence,
    ) -> Result<Vec<T>>
    where
        T: Serialize,
    {
        let connection_exists: Vec<_> = self
            .where_clauses
            .iter()
            .filter_map(|c| {
                if let WhereClause::ConnectionExists {
                    from_field,
                    to_table,
                    ..
                } = c
                {
                    Some((from_field.clone(), to_table.clone()))
                } else {
                    None
                }
            })
            .collect();

        if connection_exists.is_empty() {
            return Ok(results);
        }

        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;

        let mut filtered = Vec::with_capacity(results.len());
        for row in results {
            let mut exclude = false;
            let row_json =
                serde_json::to_value(&row).map_err(|e| Error::Internal(e.to_string()))?;
            for (from_field, to_table) in &connection_exists {
                let Some(target_id) = extract_id_from_fk_value(row_json.get(from_field)) else {
                    exclude = true;
                    break;
                };
                let Some(raw_data) =
                    Self::get_record_json(to_table.as_str(), target_id.as_str(), valence).await?
                else {
                    exclude = true;
                    break;
                };
                let Some(schema) = SchemaRegistry::global().get_schema(to_table.as_str()) else {
                    exclude = true;
                    break;
                };
                if PrivacyEvaluator::check_entity_read(schema, &raw_data, valence)
                    .await
                    .is_err()
                {
                    exclude = true;
                    break;
                }
            }
            if !exclude {
                filtered.push(row);
            }
        }
        Ok(filtered)
    }

    async fn post_filter_hop_privacy<T>(&self, results: Vec<T>, valence: &Valence) -> Result<Vec<T>>
    where
        T: Serialize,
    {
        if self.hop_source.is_none() {
            return Ok(results);
        }

        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;

        let Some(schema) = SchemaRegistry::global().get_schema(&self.table) else {
            return Ok(results);
        };

        let concurrency = 1usize;
        let rows: Vec<T> = results;
        let mut keep = vec![false; rows.len()];
        for chunk_start in (0..rows.len()).step_by(concurrency) {
            let chunk_end = (chunk_start + concurrency).min(rows.len());
            for idx in chunk_start..chunk_end {
                let row_json =
                    serde_json::to_value(&rows[idx]).map_err(|e| Error::Internal(e.to_string()))?;
                let ok = PrivacyEvaluator::check_entity_read(schema, &row_json, valence)
                    .await
                    .is_ok();
                keep[idx] = ok;
            }
        }
        Ok(rows
            .into_iter()
            .enumerate()
            .filter_map(|(idx, row)| keep[idx].then_some(row))
            .collect())
    }
}
