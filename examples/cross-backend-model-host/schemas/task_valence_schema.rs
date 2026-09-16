use valence::prelude::*;

valence_schema! {
    Task {
        repository: "https://github.com/unified-field-dev/valence",
        table: "xb_task",
        version: "0.1.0",
        database: crate::TASK_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
            project: { r#type: FieldType::Record("xb_project"), required: true },
        ],
        connections: [
            project: {
                table: "xb_project",
                on_delete: Cascade,
            },
        ],
    }
}
