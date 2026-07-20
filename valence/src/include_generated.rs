//! Host helpers for including `valence-codegen` build artifacts.

/// Include `$OUT_DIR/generated_models.rs` produced by `valence_codegen::build`.
///
/// Expands to a `generated` module plus `pub use generated::*`. Invoke once at
/// crate root (or another module that should re-export models):
///
/// ```ignore
/// valence::include_generated_models!();
/// ```
///
/// Schema DSL files under `schemas/` are scan inputs for codegen; they are not
/// linked via `mod`. The generated module must be linked into the binary so
/// `inventory` registrations are visible.
#[macro_export]
macro_rules! include_generated_models {
    () => {
        pub mod generated {
            #![allow(
                dead_code,
                unused_imports,
                clippy::uninlined_format_args,
                clippy::single_match_else,
                clippy::unnecessary_trailing_comma,
                clippy::unused_async,
                clippy::elidable_lifetime_names,
                clippy::missing_errors_doc,
                clippy::return_self_not_must_use
            )]
            include!(concat!(env!("OUT_DIR"), "/generated_models.rs"));
        }
        pub use generated::*;
    };
}
