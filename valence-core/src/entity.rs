//! Privacy-filtered entity view for admin tooling.

use crate::schema::SchemaMetadata;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ValenceEntity {
    pub table: String,
    pub id: String,
    pub data: BTreeMap<String, serde_json::Value>,
    pub hidden_fields: Vec<String>,
    /// For each hidden field, the name of the read policy that hid it.
    ///
    /// Filled by [`crate::query::QueryCore::get_entity_read`] only; empty otherwise.
    pub hidden_field_policies: BTreeMap<String, String>,
    pub schema: Arc<SchemaMetadata>,
}

impl ValenceEntity {
    #[must_use]
    pub fn new(
        table: String,
        id: String,
        data: BTreeMap<String, serde_json::Value>,
        hidden_fields: Vec<String>,
        schema: Arc<SchemaMetadata>,
    ) -> Self {
        Self {
            table,
            id,
            data,
            hidden_fields,
            hidden_field_policies: BTreeMap::new(),
            schema,
        }
    }

    /// Set [`Self::hidden_field_policies`] (field name to read policy name).
    #[must_use]
    pub fn with_hidden_field_policies(mut self, policies: BTreeMap<String, String>) -> Self {
        self.hidden_field_policies = policies;
        self
    }
}

/// Result of [`crate::query::QueryCore::get_entity_read`]: tells a record the actor may not
/// read apart from one that doesn't exist.
#[derive(Debug, Clone)]
pub enum EntityRead {
    /// The actor may read the record; fields hidden from it are listed on the entity.
    Found(ValenceEntity),
    /// The record exists, and the named entity read policy denies this actor.
    Hidden {
        /// Name of the read policy that denied.
        policy: String,
    },
    /// No record has this id.
    Missing,
}
