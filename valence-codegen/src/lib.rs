//! Build-time code generation for Valence models from `valence_schema!` and `valence_trait_schema!` sources.
//!
//! # Pipeline
//!
//! 1. **Trait pass** — Collect `*_valence_trait.rs` (or your configured suffix), parse trait defs, emit
//!    shared trait definitions (see `generate_from_trait_file` in the internal `codegen` module).
//! 2. **Schema pass** — For each `*_valence_schema.rs`, merge trait fields/connections, validate, then
//!    emit structs, connections, CRUD, query builder, metadata, and trait impls into one
//!    `generated_models.rs` via [`generate_models`].
//!
//! # Where to read next
//!
//! - [`build`] / [`build_with`] — one-liner host `build.rs` helpers.
//! - Internal `codegen` module — per-schema orchestration and generator dispatch.
//! - `codegen::parser` — shared [`valence_schema_dsl`] parse + lower into generator IR.
//! - `codegen::generators` — `proc_macro2::TokenStream` builders for each emitted surface area.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

mod codegen;

pub use codegen::parser::ParsedTraitDef;

/// Default schema file suffix scanned by [`build`] / [`generate_models`].
pub const DEFAULT_SCHEMA_SUFFIX: &str = "_valence_schema.rs";

/// Default trait file suffix scanned by [`build`] / [`generate_models`].
pub const DEFAULT_TRAIT_SUFFIX: &str = "_valence_trait.rs";

/// Default schemas subdirectory under `CARGO_MANIFEST_DIR`.
pub const DEFAULT_SCHEMAS_SUBDIR: &str = "schemas";

/// Overrides for [`build_with`].
///
/// Most hosts call [`build`] and leave these at [`BuildOptions::default`].
pub struct BuildOptions<'a> {
    /// Subdirectory under the manifest dir that holds schema/trait files.
    pub schemas_subdir: &'a str,
    /// Schema file suffix (default [`DEFAULT_SCHEMA_SUFFIX`]).
    pub file_suffix: &'a str,
    /// Trait file suffix (default [`DEFAULT_TRAIT_SUFFIX`]).
    pub trait_file_suffix: &'a str,
    /// Override `CARGO_MANIFEST_DIR` (tests and custom roots).
    pub manifest_dir: Option<PathBuf>,
    /// Override `OUT_DIR` (tests and custom roots).
    pub out_dir: Option<PathBuf>,
}

impl Default for BuildOptions<'static> {
    fn default() -> Self {
        Self {
            schemas_subdir: DEFAULT_SCHEMAS_SUBDIR,
            file_suffix: DEFAULT_SCHEMA_SUFFIX,
            trait_file_suffix: DEFAULT_TRAIT_SUFFIX,
            manifest_dir: None,
            out_dir: None,
        }
    }
}

/// Scan `$CARGO_MANIFEST_DIR/schemas` and write `$OUT_DIR/generated_models.rs`.
///
/// # Examples
///
/// ```ignore
/// // build.rs
/// fn main() {
///     valence_codegen::build().expect("valence codegen failed");
/// }
/// ```
///
/// # Errors
///
/// Returns an error when env vars, schema paths, or codegen steps fail.
pub fn build() -> anyhow::Result<()> {
    build_with(BuildOptions::default())
}

/// Like [`build`], with path/suffix overrides.
///
/// # Examples
///
/// ```ignore
/// use valence_codegen::{build_with, BuildOptions};
///
/// build_with(BuildOptions {
///     schemas_subdir: "my_schemas",
///     ..BuildOptions::default()
/// })?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// # Errors
///
/// Returns an error when env vars, schema paths, or codegen steps fail.
pub fn build_with(options: BuildOptions<'_>) -> anyhow::Result<()> {
    let manifest_dir = match options.manifest_dir {
        Some(path) => path,
        None => PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR")
                .map_err(|_| anyhow::anyhow!("CARGO_MANIFEST_DIR is not set"))?,
        ),
    };
    let out_dir = match options.out_dir {
        Some(path) => path,
        None => PathBuf::from(
            std::env::var("OUT_DIR").map_err(|_| anyhow::anyhow!("OUT_DIR is not set"))?,
        ),
    };
    let schemas_dir = manifest_dir.join(options.schemas_subdir);
    #[allow(clippy::print_stdout)] // cargo build-script protocol
    {
        println!("cargo:rerun-if-changed={}", schemas_dir.display());
    }

    generate_models(&CodegenConfig {
        schemas_dir,
        out_dir,
        file_suffix: options.file_suffix,
        trait_file_suffix: options.trait_file_suffix,
    })
}

/// Configuration for model code generation from schema files
pub struct CodegenConfig<'a> {
    /// Directory containing schema files (files ending with `file_suffix`)
    pub schemas_dir: PathBuf,
    /// Output directory where generated_models.rs will be written
    pub out_dir: PathBuf,
    /// File suffix to match (e.g., "_valence_schema.rs")
    pub file_suffix: &'a str,
    /// File suffix for trait files (e.g., "_valence_trait.rs")
    pub trait_file_suffix: &'a str,
}

/// Generate model code from schema files in the configured directory
///
/// Two-pass pipeline:
///   Pass 1 – scan `*_valence_trait.rs` files, build a trait-definitions map.
///   Pass 2 – process `*_valence_schema.rs` files with trait context so that
///            trait fields/connections can be merged and trait impls generated.
///
/// # Errors
///
/// Returns an error when the schemas directory is missing or generation/write fails.
pub fn generate_models(config: &CodegenConfig<'_>) -> anyhow::Result<()> {
    validate_schemas_dir(&config.schemas_dir)?;

    // Pass 1: collect + generate trait definitions
    let trait_files = collect_files_with_suffix(&config.schemas_dir, config.trait_file_suffix)?;
    let trait_defs = build_trait_definitions(&trait_files)?;

    // Pass 2: collect + generate schema code (with trait context)
    let schema_files = collect_files_with_suffix(&config.schemas_dir, config.file_suffix)?;
    let generated_code = build_generated_code(&schema_files, &trait_files, &trait_defs);

    // Write
    write_generated_code(&config.out_dir, &generated_code)
}

/// Ensure the configured schemas directory exists before scanning.
fn validate_schemas_dir(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("Schemas directory does not exist: {}", path.display());
    }
    Ok(())
}

/// Collect `*.rs` paths under `dir` whose file stem ends with `suffix` (e.g. `_valence_schema`).
fn collect_files_with_suffix(dir: &Path, suffix: &str) -> anyhow::Result<Vec<PathBuf>> {
    use std::fs;

    let stem_suffix = suffix.strip_suffix(".rs").unwrap_or(suffix);

    let entries =
        fs::read_dir(dir).map_err(|e| anyhow::anyhow!("Failed to read schemas directory: {e}"))?;

    let mut paths = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        let matches_suffix = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.ends_with(stem_suffix));
        let is_rs = path.extension().is_some_and(|ext| ext == "rs");
        if matches_suffix && is_rs {
            paths.push(path);
        }
    }

    Ok(paths)
}

/// Parse all trait schema files into a map keyed by trait name.
fn build_trait_definitions(
    trait_files: &[PathBuf],
) -> anyhow::Result<HashMap<String, ParsedTraitDef>> {
    use std::fs;

    let mut map = HashMap::new();

    for path in trait_files {
        #[allow(clippy::print_stdout)] // cargo build-script protocol
        {
            println!("cargo:rerun-if-changed={}", path.display());
        }
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read trait file {}: {e}", path.display()))?;
        let def = codegen::parser::extract_trait_from_file(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse trait file {}: {e}", path.display()))?;
        map.insert(def.name.clone(), def);
    }

    Ok(map)
}

/// Concatenate formatted Rust source for all traits then all schemas (trait order first for type visibility).
fn build_generated_code(
    schema_files: &[PathBuf],
    trait_files: &[PathBuf],
    trait_defs: &HashMap<String, ParsedTraitDef>,
) -> String {
    let mut generated_code = String::new();

    // Header
    generated_code.push_str("// This file is auto-generated by build.rs\n");
    generated_code.push_str("// DO NOT EDIT MANUALLY\n\n");
    // Outer allows apply to the first item; prefer `include_generated_models!` module allows.
    generated_code.push_str("#[allow(dead_code)]\n");
    generated_code.push_str("#[allow(clippy::uninlined_format_args)]\n");
    generated_code.push_str("#[allow(clippy::single_match_else)]\n");
    generated_code.push_str("#[allow(clippy::unnecessary_trailing_comma)]\n");
    generated_code.push_str("#[allow(clippy::unused_async)]\n\n");

    // Emit trait definitions first (types must exist before schema impls)
    for path in trait_files {
        match codegen::generate_from_trait_file(path) {
            Ok(code) => {
                generated_code.push_str(&code);
                generated_code.push_str("\n\n");
            }
            Err(e) => {
                #[allow(clippy::print_stderr)] // cargo build-script protocol
                {
                    eprintln!(
                        "cargo:warning=Failed to generate trait code for {}: {e}",
                        path.display()
                    );
                }
            }
        }
    }

    // Emit schema code
    for path in schema_files {
        #[allow(clippy::print_stdout)] // cargo build-script protocol
        {
            println!("cargo:rerun-if-changed={}", path.display());
        }

        match codegen::generate_from_schema_file(path, trait_defs) {
            Ok(code) => {
                generated_code.push_str(&code);
                generated_code.push_str("\n\n");
            }
            Err(e) => {
                #[allow(clippy::print_stderr)] // cargo build-script protocol
                {
                    eprintln!(
                        "cargo:warning=Failed to generate code for {}: {e}",
                        path.display()
                    );
                }
            }
        }
    }

    generated_code
}

/// Write `generated_models.rs` into the configured output directory (typically `OUT_DIR`).
fn write_generated_code(out_dir: &Path, generated_code: &str) -> anyhow::Result<()> {
    use std::fs;

    let dest_path = out_dir.join("generated_models.rs");
    fs::write(&dest_path, generated_code)
        .map_err(|e| anyhow::anyhow!("Failed to write generated code: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod build_defaults_tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        build, build_with, BuildOptions, DEFAULT_SCHEMAS_SUBDIR, DEFAULT_SCHEMA_SUFFIX,
        DEFAULT_TRAIT_SUFFIX,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    const MINIMAL_SCHEMA: &str = r#"
valence_schema! {
    Widget {
        table: "widget",
        version: "0.1.0",
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            name: { r#type: FieldType::String, required: true },
        ],
    }
}
"#;

    struct TempRoot {
        path: PathBuf,
    }

    impl TempRoot {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("valence-codegen-{label}-{nanos}-{seq}"));
            fs::create_dir_all(&path).expect("create temp root");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn build_with_writes_models_for_valid_schema() {
        let root = TempRoot::new("valid");
        let schemas = root.path().join("schemas");
        fs::create_dir_all(&schemas).expect("schemas dir");
        fs::write(schemas.join("widget_valence_schema.rs"), MINIMAL_SCHEMA).expect("schema");
        let out = root.path().join("out");
        fs::create_dir_all(&out).expect("out dir");

        build_with(BuildOptions {
            manifest_dir: Some(root.path().to_path_buf()),
            out_dir: Some(out.clone()),
            ..BuildOptions::default()
        })
        .expect("codegen should succeed");

        let generated = fs::read_to_string(out.join("generated_models.rs")).expect("read");
        assert!(generated.contains("Widget") || generated.contains("widget"));
        assert!(generated.contains("impl") && generated.contains("Model"));
    }

    #[test]
    fn build_with_empty_schemas_dir_writes_header_only() {
        let root = TempRoot::new("empty");
        fs::create_dir_all(root.path().join("schemas")).expect("schemas dir");
        let out = root.path().join("out");
        fs::create_dir_all(&out).expect("out dir");

        build_with(BuildOptions {
            manifest_dir: Some(root.path().to_path_buf()),
            out_dir: Some(out.clone()),
            ..BuildOptions::default()
        })
        .expect("empty schemas dir should succeed");

        let generated = fs::read_to_string(out.join("generated_models.rs")).expect("read");
        assert!(generated.contains("auto-generated"));
    }

    #[test]
    fn build_with_missing_schemas_dir_errors() {
        let root = TempRoot::new("missing");
        let out = root.path().join("out");
        fs::create_dir_all(&out).expect("out dir");

        let err = build_with(BuildOptions {
            manifest_dir: Some(root.path().to_path_buf()),
            out_dir: Some(out),
            ..BuildOptions::default()
        })
        .expect_err("missing schemas dir should fail");

        assert!(format!("{err:#}").contains("Schemas directory does not exist"));
    }

    #[test]
    fn build_errors_when_manifest_dir_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let saved_manifest = std::env::var_os("CARGO_MANIFEST_DIR");
        let saved_out = std::env::var_os("OUT_DIR");
        std::env::remove_var("CARGO_MANIFEST_DIR");
        std::env::set_var("OUT_DIR", "/tmp/valence-codegen-test-out");

        let err = build().expect_err("missing CARGO_MANIFEST_DIR should fail");
        assert!(format!("{err:#}").contains("CARGO_MANIFEST_DIR is not set"));

        restore_env("CARGO_MANIFEST_DIR", saved_manifest);
        restore_env("OUT_DIR", saved_out);
    }

    #[test]
    fn build_errors_when_out_dir_unset() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let saved_manifest = std::env::var_os("CARGO_MANIFEST_DIR");
        let saved_out = std::env::var_os("OUT_DIR");
        std::env::set_var("CARGO_MANIFEST_DIR", "/tmp/valence-codegen-test-manifest");
        std::env::remove_var("OUT_DIR");

        let err = build().expect_err("missing OUT_DIR should fail");
        assert!(format!("{err:#}").contains("OUT_DIR is not set"));

        restore_env("CARGO_MANIFEST_DIR", saved_manifest);
        restore_env("OUT_DIR", saved_out);
    }

    #[test]
    fn build_reads_env_and_writes_models() {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let root = TempRoot::new("env");
        let schemas = root.path().join("schemas");
        fs::create_dir_all(&schemas).expect("schemas dir");
        fs::write(schemas.join("widget_valence_schema.rs"), MINIMAL_SCHEMA).expect("schema");
        let out = root.path().join("out");
        fs::create_dir_all(&out).expect("out dir");

        let saved_manifest = std::env::var_os("CARGO_MANIFEST_DIR");
        let saved_out = std::env::var_os("OUT_DIR");
        std::env::set_var("CARGO_MANIFEST_DIR", root.path());
        std::env::set_var("OUT_DIR", &out);

        build().expect("build from env should succeed");
        let generated = fs::read_to_string(out.join("generated_models.rs")).expect("read");
        assert!(generated.contains("Widget") || generated.contains("widget"));

        restore_env("CARGO_MANIFEST_DIR", saved_manifest);
        restore_env("OUT_DIR", saved_out);
    }

    #[test]
    fn cli_default_suffix_constants_match_build_options() {
        let defaults = BuildOptions::default();
        assert_eq!(defaults.file_suffix, DEFAULT_SCHEMA_SUFFIX);
        assert_eq!(defaults.trait_file_suffix, DEFAULT_TRAIT_SUFFIX);
        assert_eq!(defaults.schemas_subdir, DEFAULT_SCHEMAS_SUBDIR);
    }
}
