use valence::prelude::*;

valence_schema! {
    Note {
        repository: "https://github.com/unified-field-dev/valence",
        table: "note",
        version: "0.1.0",
        description: "Signed-in users read notes; only the owner edits one or changes its labels",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
            title: { r#type: FieldType::String, required: true },
        ],
        connections: [
            labels: {
                table: "label",
                cardinality: ManyToMany,
                edge_table: "note_label",
                on_delete: Cascade,
                model: "crate::Label",
            },
        ],
    }
}
