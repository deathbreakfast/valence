//! Installing the process-wide data-use catalog (own test binary: one process, one install).

#![allow(clippy::unwrap_used)]

use valence_core::data_use::{DataOp, DataUse, DataUseCatalog, DataUseTarget};

fn row(crate_name: &str, schema: &str) -> DataUse {
    DataUse {
        purpose: format!("{crate_name} reads {schema}"),
        file: format!("{crate_name}/src/lib.rs"),
        line: 7,
        crate_name: crate_name.into(),
        repository: "https://github.com/unified-field-dev/example".into(),
        target: DataUseTarget::Schema(schema.into()),
        op: DataOp::Read,
        method: "get".into(),
        connection: None,
        referenced_schema: None,
        via_trait: None,
    }
}

#[test]
fn install_then_global_returns_same_rows() {
    assert!(DataUseCatalog::global().is_none(), "nothing installed yet");

    let catalog = DataUseCatalog::from_entries(vec![
        row("counter-app-worker", "counter"),
        row("data-use-probe-worker", "widget"),
    ]);
    let expected = catalog.clone();

    let installed = catalog.install().unwrap();
    assert_eq!(installed, &expected);

    let global = DataUseCatalog::global().unwrap();
    assert!(std::ptr::eq(global, installed));
    assert_eq!(global.len(), 2);
    assert_eq!(
        global.crate_names().into_iter().collect::<Vec<_>>(),
        vec!["counter-app-worker", "data-use-probe-worker"]
    );
}
