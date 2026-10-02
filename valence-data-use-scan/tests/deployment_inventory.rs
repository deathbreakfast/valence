//! Host deployment scans against a fixture workspace.
//!
//! Layout (all path dependencies, so tests run offline):
//!
//! ```text
//! host/                 (.git)  workspace: server, worker, host-e2e
//!   server/             host package; links product-worker, noval, widget (optional)
//!                       [target.'cfg(any())'.dependencies] worker (features = ["extra"])
//!   worker/             lib + bin, inventory only; compile_error! proves it never builds
//!   host-e2e/           workspace member the server never names
//! product-repo/         (.cargo-ok) product-worker, a product crate from another repo
//! extra-repo/           worker-extra, optional dep of worker behind `extra`
//! widget-repo/          widget, optional dep of server behind `with-widget`
//! noval-repo/           noval, linked but never touches Valence
//! valence-stub/         uf-valence stand-in
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use valence_data_use_scan::{
    collect_hits, generate, Config, ConnectionEdge, DataUseScanError, HostPackage, ScanHit,
    TargetKind,
};

const PROD_LIB: &str = include_str!("fixtures/prod_lib.rs");
const TEST_TWIN: &str = include_str!("fixtures/test_twin.rs");

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fx = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fx.write_standard();
        fx
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn host(&self) -> PathBuf {
        self.root().join("host")
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.root().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn package(&self, dir: &str, name: &str, extra: &str, own_workspace: bool) {
        let ws = if own_workspace { "\n[workspace]\n" } else { "" };
        self.write(
            &format!("{dir}/Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrepository = \"https://github.com/acme/{name}\"\n{ws}\n{extra}"
            ),
        );
    }

    fn write_standard(&self) {
        let valence = "uf-valence = { path = \"../../valence-stub\" }";

        self.package("valence-stub", "uf-valence", "", true);
        self.write("valence-stub/src/lib.rs", "//! stub\n");

        self.write("product-repo/.cargo-ok", "");
        self.package(
            "product-repo/product-worker",
            "product-worker",
            &format!("[dependencies]\n{valence}\n"),
            true,
        );
        self.write(
            "product-repo/product-worker/src/lib.rs",
            &format!(
                "#[cfg(any())]\nmod uses {{\n{PROD_LIB}\n\nasync fn widget() {{\n    let _ = Widget::get(\"id\", &valence, valence::use_!(\"Product worker reads the widget.\")).await;\n}}\n}}\n"
            ),
        );
        self.write(
            "product-repo/product-worker/tests/integration.rs",
            TEST_TWIN,
        );

        self.package(
            "extra-repo/worker-extra",
            "worker-extra",
            &format!("[dependencies]\n{valence}\n"),
            true,
        );
        self.write(
            "extra-repo/worker-extra/src/lib.rs",
            "compile_error!(\"inventory components must never be compiled\");\nasync fn f() {\n    let _ = Widget::delete_now(\"id\", &valence, valence::use_!(\"Worker extra deletes the widget.\")).await;\n}\n",
        );

        self.package(
            "widget-repo/widget",
            "widget",
            &format!("[dependencies]\n{valence}\n"),
            true,
        );
        self.write(
            "widget-repo/widget/src/lib.rs",
            "#[cfg(any())]\nmod uses {\nasync fn f() {\n    let _ = Widget::create(row, &valence, valence::use_!(\"Optional widget crate creates widgets.\")).await;\n}\n}\n",
        );

        self.package("noval-repo/noval", "noval", "", true);
        self.write(
            "noval-repo/noval/src/lib.rs",
            "#[cfg(any())]\nmod uses {\nasync fn f() {\n    let _ = Widget::get(\"id\", &valence, valence::use_!(\"Noval must never appear.\")).await;\n}\n}\n",
        );

        self.write("host/.git/HEAD", "ref: refs/heads/main\n");
        self.write(
            "host/Cargo.toml",
            "[workspace]\nresolver = \"2\"\nmembers = [\"server\", \"worker\", \"host-e2e\"]\n",
        );
        self.write_server("worker = { path = \"../worker\", features = [\"extra\"] }");
        self.write(
            "host/server/src/lib.rs",
            "#[cfg(any())]\nmod uses {\nasync fn f() {\n    let _ = Session::get(\"id\", &valence, valence::use_!(\"Server reads the session.\")).await;\n}\n}\n",
        );

        self.package(
            "host/worker",
            "worker",
            "[dependencies]\nuf-valence = { path = \"../../valence-stub\" }\nworker-extra = { path = \"../../extra-repo/worker-extra\", optional = true }\n\n[features]\nextra = [\"dep:worker-extra\"]\n",
            false,
        );
        self.write(
            "host/worker/src/lib.rs",
            "compile_error!(\"inventory components must never be compiled\");\nasync fn f() {\n    let _ = Widget::update(row, &valence, valence::use_!(\"Worker updates the widget.\")).await;\n}\n",
        );
        self.write("host/worker/src/main.rs", "fn main() {}\n");

        self.package(
            "host/host-e2e",
            "host-e2e",
            "[dependencies]\nuf-valence = { path = \"../../valence-stub\" }\n",
            false,
        );
        self.write(
            "host/host-e2e/src/lib.rs",
            "#[cfg(any())]\nmod uses {\nasync fn f() {\n    let _ = Widget::get(\"id\", &valence, valence::use_!(\"**Test:** host-e2e fixture purpose.\")).await;\n}\n}\n",
        );
    }

    fn write_server(&self, inventory: &str) {
        self.package(
            "host/server",
            "server",
            &format!(
                "[dependencies]\nuf-valence = {{ path = \"../../valence-stub\" }}\nproduct-worker = {{ path = \"../../product-repo/product-worker\" }}\nnoval = {{ path = \"../../noval-repo/noval\" }}\nwidget = {{ path = \"../../widget-repo/widget\", optional = true }}\n\n[features]\nwith-widget = [\"dep:widget\"]\n\n[target.'cfg(any())'.dependencies]\n{inventory}\n"
            ),
            false,
        );
    }

    fn config(&self, features: &[&str]) -> Config {
        Config {
            workspace_root: self.host(),
            out_dir: self.root().join("out"),
            host: HostPackage::Named {
                package: "server".into(),
                features: features.iter().map(|f| (*f).to_string()).collect(),
            },
            exclude_tests_from_snapshot: true,
            connection_edges: vec![],
        }
    }
}

fn crates(hits: &[ScanHit]) -> Vec<&str> {
    let mut names: Vec<&str> = hits.iter().map(|h| h.crate_name.as_str()).collect();
    names.dedup();
    names
}

fn purposes(hits: &[ScanHit]) -> String {
    hits.iter()
        .map(|h| h.purpose.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn inventory_dep_from_other_repo_is_scanned() {
    let fx = Fixture::new();
    let hits = collect_hits(&fx.config(&[])).unwrap();

    assert_eq!(
        crates(&hits),
        vec!["product-worker", "server", "worker", "worker-extra"]
    );
    let worker = hits.iter().find(|h| h.crate_name == "worker").unwrap();
    assert_eq!(worker.purpose, "Worker updates the widget.");
    assert_eq!(worker.target, TargetKind::Schema("widget".into()));
    assert_eq!(worker.repository, "https://github.com/acme/worker");

    let product_widget = hits
        .iter()
        .find(|h| h.purpose == "Product worker reads the widget.")
        .unwrap();
    assert_eq!(product_widget.crate_name, "product-worker");
}

#[test]
fn files_are_repo_relative() {
    let fx = Fixture::new();
    let hits = collect_hits(&fx.config(&[])).unwrap();
    let root = fx.root().to_string_lossy().into_owned();

    for hit in &hits {
        assert!(!hit.file.starts_with('/'), "absolute path: {}", hit.file);
        assert!(!hit.file.contains(&root), "build path leaked: {}", hit.file);
    }
    let file_of = |name: &str| {
        hits.iter()
            .find(|h| h.crate_name == name)
            .map(|h| h.file.as_str())
            .unwrap()
    };
    assert_eq!(file_of("product-worker"), "product-worker/src/lib.rs");
    assert_eq!(file_of("worker"), "worker/src/lib.rs");
    assert_eq!(file_of("server"), "server/src/lib.rs");
}

#[test]
fn undeclared_member_is_not_scanned() {
    let fx = Fixture::new();
    let hits = collect_hits(&fx.config(&[])).unwrap();
    assert!(!crates(&hits).contains(&"host-e2e"));
    assert!(!purposes(&hits).contains("host-e2e fixture purpose"));
}

#[test]
fn package_without_valence_dep_is_skipped() {
    let fx = Fixture::new();
    let hits = collect_hits(&fx.config(&[])).unwrap();
    assert!(!crates(&hits).contains(&"noval"));
    assert!(!purposes(&hits).contains("Noval must never appear."));
    assert!(!crates(&hits).contains(&"uf-valence"));
}

#[test]
fn host_feature_gates_optional_product() {
    let fx = Fixture::new();
    let without = collect_hits(&fx.config(&[])).unwrap();
    assert!(!crates(&without).contains(&"widget"));

    let with = collect_hits(&fx.config(&["with-widget"])).unwrap();
    assert!(crates(&with).contains(&"widget"));
    assert!(purposes(&with).contains("Optional widget crate creates widgets."));
}

#[test]
fn inventory_entry_features_gate_optional_crate() {
    let fx = Fixture::new();
    fx.write_server("worker = { path = \"../worker\" }");
    let hits = collect_hits(&fx.config(&[])).unwrap();
    assert!(crates(&hits).contains(&"worker"));
    assert!(!crates(&hits).contains(&"worker-extra"));
}

#[test]
fn unknown_host_package_fails() {
    let fx = Fixture::new();
    let mut config = fx.config(&[]);
    config.host = HostPackage::Named {
        package: "servr".into(),
        features: vec![],
    };
    let err = generate(&config).unwrap_err();
    assert!(matches!(err, DataUseScanError::UnknownHostPackage { ref name } if name == "servr"));
    assert!(
        !config.out_dir.join("data_uses.rs").exists(),
        "no catalog on failure"
    );
}

#[test]
fn unknown_feature_fails() {
    let fx = Fixture::new();
    let err = collect_hits(&fx.config(&["with-gadget"])).unwrap_err();
    assert!(matches!(
        err,
        DataUseScanError::UnknownFeature { ref package, ref feature }
            if package == "server" && feature == "with-gadget"
    ));
}

#[test]
fn bin_only_inventory_dep_is_reported() {
    let fx = Fixture::new();
    fx.write(
        "host/Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"server\", \"worker\", \"host-e2e\", \"bin-only\"]\n",
    );
    fx.package("host/bin-only", "bin-only", "", false);
    fx.write("host/bin-only/src/main.rs", "fn main() {}\n");
    fx.write_server("bin-only = { path = \"../bin-only\" }");

    let err = collect_hits(&fx.config(&[])).unwrap_err();
    assert!(matches!(
        err,
        DataUseScanError::InventoryDependencyWithoutLib { ref package } if package == "bin-only"
    ));
    assert!(err.to_string().contains("src/lib.rs"));
}

#[test]
fn inventory_deps_are_never_compiled() {
    let fx = Fixture::new();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "build",
            "-p",
            "server",
            "--offline",
            "--message-format=json",
        ])
        .current_dir(fx.host())
        .env("CARGO_TARGET_DIR", fx.root().join("target"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "host build failed (an inventory crate was compiled?):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let built: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("\"reason\":\"compiler-artifact\""))
        .filter_map(|l| l.split("\"name\":\"").nth(1))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert!(built.contains(&"server"), "built: {built:?}");
    assert!(built.contains(&"product_worker"), "built: {built:?}");
    assert!(!built.contains(&"worker"), "built: {built:?}");
    assert!(!built.contains(&"worker_extra"), "built: {built:?}");
}

#[test]
fn generate_writes_catalog_and_excludes_tests_when_configured() {
    let fx = Fixture::new();
    let config = fx.config(&[]);
    generate(&config).unwrap();

    let generated = fs::read_to_string(config.out_dir.join("data_uses.rs")).unwrap();
    assert!(generated.contains("pub fn data_use_catalog()"));
    assert!(generated.contains("session cookie"));
    assert!(
        !generated.contains("data-use scan twin suite"),
        "test twin purpose must be excluded from the catalog"
    );
    assert!(generated.contains("DataUseTarget::Schema"));
    assert!(generated.contains("DataUseTarget::Trait"));
    assert!(generated.contains("DataUseTarget::Unscoped"));
    assert!(
        generated.contains("https://github.com/acme/product-worker"),
        "catalog must carry package repository for Unscoped View source"
    );
}

#[test]
fn generate_includes_tests_when_not_excluded() {
    let fx = Fixture::new();
    let mut config = fx.config(&[]);
    config.exclude_tests_from_snapshot = false;
    let hits = collect_hits(&config).unwrap();
    // Only `src/` is scanned, so test twins under `tests/` never reach the catalog.
    assert!(!purposes(&hits).contains("data-use scan twin suite"));

    fx.write(
        "product-repo/product-worker/src/tests.rs",
        &format!("#[cfg(any())]\nmod uses {{\n{TEST_TWIN}\n}}\n"),
    );
    let excluded = collect_hits(&fx.config(&[])).unwrap();
    assert!(!purposes(&excluded).contains("data-use scan twin suite"));
    let included = collect_hits(&config).unwrap();
    assert!(purposes(&included).contains("data-use scan twin suite"));
}

fn append_hop(fx: &Fixture, purpose: &str) {
    let lib = fx.root().join("product-repo/product-worker/src/lib.rs");
    let mut body = fs::read_to_string(&lib).unwrap();
    body.push_str(&format!(
        "\n#[cfg(any())]\nmod hop {{\nasync fn f() {{\n    let _ = Todo::get_owner(&valence, valence::use_!(r#\"{purpose}\"#)).await;\n}}\n}}\n"
    ));
    fs::write(lib, body).unwrap();
}

#[test]
fn generate_bakes_referenced_schema_from_edges() {
    let fx = Fixture::new();
    append_hop(&fx, "**Test:** Fixture hop owner load for peer bake.");
    let mut config = fx.config(&[]);
    config.connection_edges = vec![ConnectionEdge {
        from_table: "todo".into(),
        from_field: "owner".into(),
        to_table: "user".into(),
    }];
    generate(&config).unwrap();

    let generated = fs::read_to_string(config.out_dir.join("data_uses.rs")).unwrap();
    assert!(
        generated.contains("referenced_schema: Some(String::from(\"user\"))"),
        "expected baked peer user, got:\n{generated}"
    );
    assert!(generated.contains(
        "connection: Some(ConnectionHop { field: String::from(\"owner\"), kind: ConnectionHopKind::ForwardGet })"
    ));
}

#[test]
fn generate_leaves_referenced_none_without_matching_edge() {
    let fx = Fixture::new();
    append_hop(&fx, "**Test:** Fixture hop without matching edge.");
    let hits = collect_hits(&fx.config(&[])).unwrap();
    let hop = hits
        .iter()
        .find(|h| h.purpose.contains("hop without matching edge"))
        .unwrap();
    assert_eq!(hop.connection_field.as_deref(), Some("owner"));
    assert_eq!(
        hop.referenced_schema, None,
        "without edges, peer must stay None"
    );
}
