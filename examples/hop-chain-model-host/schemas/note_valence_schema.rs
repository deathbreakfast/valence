use valence::prelude::*;

valence_schema! {
    Note {
        repository: "https://github.com/unified-field-dev/valence",
        table: "hop_chain_note",
        version: "0.1.0",
        database: crate::NOTE_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            body: { r#type: FieldType::String, required: true },
            task: { r#type: FieldType::Record("hop_chain_task"), required: true },
        ],
        connections: [
            task: {
                table: "hop_chain_task",
                on_delete: Cascade,
            },
        ],
    }
}
