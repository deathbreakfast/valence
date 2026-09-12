use valence::prelude::*;

valence_schema! {
    Task {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_chain_task",
        version: "0.1.0",
        database: crate::TASK_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
            project: { r#type: FieldType::Record("hop_chain_project"), required: true },
        ],
        connections: [
            project: {
                table: "hop_chain_project",
                on_delete: Cascade,
            },
            notes: {
                table: "hop_chain_note",
                cardinality: HasMany,
                reverse_field: "task",
                on_delete: Cascade,
            },
        ],
    }
}
