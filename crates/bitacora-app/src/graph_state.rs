//! Per-graph UI state kept outside the graph folder: the recently visited pages and the right
//! sidebar stack (BIT-US-0079, BIT-US-0080).
//!
//! It lives next to the index database (`<data_dir>/graphs/<hash>/state.json`, ADR-005) so the
//! graph folder stays clean. Everything in it is disposable: a missing or corrupt file only
//! loses the history.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::nav::Route;
use crate::paths::graph_hash;

/// Pages remembered in the "Recent" section.
pub const MAX_RECENT: usize = 20;

/// A route as stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum StoredRoute {
    /// A page by title.
    Page(String),
    /// A block by UUID.
    Block(String),
}

impl StoredRoute {
    /// The stored form of `route`; the journals feed and the page list are not stackable.
    pub fn from_route(route: &Route) -> Option<Self> {
        match route {
            Route::Page(name) | Route::PageAt { page: name, .. } => Some(Self::Page(name.clone())),
            Route::Block(uuid) => Some(Self::Block(uuid.clone())),
            Route::Journals | Route::AllPages | Route::Graph | Route::Tasks => None,
        }
    }

    /// Back to a route.
    pub fn to_route(&self) -> Route {
        match self {
            Self::Page(name) => Route::Page(name.clone()),
            Self::Block(uuid) => Route::Block(uuid.clone()),
        }
    }
}

/// One item of the right sidebar stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackEntry {
    /// What the item shows.
    pub route: StoredRoute,
    /// The item is folded to its header.
    #[serde(default)]
    pub collapsed: bool,
}

/// The persisted state of one graph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphState {
    /// Recently visited page titles, newest first (at most [`MAX_RECENT`]).
    pub recent: Vec<String>,
    /// The right sidebar stack, top item first.
    pub right_sidebar: Vec<StackEntry>,
}

impl GraphState {
    /// `<data_dir>/graphs/<hash>/state.json` for the graph at `root`.
    pub fn file_for(data_dir: &Path, root: &Path) -> PathBuf {
        data_dir
            .join("graphs")
            .join(graph_hash(root))
            .join("state.json")
    }

    /// Loads the state; a missing, unreadable or invalid file yields the empty state.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
                tracing::warn!(path = %path.display(), "invalid graph state, ignoring: {err}");
                Self::default()
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(err) => {
                tracing::warn!(path = %path.display(), "cannot read graph state: {err}");
                Self::default()
            }
        }
    }

    /// Writes the state atomically.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::settings::write_atomic(path, &json)
    }

    /// Moves `title` to the front of the recent list (case-insensitive de-duplication).
    pub fn push_recent(&mut self, title: &str) {
        let title = title.trim();
        if title.is_empty() {
            return;
        }
        self.recent.retain(|t| !t.eq_ignore_ascii_case(title));
        self.recent.insert(0, title.to_owned());
        self.recent.truncate(MAX_RECENT);
    }

    /// Forgets a page (deleted pages leave the recent list).
    pub fn forget_recent(&mut self, title: &str) {
        self.recent.retain(|t| !t.eq_ignore_ascii_case(title));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_is_deduplicated_ordered_and_bounded() {
        let mut s = GraphState::default();
        s.push_recent("A");
        s.push_recent("B");
        s.push_recent("a");
        assert_eq!(s.recent, vec!["a".to_owned(), "B".to_owned()]);
        for n in 0..40 {
            s.push_recent(&format!("p{n}"));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
        assert_eq!(s.recent[0], "p39");
        s.push_recent("  ");
        assert_eq!(s.recent.len(), MAX_RECENT);
    }

    #[test]
    fn state_round_trips_and_tolerates_garbage() {
        let tmp = tempfile::tempdir().expect("tmp");
        let file = GraphState::file_for(tmp.path(), Path::new("/g"));
        assert_eq!(GraphState::load(&file), GraphState::default());
        let mut s = GraphState::default();
        s.push_recent("Alpha");
        s.right_sidebar.push(StackEntry {
            route: StoredRoute::Block("abc".into()),
            collapsed: true,
        });
        s.save(&file).expect("save");
        assert_eq!(GraphState::load(&file), s);
        std::fs::write(&file, b"{ nope").expect("corrupt");
        assert_eq!(GraphState::load(&file), GraphState::default());
    }

    #[test]
    fn only_pages_and_blocks_are_stackable() {
        assert!(StoredRoute::from_route(&Route::Journals).is_none());
        assert!(StoredRoute::from_route(&Route::AllPages).is_none());
        let r = Route::Page("X".into());
        assert_eq!(StoredRoute::from_route(&r).map(|s| s.to_route()), Some(r));
    }
}
