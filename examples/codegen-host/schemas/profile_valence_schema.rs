use valence::prelude::*;

valence_schema! {
    Profile {
        repository: "https://github.com/unified-field-dev/valence",
        table: "profile",
        version: "0.1.0",
        description: "Signed-in users read and edit any profile; only the owner reads legal_name",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            delete: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            legal_name: {
                r#type: FieldType::String,
                required: true,
                validations: [Validator::NonEmpty],
                policies: {
                    read: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
                },
            },
            display_name: { r#type: FieldType::String, required: true },
        ],
    }
}
