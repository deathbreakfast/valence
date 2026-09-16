use valence::prelude::*;

valence_schema! {
    Widget {
        repository: "https://github.com/unified-field-dev/valence",
        table: "widget",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
