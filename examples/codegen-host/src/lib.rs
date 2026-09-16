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
            valence::use_!(r"When the **codegen-host** compile check runs, we **create a demo widget** so generated Model create can prove the schema built correctly. Developers running that example suite use this row."),
        )
        .await
        .expect("create");
        assert_eq!(created.name(), "demo");
        let id = created.id().expect("id").id();

        // Step 3 — Read back the persisted row.
        let fetched = Widget::get_used(
            id,
            &valence,
            valence::use_!(r"After create, we **reload the demo widget by id** so the codegen-host suite can confirm the generated get path returned the row. Developers running that example suite use this result."),
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
            valence::use_!(r"During the **codegen-host** compile check, we **merge a new name onto the widget** so generated merge can prove partial updates work. Developers running that example suite use this result."),
        )
        .await
        .expect("merge");
        assert_eq!(merged.name(), "updated");
    }
}
