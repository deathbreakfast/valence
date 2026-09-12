use valence::prelude::*;

valence_schema! {
    Org {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_chain_org",
        version: "0.1.0",
        database: crate::ORG_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            projects: {
                table: "hop_chain_project",
                cardinality: HasMany,
                reverse_field: "org",
                on_delete: Cascade,
            },
        ],
    }
}
