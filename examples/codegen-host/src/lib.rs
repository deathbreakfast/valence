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

    use valence::{Actor, InMemoryBackend, Model, Valence};

    use super::Widget;

    #[tokio::test]
    async fn generated_widget_impl_model_compiles_and_runs() {
        let valence = Valence::builder()
            .add_backend("default", Arc::new(InMemoryBackend::new()))
            .with_actor(Actor::System {
                operation: "codegen_host_compile".into(),
            })
            .build()
            .expect("build");

        let widget = Widget::new("demo".to_string()).expect("new");
        let created = Widget::create(widget, &valence).await.expect("create");
        assert_eq!(created.name(), "demo");
        let id = created.id().expect("id").id();

        let fetched = Widget::get(id, &valence).await.expect("get");
        assert!(fetched.is_some());

        let patch = serde_json::json!({ "name": "updated" });
        let merged = Widget::merge(id, patch, &valence).await.expect("merge");
        assert_eq!(merged.name(), "updated");
    }
}
