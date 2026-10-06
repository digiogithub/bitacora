//! `bitacora-testkit`: dev-only test helpers (fixture loader, temporary graphs and repos).
//!
//! Must only be used from `[dev-dependencies]`; it is never a runtime dependency.

use std::path::PathBuf;

/// Root directory of the sample Logseq graphs used by tests (`fixtures/graphs`).
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs")
}

/// Create an empty temporary directory that is removed on drop.
pub fn temp_dir() -> std::io::Result<tempfile::TempDir> {
    tempfile::tempdir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dir_is_created() {
        let dir = temp_dir().expect("temp dir");
        assert!(dir.path().is_dir());
        assert!(fixtures_dir().ends_with("fixtures/graphs"));
    }
}
