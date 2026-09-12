//! Path filters for UI snapshot exclusion.

use std::path::Path;

/// Whether `path` should be omitted from the UI snapshot when exclusion is on.
#[must_use]
pub fn should_exclude_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.contains("/tests/") || s.contains("\\tests\\") {
        return true;
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.ends_with("_test.rs") || name == "tests.rs" {
            return true;
        }
    }
    // `src/.../tests/mod.rs` style modules
    path.components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s == "tests"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn excludes_tests_dir_and_suffix() {
        assert!(should_exclude_path(Path::new(
            "/ws/crate/tests/integration.rs"
        )));
        assert!(should_exclude_path(Path::new(
            r"C:\ws\crate\tests\integration.rs"
        )));
        assert!(should_exclude_path(Path::new("/ws/crate/src/foo_test.rs")));
        assert!(should_exclude_path(Path::new("/ws/crate/src/tests/mod.rs")));
        assert!(!should_exclude_path(Path::new("/ws/crate/src/lib.rs")));
        assert!(!should_exclude_path(&PathBuf::from(
            "/ws/crate/src/service.rs"
        )));
    }
}
