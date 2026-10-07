//! Remembered answers to approval cards ("Remember my decision").
//!
//! The chat consults a [`ToolMemory`] before it shows a card for a tool call: a remembered
//! decision answers it without asking. Memory is machine-local and per graph (the runtime backs
//! it with `pando.json`, never the graph folder) and is cleared when consent is revoked. It is
//! keyed by tool name only, so it can never widen what a tool is allowed to touch: a remembered
//! allow of `propose_edit` still goes through the content guard, the core queue, the audit log
//! and undo like a manual approval.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// Where remembered tool decisions live.
pub trait ToolMemory: Send + Sync {
    /// `Some(true)` = always allow, `Some(false)` = always deny, `None` = ask.
    fn decision(&self, tool: &str) -> Option<bool>;
    /// Remembers `allow` for `tool` (persisting it is up to the implementation).
    fn remember(&self, tool: &str, allow: bool);
}

/// A [`ToolMemory`] kept in memory only (tests, headless use).
#[derive(Debug, Default)]
pub struct InMemoryToolMemory {
    map: Mutex<BTreeMap<String, bool>>,
}

impl InMemoryToolMemory {
    /// A memory seeded with `decisions`.
    #[must_use]
    pub fn with(decisions: impl IntoIterator<Item = (String, bool)>) -> Self {
        Self {
            map: Mutex::new(decisions.into_iter().collect()),
        }
    }

    /// Everything remembered so far.
    #[must_use]
    pub fn snapshot(&self) -> BTreeMap<String, bool> {
        self.map.lock().map(|m| m.clone()).unwrap_or_default()
    }

    /// Forgets everything (consent revoked).
    pub fn clear(&self) {
        if let Ok(mut m) = self.map.lock() {
            m.clear();
        }
    }
}

impl ToolMemory for InMemoryToolMemory {
    fn decision(&self, tool: &str) -> Option<bool> {
        self.map.lock().ok()?.get(tool).copied()
    }

    fn remember(&self, tool: &str, allow: bool) {
        if let Ok(mut m) = self.map.lock() {
            m.insert(tool.to_owned(), allow);
        }
    }
}
