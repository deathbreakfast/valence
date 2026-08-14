use valence::prelude::*;

valence_schema! {
    TypedProbe {
        table: "typed_probe",
        version: "0.1.0",
        policies: {
            read: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            create: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            update: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
            delete: { allow: [valence::privacy_policies::common::PUBLIC_READ] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            label: { r#type: FieldType::String, required: true },
            at: { r#type: FieldType::DateTime, required: true },
            price: { r#type: FieldType::Currency, required: true },
            payload: { r#type: FieldType::JsonAs("crate::ProbePayload"), required: true },
        ],
    }
}
