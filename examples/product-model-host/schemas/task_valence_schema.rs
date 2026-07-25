use valence::prelude::*;

valence_schema! {
    Task {
        table: "task",
        version: "0.1.0",
        policies: {
            read: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            create: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            update: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            delete: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            title: { r#type: FieldType::String, required: true },
            project: { r#type: FieldType::Record("project"), required: true },
        ],
        connections: [
            project: {
                table: "project",
                on_delete: Cascade,
                model: "crate::generated::Project",
            },
        ],
    }
}
