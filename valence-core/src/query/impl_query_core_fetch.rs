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
    /// **Privacy:** evaluates the table's entity Read policy for `valence`'s actor, the
    /// same as [`Self::get_record_json`]. A row the actor can't read returns `Ok(None)`,
    /// the same as a missing row, so callers can't use this to probe for ids.
    /// # Errors
    ///
    /// [`Error::NotFound`] when `table` has no registered schema, and any storage or
    /// policy evaluation error.
    pub async fn get_id_only(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Option<IdOnlyRecord>> {
        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;

        let table_str = table.into();
        let id_str = id.as_ref();
        let schema = SchemaRegistry::lookup(&table_str)
            .ok_or_else(|| Error::NotFound(format!("Schema not found: {table_str}")))?;
        let Some(raw_data) =
            Self::get_stored_record_json(&table_str, id_str, valence, purpose).await?
        else {
            return Ok(None);
        };
        match PrivacyEvaluator::check_entity_read(schema, &raw_data, valence).await {
            Ok(()) => Ok(Some(IdOnlyRecord {
                id: id_str.to_string(),
            })),
            Err(Error::Privacy(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get a record by ID as `serde_json::Value`, as the calling actor may see it.
    ///
    /// Uses the table's resolved [`crate::backend::DatabaseBackend`] (same routing as CRUD).
    ///
    /// **Privacy:** evaluates the table's entity Read policy for `valence`'s actor. A row
    /// the actor can't read returns `Ok(None)`, the same as a missing row, and fields
    /// whose own read policy hides them are left out of the returned object. Every other
    /// key keeps its stored shape, including `id`.
    /// # Errors
    ///
    /// [`Error::NotFound`] when `table` has no registered schema, and any storage or
    /// policy evaluation error.
    pub async fn get_record_json(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Option<serde_json::Value>> {
        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;

        let table_str = table.into();
        let schema = SchemaRegistry::lookup(&table_str)
            .ok_or_else(|| Error::NotFound(format!("Schema not found: {table_str}")))?;
        let Some(mut raw_data) =
            Self::get_stored_record_json(&table_str, id.as_ref(), valence, purpose).await?
        else {
            return Ok(None);
        };
        match PrivacyEvaluator::check_entity_read(schema, &raw_data, valence).await {
            Ok(()) => {}
            Err(Error::Privacy(_)) => return Ok(None),
            Err(e) => return Err(e),
        }
        let (_, hidden_fields) =
            PrivacyEvaluator::filter_entity_fields(schema, &raw_data, valence.actor())?;
        if let Some(obj) = raw_data.as_object_mut() {
            for field in &hidden_fields {
                obj.remove(field);
            }
        }
        Ok(Some(raw_data))
    }

    /// Get a stored record by ID with no privacy evaluation.
    ///
    /// Framework use only: generated CRUD, deletion and policy evaluation load the
    /// stored row with this and then authorize it themselves (Update, Delete, or the
    /// parent's policy for `defer_to_edge`). Callers showing data to an actor use
    /// [`Self::get_record_json`], [`Self::get_entity`] or [`crate::Model::get`].
    /// # Errors
    ///
    /// Returns an error when the backend can't be resolved or the read fails.
    #[doc(hidden)]
    pub async fn get_stored_record_json(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Option<serde_json::Value>> {
        let _ = purpose;
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
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<Vec<IdOnlyRecord>> {
        let table_str = table.into();

        // Use QueryCore to build the query with projection
        let query = QueryCore::new(table_str)
            .select(vec!["id".to_string()])
            .order_by("id".to_string(), SortDirection::Desc)
            .limit(limit);

        // Rows come from `execute_compiled_query` as JSON where Thing-shaped `id` values are
        // strings like `"counter:singleton"` — deserialize as `String`, then strip to id-only.
        let raw: Vec<IdOnlyRecord> = {
            query.execute(valence, purpose).await?
        };
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
    /// distinguish existence from a privacy denial. Telling them apart would let anyone who
    /// can guess ids learn which records exist, so this stays the default. Admin tooling
    /// that already lists record ids uses [`Self::get_entity_read`] instead.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_entity(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
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
        let raw_data = Self::get_stored_record_json(&table_str, id_str, valence, purpose).await?;
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

    /// Like [`Self::get_entity`], but tells a hidden record apart from a missing one.
    ///
    /// Returns [`EntityRead::Hidden`] with the name of the entity read policy that denied
    /// (see [`crate::privacy::PrivacyEvaluator::entity_read_denial`] for the names) and
    /// [`EntityRead::Missing`] when no row has `id`. On [`EntityRead::Found`], each hidden
    /// field also names its policy in
    /// [`ValenceEntity::hidden_field_policies`](crate::entity::ValenceEntity::hidden_field_policies).
    ///
    /// This reveals whether a record exists to an actor who may not read it. It is for
    /// admin tooling that already lists record ids, and `purpose` must say that the call
    /// reveals existence. Everything else uses [`Self::get_entity`].
    /// # Errors
    ///
    /// [`Error::NotFound`] when `table` has no registered schema, and any storage or
    /// policy evaluation error other than a denial.
    ///
    /// [`EntityRead::Hidden`]: crate::entity::EntityRead::Hidden
    /// [`EntityRead::Missing`]: crate::entity::EntityRead::Missing
    /// [`EntityRead::Found`]: crate::entity::EntityRead::Found
    pub async fn get_entity_read(
        table: impl Into<String>,
        id: impl AsRef<str>,
        valence: &Valence,
        purpose: crate::data_use::DataUsePurpose,
    ) -> Result<crate::entity::EntityRead> {
        use crate::entity::{EntityRead, ValenceEntity};
        use crate::privacy::PrivacyEvaluator;
        use crate::schema::SchemaRegistry;
        use std::sync::Arc;

        let table_str = table.into();
        let id_str = id.as_ref();
        let schema = SchemaRegistry::global()
            .get_schema(&table_str)
            .ok_or_else(|| Error::NotFound(format!("Schema not found: {table_str}")))?;

        let Some(raw_data) =
            Self::get_stored_record_json(&table_str, id_str, valence, purpose).await?
        else {
            return Ok(EntityRead::Missing);
        };

        if let Some(policy) = PrivacyEvaluator::entity_read_denial(schema, &raw_data, valence).await?
        {
            return Ok(EntityRead::Hidden { policy });
        }

        let (filtered_data, hidden_fields, hidden_field_policies) =
            PrivacyEvaluator::filter_entity_fields_with_policies(
                schema,
                &raw_data,
                valence.actor(),
            )?;

        Ok(EntityRead::Found(
            ValenceEntity::new(
                table_str.clone(),
                id_str.to_string(),
                filtered_data,
                hidden_fields,
                Arc::new((*schema).clone()),
            )
            .with_hidden_field_policies(hidden_field_policies),
        ))
    }



}
