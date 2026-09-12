//! Generated `impl Model` against the `valence` crate.
//!
//! End-to-end proof: `cargo test -p codegen-host`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
valence::include_generated_models!();

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use valence::{use_, Actor, InMemoryBackend, Model, Valence};

    use super::Widget;

    #[tokio::test]
    async fn generated_widget_impl_model_compiles_and_runs() {
        // Step 1 — Boot Valence with mem backend + System actor (generated CRUD expects actor context).
        let valence = Valence::builder()
            .add_backend("default", Arc::new(InMemoryBackend::new()))
            .with_actor(Actor::System {
                operation: "codegen_host_compile".into(),
            })
            .build()
            .expect("build");

        // Step 2 — Create: `Widget` is generated from schemas/widget_valence_schema.rs via build.rs.
        let widget = Widget::new("demo".to_string()).expect("new");
        let created = Widget::create_used(
            widget,
            &valence,
            use_!("Create the demo widget row for the codegen-host compile check."),
        )
        .await
        .expect("create");
        assert_eq!(created.name(), "demo");
        let id = created.id().expect("id").id();

        // Step 3 — Read back the persisted row.
        let fetched = Widget::get_used(
            id,
            &valence,
            use_!("Reload the demo widget to confirm create persisted."),
        )
        .await
        .expect("get");
        assert!(fetched.is_some());

        // Step 4 — Partial update via JSON merge patch.
        let patch = serde_json::json!({ "name": "updated" });
        let merged = Widget::merge_used(
            id,
            patch,
            &valence,
            use_!("Rename the demo widget through merge for the compile check."),
        )
        .await
        .expect("merge");
        assert_eq!(merged.name(), "updated");
    }
}
