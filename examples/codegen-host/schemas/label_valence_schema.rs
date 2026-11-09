use valence::prelude::*;

valence_schema! {
    Label {
        repository: "https://github.com/unified-field-dev/valence",
        table: "label",
        version: "0.1.0",
        description: "Label a note can be tagged with",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            delete: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
