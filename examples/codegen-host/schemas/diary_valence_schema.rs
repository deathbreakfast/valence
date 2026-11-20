use valence::prelude::*;

valence_schema! {
    Diary {
        repository: "https://github.com/unified-field-dev/valence",
        table: "diary",
        version: "0.1.0",
        description: "Users write diary entries for themselves; only the owner reads, edits or deletes one",
        policies: {
            read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            create: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            body: { r#type: FieldType::String, required: true },
        ],
    }
}
