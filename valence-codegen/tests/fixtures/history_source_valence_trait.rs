use valence::prelude::*;

valence_trait_schema! {
    HistorySource {
        repository: "https://github.com/unified-field-dev/valence",
        fields: [
            record_history_table: { r#type: FieldType::String, required: true },
        ],
    }
}
