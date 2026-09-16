use valence::prelude::*;

valence_trait_schema! {
    RecordHistory {
        repository: "https://github.com/unified-field-dev/valence",
        fields: [
            history_id: { r#type: FieldType::String, required: true },
        ],
    }
}
