use valence::prelude::*;

valence_schema! {
    Task {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_pair_task",
        version: "0.1.0",
        database: crate::TASK_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
            project: { r#type: FieldType::Record("hop_pair_project"), required: true },
        ],
        connections: [
            project: {
                table: "hop_pair_project",
                on_delete: Cascade,
            },
        ],
    }
}
