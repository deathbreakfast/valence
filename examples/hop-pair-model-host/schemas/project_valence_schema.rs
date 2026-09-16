use valence::prelude::*;

valence_schema! {
    Project {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_pair_project",
        version: "0.1.0",
        database: crate::PROJECT_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            tasks: {
                table: "hop_pair_task",
                cardinality: HasMany,
                reverse_field: "project",
                on_delete: Cascade,
            },
        ],
    }
}
