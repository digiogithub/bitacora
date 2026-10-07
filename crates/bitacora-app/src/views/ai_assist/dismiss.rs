//! Machine-local memory of dismissed AI suggestions (BIT-US-0152).
//!
//! A dismissed suggestion is not offered again for the same content: the key hashes the kind,
//! the page and what the suggestion says (target page, tag, text), so a different suggestion for
//! the same page still shows. Only hashes are stored, never graph text, in a state-directory file
//! next to the other per-machine state; nothing goes into the graph or the index.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name inside the app state directory.
pub const DISMISS_FILE: &str = "ai-dismissed.json";
/// Most remembered suggestions per graph (the oldest are forgotten first).
pub const MAX_PER_GRAPH: usize = 1000;

/// What kind of suggestion a key is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A missing `[[link]]` in a block.
    Link,
    /// A tag to add to the page.
    Tag,
    /// A page worth reading.
    Related,
    /// A next action.
    Action,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Link => "link",
            Kind::Tag => "tag",
            Kind::Related => "related",
            Kind::Action => "action",
        }
    }
}

/// The stable key of a suggestion: FNV-1a over the case-folded parts.
#[must_use]
pub fn key(kind: Kind, page: &str, parts: &[&str]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |s: &str| {
        for b in s.to_lowercase().bytes().chain(std::iter::once(0)) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(kind.name());
    feed(page);
    for p in parts {
        feed(p);
    }
    format!("{h:016x}")
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct FileData {
    /// Keys per graph, oldest first.
    graphs: BTreeMap<String, Vec<String>>,
}

/// The dismissed suggestions of one graph.
#[derive(Debug, Clone, Default)]
pub struct DismissStore {
    path: Option<PathBuf>,
    graph: String,
    keys: Vec<String>,
}

impl DismissStore {
    /// An in-memory store (nothing is saved).
    #[must_use]
    pub fn memory() -> Self {
        Self::default()
    }

    /// The store of `graph` in `dir`. A missing or unreadable file is an empty memory.
    #[must_use]
    pub fn load(dir: &Path, graph: &str) -> Self {
        let path = dir.join(DISMISS_FILE);
        let keys = read(&path).graphs.remove(graph).unwrap_or_default();
        Self {
            path: Some(path),
            graph: graph.to_owned(),
            keys,
        }
    }

    /// Whether `key` was dismissed.
    #[must_use]
    pub fn is_dismissed(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }

    /// How many suggestions are remembered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Nothing is remembered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Remembers `key` and saves. Saving failures are logged: the dismissal still holds for the
    /// running session.
    pub fn dismiss(&mut self, key: String) {
        if self.is_dismissed(&key) {
            return;
        }
        self.keys.push(key);
        if self.keys.len() > MAX_PER_GRAPH {
            let extra = self.keys.len() - MAX_PER_GRAPH;
            self.keys.drain(..extra);
        }
        if let Err(e) = self.save() {
            tracing::warn!("cannot save the dismissed AI suggestions: {e}");
        }
    }

    /// Forgets everything of this graph and saves.
    pub fn clear(&mut self) {
        self.keys.clear();
        if let Err(e) = self.save() {
            tracing::warn!("cannot save the dismissed AI suggestions: {e}");
        }
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        // Re-read so another window's graphs are kept.
        let mut data = read(path);
        if self.keys.is_empty() {
            data.graphs.remove(&self.graph);
        } else {
            data.graphs.insert(self.graph.clone(), self.keys.clone());
        }
        let json = serde_json::to_vec_pretty(&data).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write as _;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)
    }
}

fn read(path: &Path) -> FileData {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn keys_depend_on_kind_page_and_content_not_on_case() {
        let a = key(Kind::Tag, "Rust", &["ai"]);
        assert_eq!(a, key(Kind::Tag, "rust", &["AI"]));
        assert_ne!(a, key(Kind::Tag, "Rust", &["ml"]));
        assert_ne!(a, key(Kind::Related, "Rust", &["ai"]));
        assert_ne!(a, key(Kind::Tag, "Go", &["ai"]));
        // Part boundaries matter: ("ab","c") is not ("a","bc").
        assert_ne!(
            key(Kind::Link, "p", &["ab", "c"]),
            key(Kind::Link, "p", &["a", "bc"])
        );
    }

    #[test]
    fn dismissals_persist_per_graph_and_stay_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = DismissStore::load(dir.path(), "/g/a");
        let mut b = DismissStore::load(dir.path(), "/g/b");
        a.dismiss("k1".into());
        b.dismiss("k2".into());
        a.dismiss("k1".into());
        assert_eq!(a.len(), 1);
        let a2 = DismissStore::load(dir.path(), "/g/a");
        let b2 = DismissStore::load(dir.path(), "/g/b");
        assert!(a2.is_dismissed("k1") && !a2.is_dismissed("k2"));
        assert!(b2.is_dismissed("k2"));
        a.clear();
        assert!(DismissStore::load(dir.path(), "/g/a").is_empty());
        assert!(DismissStore::load(dir.path(), "/g/b").is_dismissed("k2"));

        let mut m = DismissStore::memory();
        for i in 0..MAX_PER_GRAPH + 5 {
            m.dismiss(format!("k{i}"));
        }
        assert_eq!(m.len(), MAX_PER_GRAPH);
        assert!(!m.is_dismissed("k0") && m.is_dismissed(&format!("k{}", MAX_PER_GRAPH + 4)));
    }

    #[test]
    fn a_corrupt_file_is_an_empty_memory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(DISMISS_FILE), "{ nope").unwrap();
        let mut s = DismissStore::load(dir.path(), "g");
        assert!(s.is_empty());
        s.dismiss("k".into());
        assert!(DismissStore::load(dir.path(), "g").is_dismissed("k"));
    }
}
