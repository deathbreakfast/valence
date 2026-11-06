use valence::prelude::*;

// Test fixture for `edge_policy`: any signed-in user reads a doc, only its owner updates it,
// so only the owner may link or unlink labels. `user` is a bare String id with no connection
// or `on_delete`. Do not copy it as a model for user-owned data.
valence_schema! {
    EdgeDoc {
        repository: "https://github.com/unified-field-dev/valence",
        table: "edge_doc",
        version: "0.1.0",
        policies: {
            read: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            create: { allow: [valence::privacy_policies::common::AUTHENTICATED] },
            update: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
            delete: { allow: [valence::privacy_policies::owner::OWNER_BY_USER_FIELD] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            user: { r#type: FieldType::String, required: true },
        ],
        connections: [
            labels: {
                table: "edge_label",
                cardinality: ManyToMany,
                edge_table: "edge_doc_label",
                edge_policy: SourceUpdateTargetRead,
                on_delete: Cascade,
                model: "crate::generated::EdgeLabel",
            },
        ],
    }
}
