/// Rows each privacy stage dropped from a query.
#[derive(Default)]
struct RowsDropped {
    connection: usize,
    hop: usize,
    pending: usize,
    entity: usize,
}

impl QueryCore {
    /// Clamp limit/offset; default missing limit to [`super::MAX_QUERY_LIMIT`].
    fn apply_query_window_clamps(&mut self) {
        self.limit = Some(
            self.limit
                .unwrap_or(super::MAX_QUERY_LIMIT)
                .min(super::MAX_QUERY_LIMIT),
        );
        if let Some(offset) = self.offset {
            self.offset = Some(offset.min(super::MAX_QUERY_OFFSET));
        }
    }

    /// Compile the query for the active backend and execute it
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    #[tracing::instrument(
        name = "valence.query.execute",
        skip(self, valence, purpose),
        fields(table = %self.table)
    )]
    pub async fn execute<T>(
        self,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Vec<T>>
    where
        T: DeserializeOwned + Serialize,
    {
        let _ = purpose;
        let (rows, _) = self.execute_inner(valence).await?;
        Ok(rows)
    }

    /// Like [`Self::execute`], but also reports how many rows the backend returned.
    ///
    /// [`super::QueryStats::fetched`] counts the rows the backend returned for this window
    /// *before* connection, hop, pending-deletion and entity read privacy, so it can
    /// be larger than the returned `Vec`. A caller paging in fixed windows can stop
    /// when `fetched < limit` (the table ended) and keep going when the window was
    /// full but privacy hid every row. Since the query now refills the page itself, a page
    /// shorter than `limit` also means the table (or the scan budget) ended.
    ///
    /// Read privacy runs before the caller's `limit` and `offset` count: the query pages
    /// through backend windows until `limit` readable rows are found, the table ends, or
    /// [`super::MAX_QUERY_HIDDEN_SCAN`] unreadable rows past the window have been scanned.
    ///
    /// Paging by `offset` re-reads every row before the window on each call, because
    /// `offset` counts readable rows. To read a long table once, page with
    /// [`super::QueryStats::resume`] and [`Self::resume_at`] instead, and stop when
    /// `resume` is `None`.
    ///
    /// `fetched` counts rows the caller may not read. Do not show it to users: it
    /// can reveal that hidden rows exist.
    ///
    /// For a trait-union query (several tables) `fetched` is the sum of each
    /// table's backend rows before the merge window, so it is an upper bound.
    ///
    /// ```ignore
    /// let limit = 100;
    /// let mut offset = 0;
    /// loop {
    ///     let (rows, stats) = QueryCore::new("notes".into())
    ///         .limit(limit)
    ///         .offset(offset)
    ///         .execute_with_stats::<Note>(&valence, purpose.clone())
    ///         .await?;
    ///     handle(rows);
    ///     if stats.fetched < limit as usize {
    ///         break;
    ///     }
    ///     offset += limit;
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Fails when no query compiler or backend is registered for the table, when the
    /// backend query fails, when a row does not deserialize into `T`, or when a privacy
    /// or ownership lookup errors. Rows the actor may not read are dropped, not errors.
    #[tracing::instrument(
        name = "valence.query.execute_with_stats",
        skip(self, valence, purpose),
        fields(table = %self.table)
    )]
    pub async fn execute_with_stats<T>(
        self,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<(Vec<T>, super::QueryStats)>
    where
        T: DeserializeOwned + Serialize,
    {
        let _ = purpose;
        self.execute_inner(valence).await
    }

    /// Shared body of `execute` and `execute_with_stats`: rows plus scan stats.
    async fn execute_inner<T>(mut self, valence: &Valence) -> Result<(Vec<T>, super::QueryStats)>
    where
        T: DeserializeOwned + Serialize,
    {
        self.apply_query_window_clamps();
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
            // Cap each branch fetch; final window applied after merge.
            branch_core.limit = Some(super::MAX_QUERY_LIMIT);
            branch_core.offset = None;
            // A trait union merges several tables, so it has no single raw position.
            branch_core.resume_at = None;

            let mut merged_json = Vec::new();
            let mut fetched = 0usize;
            for tbl in tables {
                let mut branch = branch_core.clone();
                branch.table = tbl;
                let (rows, branch_stats): (Vec<T>, super::QueryStats) =
                    Self::execute_on_table(branch, valence).await?;
                fetched += branch_stats.fetched;
                for row in rows {
                    merged_json.push(
                        serde_json::to_value(&row)
                            .map_err(Error::from)?,
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
                        .map_err(Error::from)?,
                );
            }
            return Ok((
                results,
                super::QueryStats {
                    fetched,
                    resume: None,
                },
            ));
        }

        Self::execute_on_table(self, valence).await
    }

    #[allow(clippy::cast_possible_wrap, reason = "telemetry accepts signed row counts while collection lengths are usize")]
    #[tracing::instrument(
        name = "valence.query.execute_on_table",
        skip(self, valence),
        fields(table = %self.table)
    )]
    async fn execute_on_table<T>(self, valence: &Valence) -> Result<(Vec<T>, super::QueryStats)>
    where
        T: DeserializeOwned + Serialize,
    {
        use crate::instrumentation::query::{
            classify_query_target, record_compile_error, record_deserialize_error,
            record_query_success, resolve_trait_name,
        };
        use crate::instrumentation::timing::QueryTimer;
        use crate::query_compiler_registry::compile_for_engine;
        use tracing::Instrument;

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
        let table_label = table.split(',').next().unwrap_or(&table).trim().to_string();

        // Read privacy runs after the backend applies LIMIT/OFFSET, so one backend window can
        // come back short. Page through backend windows, filter each, and count the caller's
        // OFFSET and LIMIT over rows the actor can read. Grouped queries have no row-level
        // privacy and keep a single backend window.
        let want = self.limit.unwrap_or(super::MAX_QUERY_LIMIT) as usize;
        let skip = self.offset.unwrap_or(0) as usize;
        let paged = self.group_by.is_empty();
        let chunk = if paged {
            (skip + want).clamp(1, super::MAX_QUERY_LIMIT as usize)
        } else {
            want
        };
        let scan_budget = skip + want + super::MAX_QUERY_HIDDEN_SCAN as usize;

        let mut results: Vec<T> = Vec::new();
        let mut to_skip = skip;
        // A resumed scan starts at the raw row where the last one stopped, so paging
        // stays linear instead of re-reading from row 0 for every `offset`.
        let start = if paged {
            self.resume_at.unwrap_or(0) as usize
        } else {
            skip
        };
        let mut raw_offset = start;
        // Raw position just past the last row this page consumed (skipped or returned).
        let mut consumed = start;
        let mut table_ended = false;
        let mut rows_db = 0usize;
        let mut dropped = RowsDropped::default();
        loop {
            let mut window = self.clone();
            window.limit = Some(u32::try_from(chunk).unwrap_or(super::MAX_QUERY_LIMIT));
            window.offset = (raw_offset > 0).then(|| u32::try_from(raw_offset).unwrap_or(u32::MAX));
            let compiled = match compile_for_engine(engine_id, &window) {
                Ok(c) => c,
                Err(e) => {
                    record_compile_error(&self, &e);
                    return Err(e);
                }
            };
            let backend_span = tracing::info_span!(
                "valence.backend.execute_compiled_query",
                engine = engine_id,
                table = %table
            );
            let json_rows: Vec<serde_json::Value> = backend
                .execute_compiled_query(&compiled)
                .instrument(backend_span)
                .await?;
            let fetched = json_rows.len();
            let window_start = raw_offset;
            rows_db += fetched;
            raw_offset += fetched;

            let mut typed = Vec::with_capacity(fetched);
            for (i, row) in json_rows.into_iter().enumerate() {
                match serde_json::from_value(row) {
                    Ok(v) => typed.push((window_start + i, v)),
                    Err(e) => {
                        let err = Error::from(e);
                        record_deserialize_error(&table, &err);
                        return Err(err);
                    }
                }
            }

            let visible = self.filter_window(typed, valence, &mut dropped).await?;
            if !paged {
                results = visible.into_iter().map(|(_, row)| row).collect();
                break;
            }
            // Rows the filters dropped are consumed too: a resumed scan never needs them.
            consumed = raw_offset;
            for (pos, row) in visible {
                if results.len() >= want {
                    // Page full: resume at the first readable row it didn't take.
                    consumed = pos;
                    break;
                }
                if to_skip > 0 {
                    to_skip -= 1;
                } else {
                    results.push(row);
                }
            }
            if fetched < chunk {
                // The backend has nothing past this window; rows left in it still resume.
                table_ended = consumed == raw_offset;
                break;
            }
            if results.len() >= want || rows_db >= scan_budget {
                break;
            }
        }
        let resume = (paged && !table_ended).then(|| u32::try_from(consumed).unwrap_or(u32::MAX));

        for (stage, count) in [
            ("connection_privacy", dropped.connection),
            ("hop_privacy", dropped.hop),
            ("pending_deletion", dropped.pending),
            ("entity_privacy", dropped.entity),
        ] {
            if count > 0 {
                crate::instrumentation::metrics::record_query_rows_filtered(
                    &table_label,
                    stage,
                    count as i64,
                );
            }
        }
        let wall_ms = timer.elapsed_ms();
        let rows_after_hop = rows_db - dropped.connection - dropped.hop;
        record_query_success(
            &self,
            query_target,
            &trait_name,
            rows_db as i64,
            rows_after_hop as i64,
            (rows_after_hop - dropped.pending - dropped.entity) as i64,
            wall_ms,
        );

        Ok((
            results,
            super::QueryStats {
                fetched: rows_db,
                resume,
            },
        ))
    }

    /// Run one backend window through connection, hop, pending-deletion and entity privacy,
    /// adding the rows each stage dropped to `dropped`. Each row keeps its position in
    /// the backend window, so a page that fills mid-window knows where to resume.
    async fn filter_window<T>(
        &self,
        rows: Vec<(usize, T)>,
        valence: &Valence,
        dropped: &mut RowsDropped,
    ) -> Result<Vec<(usize, T)>>
    where
        T: DeserializeOwned + Serialize,
    {
        let n = rows.len();
        let rows = self.post_filter_connection_privacy(rows, valence).await?;
        dropped.connection += n - rows.len();
        let n = rows.len();
        let rows = self.post_filter_hop_privacy(rows, valence).await?;
        dropped.hop += n - rows.len();
        // `Model::get` hides rows queued for deletion; lists and trait queries match it. This runs
        // before entity privacy, which drops undeclared fields such as `id` from the row.
        let n = rows.len();
        let rows = self.post_filter_pending_deletion(rows, valence).await?;
        dropped.pending += n - rows.len();
        // Entity-level read privacy for every list/query row (HasMany / Model::query inherit this).
        let n = rows.len();
        let rows = self.post_filter_entity_privacy(rows, valence).await?;
        dropped.entity += n - rows.len();
        Ok(rows)
    }

    /// Execute as a distinct-value query, returning a flat `Vec<String>`.
    ///
    /// Loads rows through [`Self::execute`] so entity / connection / hop privacy
    /// post-filters apply before values are collected.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn distinct_values(
        mut self,
        field: &str,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Vec<String>> {
        let _ = purpose;
        crate::safe_ident::assert_safe_ident(field)?;
        self.projection = None;
        self.group_by.clear();
        self.order_by.clear();
        self.limit = None;
        self.offset = None;

        let rows: Vec<serde_json::Value> = {
            self.execute(valence, purpose).await?
        };
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(raw) = row.get(field) else {
                continue;
            };
            let value = match raw {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => continue,
                other => other
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| other.to_string()),
            };
            if seen.insert(value.clone()) {
                results.push(value);
            }
        }
        Ok(results)
    }


    async fn post_filter_connection_privacy<T>(
        &self,
        results: Vec<(usize, T)>,
        valence: &Valence,
    ) -> Result<Vec<(usize, T)>>
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
        for (pos, row) in results {
            let mut exclude = false;
            let row_json =
                serde_json::to_value(&row).map_err(|e| Error::Internal(e.to_string()))?;
            for (from_field, to_table) in &connection_exists {
                let Some(target_id) = extract_id_from_fk_value(row_json.get(from_field)) else {
                    exclude = true;
                    break;
                };
                let Some(raw_data) =
                    Self::get_record_json(to_table.as_str(), target_id.as_str(), valence, crate::data_use::DataUsePurpose::framework_nested()).await?
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
                filtered.push((pos, row));
            }
        }
        Ok(filtered)
    }

    async fn post_filter_hop_privacy<T>(
        &self,
        results: Vec<(usize, T)>,
        valence: &Valence,
    ) -> Result<Vec<(usize, T)>>
    where
        T: Serialize + DeserializeOwned,
    {
        if self.hop_source.is_none() {
            return Ok(results);
        }
        self.filter_rows_by_entity_read(results, valence).await
    }

    /// Drop rows the viewer cannot read under entity-level policies, then apply field privacy.
    ///
    /// Unregistered tables pass through (no schema ⇒ no policy evaluation). Prefer registering
    /// schemas for any table that holds sensitive data.
    async fn post_filter_entity_privacy<T>(
        &self,
        results: Vec<(usize, T)>,
        valence: &Valence,
    ) -> Result<Vec<(usize, T)>>
    where
        T: Serialize + DeserializeOwned,
    {
        // Hop path already applied the same filter when `hop_source` is set.
        if self.hop_source.is_some() {
            return Ok(results);
        }
        self.filter_rows_by_entity_read(results, valence).await
    }

    /// Drop rows whose ownership status is `pending_deletion`, as `Model::get` does.
    ///
    /// Each row's table comes from its record id, so trait-union queries check every member
    /// table. Ownership lookup errors fail closed, like the `get` gate.
    async fn post_filter_pending_deletion<T>(
        &self,
        results: Vec<(usize, T)>,
        valence: &Valence,
    ) -> Result<Vec<(usize, T)>>
    where
        T: Serialize,
    {
        use crate::ownership::{normalize_record_id_for_ownership, OwnershipService};
        use std::collections::{HashMap, HashSet};

        if results.is_empty() {
            return Ok(results);
        }
        let default_table = backend_schema_table(self.table.as_str()).to_string();
        let mut keys: Vec<Option<(String, String)>> = Vec::with_capacity(results.len());
        let mut by_table: HashMap<String, Vec<String>> = HashMap::new();
        for (_, row) in &results {
            let row_json = serde_json::to_value(row).map_err(|e| Error::Internal(e.to_string()))?;
            let key = match row_json.get("id") {
                // Same normalization as the `get` gate: `table:` is stripped only for a known table.
                Some(serde_json::Value::String(raw)) => {
                    let bare = normalize_record_id_for_ownership(raw);
                    let table = raw
                        .trim()
                        .strip_suffix(bare.as_str())
                        .and_then(|head| head.strip_suffix(':'))
                        .filter(|head| !head.is_empty())
                        .map_or_else(|| default_table.clone(), str::to_string);
                    Some((table, bare))
                }
                Some(id) => serde_json::from_value::<crate::record_id::RecordId>(id.clone())
                    .ok()
                    .map(|rid| {
                        let table = if rid.table().is_empty() {
                            default_table.clone()
                        } else {
                            rid.table().to_string()
                        };
                        (table, rid.id().to_string())
                    }),
                None => None,
            };
            if let Some((table, bare)) = &key {
                by_table.entry(table.clone()).or_default().push(bare.clone());
            }
            keys.push(key);
        }

        let mut pending: HashSet<(String, String)> = HashSet::new();
        for (table, ids) in by_table {
            for bare in OwnershipService::pending_deletion_bare_ids_subset(&table, &ids, valence).await? {
                pending.insert((table.clone(), bare));
            }
        }
        if pending.is_empty() {
            return Ok(results);
        }
        Ok(results
            .into_iter()
            .zip(keys)
            .filter(|(_, key)| key.as_ref().is_none_or(|k| !pending.contains(k)))
            .map(|(row, _)| row)
            .collect())
    }

    async fn filter_rows_by_entity_read<T>(
        &self,
        results: Vec<(usize, T)>,
        valence: &Valence,
    ) -> Result<Vec<(usize, T)>>
    where
        T: Serialize + DeserializeOwned,
    {
        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;

        let schema_table = backend_schema_table(self.table.as_str());
        let Some(schema) = SchemaRegistry::global().get_schema(schema_table) else {
            return Ok(results);
        };

        let mut kept = Vec::with_capacity(results.len());
        for (pos, row) in results {
            let row_json =
                serde_json::to_value(&row).map_err(|e| Error::Internal(e.to_string()))?;
            if PrivacyEvaluator::check_entity_read(schema, &row_json, valence)
                .await
                .is_ok()
            {
                let (filtered, _) =
                    PrivacyEvaluator::filter_entity_fields(schema, &row_json, valence.actor())?;
                let filtered_json = serde_json::Value::Object(filtered.into_iter().collect());
                kept.push((
                    pos,
                    serde_json::from_value(filtered_json).map_err(Error::from)?,
                ));
            }
        }
        Ok(kept)
    }

}
