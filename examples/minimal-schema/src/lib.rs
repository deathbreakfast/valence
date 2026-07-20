//! Compile check: `valence_schema!` against facade types.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence::prelude::*;

valence_schema! {
    Smoke {
        table: "smoke",
        version: "0.1.0",
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
