//! Compile check: `valence_schema!` against public crate types.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence::prelude::*;
use valence::privacy_policies::common::PUBLIC_READ;

valence_schema! {
    Smoke {
        table: "smoke",
        version: "0.1.0",
        // Explicit policies: empty entity policy lists default-deny non-System actors.
        policies: {
            read: { allow: [PUBLIC_READ] },
            create: { allow: [PUBLIC_READ] },
            update: { allow: [PUBLIC_READ] },
            delete: { allow: [PUBLIC_READ] },
        },
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use valence::{InMemoryBackend, Valence};

    #[test]
    fn schema_metadata_registers() {
        let found = valence::inventory::iter::<valence::SchemaMetadataInit>
            .into_iter()
            .next();
        assert!(found.is_some());
    }

    #[tokio::test]
    async fn valence_builds_with_mem_backend() {
        let valence = Valence::builder()
            .add_backend("default", Arc::new(InMemoryBackend::new()))
            .build()
            .expect("build");
        assert!(valence.active_backend().is_ok());
    }
}
