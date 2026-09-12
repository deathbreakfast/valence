use valence::prelude::*;

valence_schema! {
    Project {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_chain_project",
        version: "0.1.0",
        database: crate::PROJECT_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
            org: { r#type: FieldType::Record("hop_chain_org"), required: true },
        ],
        connections: [
            org: {
                table: "hop_chain_org",
                on_delete: Cascade,
            },
            tasks: {
                table: "hop_chain_task",
                cardinality: HasMany,
                reverse_field: "project",
                on_delete: Cascade,
            },
        ],
    }
}
