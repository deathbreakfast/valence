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
            schema,
        }
    }
}
