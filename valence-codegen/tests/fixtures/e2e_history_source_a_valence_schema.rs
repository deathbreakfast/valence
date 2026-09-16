use valence::prelude::*;

valence_schema! {
    E2eHistorySourceA {
        repository: "https://github.com/unified-field-dev/valence",
        table: "e2e_history_source_a",
        version: "0.1.0",
        traits: [HistorySource, RecordHistory],
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            label: { r#type: FieldType::String, required: true },
        ],
    }
}
