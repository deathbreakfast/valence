//! Network-connected SurrealDB backend (`Surreal<Any>`).

#[cfg(feature = "remote")]
mod imp {
    use std::any::Any;

    use surrealdb::engine::any::Any as AnyEngine;
    use surrealdb::types::Value as SurrealValueType;
    use surrealdb::Surreal;

    use valence_core::backend::{BackendCapabilities, DatabaseBackend};
    use valence_core::compiled_query::CompiledQuery;
    use valence_core::error::{Error, Result};
    use valence_core::record_id::RecordId;
    use valence_core::ttl::{BackendTtlCapability, SchemaTtlPolicy, EXPIRE_AT_FIELD};

    use crate::embedded::{
        row_json_after_create, strip_id_from_content, surreal_capabilities, ENGINE_ID,
    };
    use crate::error::db_err;
    use crate::query_exec::execute_compiled_query_inner;
    use crate::record_id::{surreal_from_valence, valence_from_surreal};
    use crate::row_json::{
        ensure_schemaless_table, ensure_typed_table, json_to_surreal_content_value,
        select_record_json, sync_typed_table,
        thing_to_id_only,
    };

    /// Wraps a remote-capable [`Surreal<Any>`] client (WebSocket / HTTP / …).
    #[derive(Debug, Clone)]
    pub struct SurrealRemoteBackend {
        db: Surreal<AnyEngine>,
    }

    impl SurrealRemoteBackend {
        /// Wrap an existing remote Surreal client.
        pub fn new(db: Surreal<AnyEngine>) -> Self {
            Self { db }
        }

        /// Borrow the underlying Surreal client.
        pub fn inner(&self) -> &Surreal<AnyEngine> {
            &self.db
        }

        /// Consume the adapter and return the Surreal client.
        pub fn into_inner(self) -> Surreal<AnyEngine> {
            self.db
        }
    }

    #[async_trait::async_trait]
    impl DatabaseBackend for SurrealRemoteBackend {
        fn engine_id(&self) -> &'static str {
            ENGINE_ID
        }

        fn capabilities(&self) -> BackendCapabilities {
            surreal_capabilities()
        }

        fn as_any_local(&self) -> Option<&dyn Any> {
            None
        }

        async fn use_namespace(&self, ns: &str, db_name: &str) -> Result<()> {
            self.db.use_ns(ns).use_db(db_name).await.map_err(db_err)?;
            Ok(())
        }

        async fn execute_compiled_query(
            &self,
            compiled: &CompiledQuery,
        ) -> Result<Vec<serde_json::Value>> {
            execute_compiled_query_inner(&self.db, &compiled.query_string, &compiled.params).await
        }

        async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
            ensure_schemaless_table(&self.db, table).await?;
            select_record_json(&self.db, table, id).await
        }

        async fn create_record(
            &self,
            table: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            ensure_schemaless_table(&self.db, table).await?;
            let mut content = content;
            valence_core::ttl::prepare_create_content(table, self, &mut content)?;
            let explicit_id = content
                .get("id")
                .and_then(|v| {
                    v.as_str().map(str::to_string).or_else(|| {
                        v.as_object()
                            .and_then(|o| o.get("id"))
                            .and_then(|x| x.as_str())
                            .map(str::to_string)
                    })
                })
                .map(thing_to_id_only)
                .filter(|s| !s.is_empty());
            let json_content = strip_id_from_content(content);
            let resource = match explicit_id.as_deref() {
                Some(id) => surrealdb::opt::Resource::from((table, id)),
                None => surrealdb::opt::Resource::from(table),
            };
            let surreal_content = json_to_surreal_content_value(json_content);
            let raw: SurrealValueType = self
                .db
                .create(resource)
                .content(surreal_content)
                .await
                .map_err(db_err)?;

            if let Some(id_for_get) = explicit_id {
                return select_record_json(&self.db, table, &id_for_get)
                    .await?
                    .ok_or_else(|| Error::Validation("Failed to read record after create".into()));
            }

            row_json_after_create(&self.db, table, raw).await
        }

        async fn update_record(
            &self,
            table: &str,
            id: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            ensure_schemaless_table(&self.db, table).await?;
            let resource = surrealdb::opt::Resource::from((table, id));
            let content = json_to_surreal_content_value(strip_id_from_content(content));
            let _: SurrealValueType = self
                .db
                .update(resource)
                .content(content)
                .await
                .map_err(db_err)?;
            select_record_json(&self.db, table, id)
                .await?
                .ok_or_else(|| Error::Validation("Failed to read record after update".into()))
        }

        async fn merge_record(
            &self,
            table: &str,
            id: &str,
            patch: serde_json::Value,
        ) -> Result<serde_json::Value> {
            ensure_schemaless_table(&self.db, table).await?;
            let resource = surrealdb::opt::Resource::from((table, id));
            let patch = json_to_surreal_content_value(strip_id_from_content(patch));
            let _: SurrealValueType = self
                .db
                .update(resource)
                .merge(patch)
                .await
                .map_err(db_err)?;
            select_record_json(&self.db, table, id)
                .await?
                .ok_or_else(|| Error::Validation("Failed to read record after merge".into()))
        }

        async fn upsert_record(
            &self,
            table: &str,
            id: &str,
            content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            ensure_schemaless_table(&self.db, table).await?;
            let resource = surrealdb::opt::Resource::from((table, id));
            let content = json_to_surreal_content_value(strip_id_from_content(content));
            let _: SurrealValueType = self
                .db
                .upsert(resource)
                .content(content)
                .await
                .map_err(db_err)?;
            select_record_json(&self.db, table, id)
                .await?
                .ok_or_else(|| Error::Validation("Failed to read record after upsert".into()))
        }

        async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
            let resource = surrealdb::opt::Resource::from((table, id));
            let _: SurrealValueType = self.db.delete(resource).await.map_err(db_err)?;
            Ok(())
        }

        async fn relate_edge(
            &self,
            from: &RecordId,
            edge_table: &str,
            to: &RecordId,
        ) -> Result<()> {
            let from_t = surreal_from_valence(from);
            let to_t = surreal_from_valence(to);
            let q = format!("RELATE $from->{}->$to RETURN NONE", edge_table);
            ensure_schemaless_table(&self.db, edge_table).await?;
            self.db
                .query(&q)
                .bind(("from", from_t))
                .bind(("to", to_t))
                .await
                .map_err(db_err)?;
            Ok(())
        }

        async fn unrelate_edge(
            &self,
            from: &RecordId,
            edge_table: &str,
            to: &RecordId,
        ) -> Result<()> {
            let from_t = surreal_from_valence(from);
            let to_t = surreal_from_valence(to);
            let q = format!("DELETE $from->{} WHERE `out` = $to RETURN NONE", edge_table);
            ensure_schemaless_table(&self.db, edge_table).await?;
            self.db
                .query(&q)
                .bind(("from", from_t))
                .bind(("to", to_t))
                .await
                .map_err(db_err)?;
            Ok(())
        }

        async fn get_edge_targets(
            &self,
            from: &RecordId,
            edge_table: &str,
        ) -> Result<Vec<RecordId>> {
            use crate::query_exec::query_err_is_missing_table;

            let from_t = surreal_from_valence(from);
            let q = format!("SELECT VALUE `out` FROM {edge_table} WHERE `in` = $from");
            let mut response = match self.db.query(&q).bind(("from", from_t)).await {
                Ok(r) => r,
                Err(e) if query_err_is_missing_table(&e.to_string()) => {
                    return Ok(vec![]);
                }
                Err(e) => return Err(db_err(e)),
            };
            let outs: Vec<surrealdb::types::RecordId> = match response.take(0) {
                Ok(r) => r,
                Err(e) if query_err_is_missing_table(&e.to_string()) => {
                    return Ok(vec![]);
                }
                Err(e) => return Err(db_err(e)),
            };
            Ok(outs.into_iter().map(valence_from_surreal).collect())
        }

        async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
            ensure_schemaless_table(&self.db, table).await
        }

        async fn ensure_typed_table(
            &self,
            layout: &valence_core::storage_layout::StorageLayout,
        ) -> Result<()> {
            ensure_typed_table(&self.db, layout).await
        }

        async fn sync_typed_table(
            &self,
            layout: &valence_core::storage_layout::StorageLayout,
        ) -> Result<()> {
            sync_typed_table(&self.db, layout).await
        }

        async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
            ensure_schemaless_table(&self.db, table).await?;
            let index_name = format!("idx_{}_{}_unique", table, field);
            let query = format!(
                "DEFINE INDEX {} ON TABLE {} COLUMNS {} UNIQUE",
                index_name, table, field
            );
            match self.db.query(&query).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    let message = e.to_string().to_lowercase();
                    if message.contains("already") && message.contains("index") {
                        Ok(())
                    } else {
                        Err(db_err(e))
                    }
                }
            }
        }

        fn ttl_capability(&self) -> BackendTtlCapability {
            BackendTtlCapability::Deferred
        }

        async fn apply_ttl_policy(&self, table: &str, _policy: &SchemaTtlPolicy) -> Result<()> {
            ensure_schemaless_table(&self.db, table).await?;
            let field = EXPIRE_AT_FIELD;
            valence_core::safe_ident::assert_safe_ident(field)?;
            let index_name = format!("valence_ttl_expire_at_{table}");
            let query = format!("DEFINE INDEX {index_name} ON TABLE {table} COLUMNS {field}");
            match self.db.query(&query).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    let message = e.to_string().to_lowercase();
                    if message.contains("already") && message.contains("index") {
                        Ok(())
                    } else {
                        Err(db_err(e))
                    }
                }
            }
        }
    }
}

#[cfg(feature = "remote")]
pub use imp::SurrealRemoteBackend;
