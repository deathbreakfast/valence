use valence::prelude::*;

// Test fixture for stored-row update checks: `admin_notes` is readable and updatable by
// system actors only, so an owner never sees it and must still be able to edit `title`.
// `user` is a bare String id with no connection or `on_delete`. Do not copy it as a model
// for user-owned data.
valence_schema! {
    AdminNote {
        repository: "https://github.com/unified-field-dev/valence",
        table: "admin_note",
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
            title: { r#type: FieldType::String, required: true },
            admin_notes: {
                r#type: FieldType::String,
                policies: {
                    read: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
                    update: { allow: [valence::privacy_policies::common::SYSTEM_ONLY] },
                },
            },
        ],
    }
}
