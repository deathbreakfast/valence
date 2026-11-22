use valence::prelude::*;

valence_schema! {
    Account {
        repository: "https://github.com/unified-field-dev/valence",
        table: "account",
        version: "0.1.0",
        description: "Signed-in users read and edit any account; only the owner reads password_hash",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            delete: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            display_name: { r#type: FieldType::String, required: true },
            password_hash: {
                r#type: FieldType::String,
                required: false,
                policies: {
                    read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
                },
            },
        ],
    }
}
