use valence::prelude::*;

// Test fixture for `fetch_with_stats`: `user` is a bare String id with no connection or
// `on_delete`. Do not copy it as a model for user-owned data.
valence_schema! {
    OwnedNote {
        repository: "https://github.com/unified-field-dev/valence",
        table: "owned_note",
        version: "0.1.0",
        policies: {
            read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
        ],
    }
}
