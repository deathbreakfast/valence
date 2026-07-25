use valence::prelude::*;

valence_schema! {
    Project {
        table: "project",
        version: "0.1.0",
        policies: {
            read: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            create: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            update: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            delete: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
        connections: [
            tasks: {
                table: "task",
                cardinality: HasMany,
                reverse_field: "project",
                on_delete: Cascade,
                model: "crate::generated::Task",
            },
        ],
    }
}
