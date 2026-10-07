//! Persistence of the dock layout (`<data_dir>/workspace.json`).

use std::path::Path;

use crate::settings::write_atomic;
use crate::ui::dock::DockAreaState;

/// Bump when the panel structure or dock sizes change incompatibly; older files are ignored.
/// v2: the right dock is 360px wide (`sidebar_right` token) instead of 280px.
pub const LAYOUT_VERSION: usize = 2;

/// Loads a saved layout. Returns `None` for a missing file (silently) and for an
/// unreadable, corrupt or outdated file (with a warning): the caller then builds
/// the default layout. Never fails.
pub fn load_layout(path: &Path) -> Option<DockAreaState> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            tracing::warn!(path = %path.display(), "cannot read workspace layout: {err}");
            return None;
        }
    };
    let state: DockAreaState = match serde_json::from_slice(&bytes) {
        Ok(state) => state,
        Err(err) => {
            tracing::warn!(path = %path.display(), "invalid workspace layout, using default: {err}");
            return None;
        }
    };
    if state.version != Some(LAYOUT_VERSION) {
        tracing::warn!(
            path = %path.display(),
            found = ?state.version,
            "workspace layout version mismatch, using default"
        );
        return None;
    }
    Some(state)
}

/// Saves the layout atomically.
pub fn save_layout(path: &Path, state: &DockAreaState) -> std::io::Result<()> {
    let json = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
    write_atomic(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DockAreaState {
        DockAreaState {
            version: Some(LAYOUT_VERSION),
            ..DockAreaState::default()
        }
    }

    #[test]
    fn missing_file_is_none() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(load_layout(&tmp.path().join("workspace.json")).is_none());
    }

    #[test]
    fn roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("workspace.json");
        save_layout(&path, &sample()).expect("save");
        assert_eq!(load_layout(&path), Some(sample()));
    }

    #[test]
    fn corrupt_or_outdated_file_falls_back() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("workspace.json");
        std::fs::write(&path, b"{ definitely not a layout").expect("write");
        assert!(load_layout(&path).is_none());
        let outdated = DockAreaState {
            version: Some(LAYOUT_VERSION + 1),
            ..DockAreaState::default()
        };
        save_layout(&path, &outdated).expect("save");
        assert!(load_layout(&path).is_none());
    }
}
