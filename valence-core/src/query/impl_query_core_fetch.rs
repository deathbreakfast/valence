impl QueryCore {
    /// Set the projection fields for SELECT clause
    ///
    /// If not set, defaults to SELECT *.
    #[must_use]
    pub fn select(mut self, fields: Vec<String>) -> Self {
        self.projection = Some(fields);
        self
    }
    /// Get a record by ID, returning a minimal result with just the ID
    ///
    /// Loads via [`DatabaseBackend::get_record`](crate::DatabaseBackend::get_record)
    /// using `table:id` wire form.
    ///
    /// **Privacy:** this helper does **not** evaluate entity read policies. Prefer
    /// [`Self::get_entity`] or generated [`crate::Model::get`](crate::Model::get) for
    /// caller-visible reads; those map privacy denial to `Ok(None)`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_id_only(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
    ) -> Result<Option<IdOnlyRecord>> {
        let table_str = table.into();
        let id_str = id.as_ref();

        let backend = valence.backend_for_table(&table_str)?;
        let record = backend.get_record(&table_str, id_str).await?;

        match record {
            Some(_) => Ok(Some(IdOnlyRecord {
                id: id_str.to_string(),
            })),
            None => Ok(None),
        }
    }

    /// Get a record by ID and return it as `serde_json::Value`.
    ///
    /// Uses the table's resolved [`crate::backend::DatabaseBackend`] (same routing as CRUD).
    ///
    /// **Privacy:** raw storage read with no policy evaluation.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_record_json(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
    ) -> Result<Option<serde_json::Value>> {
        let table_str = table.into();
        let id_str = id.as_ref();
        let backend = valence.backend_for_table(&table_str)?;
        backend.get_record(&table_str, id_str).await
    }

    /// Get the latest N record IDs from a table, ordered by ID descending
    ///
    /// This is useful for displaying sample records where only the ID is needed.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn latest_ids(
        table: impl Into<String>,
        limit: u32,
        valence: &Valence,
    ) -> Result<Vec<IdOnlyRecord>> {
        let table_str = table.into();

        // Use QueryCore to build the query with projection
        let query = QueryCore::new(table_str)
            .select(vec!["id".to_string()])
            .order_by("id".to_string(), SortDirection::Desc)
            .limit(limit);

        // Rows come from `execute_compiled_query` as JSON where Thing-shaped `id` values are
        // strings like `"counter:singleton"` — deserialize as `String`, then strip to id-only.
        let raw: Vec<IdOnlyRecord> = query.execute(valence).await?;
        let records: Vec<IdOnlyRecord> = raw
            .into_iter()
            .map(|r| IdOnlyRecord {
                id: thing_to_id_only(r.id),
            })
            .collect();

        Ok(records)
    }

    /// Get an entity with privacy filtering applied
    ///
    /// This is the primary method for generic entity access.
    /// Returns a ValenceEntity with only the fields the viewer can see.
    ///
    /// Flow:
    /// 1. Get schema metadata
    /// 2. Load full record
    /// 3. Check entity-level privacy (`Ok(None)` on deny — same as missing)
    /// 4. Apply field-level privacy filtering
    /// 5. Return ValenceEntity
    ///
    /// Denied reads and missing rows both return `Ok(None)` so callers cannot
    /// distinguish existence from a privacy denial.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_entity(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
    ) -> Result<Option<ValenceEntity>> {
        use crate::entity::ValenceEntity;
        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;
        use std::sync::Arc;

        let table_str = table.into();
        let id_str = id.as_ref();

        // Step 1: Get schema metadata
        let schema = SchemaRegistry::global()
            .get_schema(&table_str)
            .ok_or_else(|| Error::NotFound(format!("Schema not found: {table_str}")))?;

        // Step 2: Load full record (single trip; no privacy-free existence probe)
        let raw_data = Self::get_record_json(&table_str, id_str, valence).await?;
        let Some(raw_data) = raw_data else {
            return Ok(None);
        };

        // Step 3: Entity-level privacy — deny looks like not-found
        match PrivacyEvaluator::check_entity_read(schema, &raw_data, valence).await {
            Ok(()) => {}
            Err(Error::Privacy(_)) => return Ok(None),
            Err(e) => return Err(e),
        }

        // Step 4: Apply field-level privacy filtering
        let (filtered_data, hidden_fields): (BTreeMap<String, serde_json::Value>, Vec<String>) =
            PrivacyEvaluator::filter_entity_fields(schema, &raw_data, valence.actor())?;

        // Step 5: Return ValenceEntity
        Ok(Some(ValenceEntity::new(
            table_str.clone(),
            id_str.to_string(),
            filtered_data,
            hidden_fields,
            Arc::new((*schema).clone()),
        )))
    }
}
