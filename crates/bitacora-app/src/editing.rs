//! The editing-block protection hook (BIT-T-0344): tells core which block the user is on, so an
//! external change to exactly that block is reported (`QueueEvent::EditingBlockChanged`)
//! instead of silently replacing what is being typed.
//!
//! The views are read-only until the block editor lands; their focus model (a click on a
//! block) already drives the hook, and the editor will call the same function when a block
//! gets the caret. A page that core has not loaded has no snapshot and so nothing to protect.

use std::sync::Mutex;

use bitacora_core::editor::BlockId;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, PageSnapshot};
use bitacora_mcp::WriteGate;

/// How long an agent is told to wait while the user has the caret in the block it wants to
/// write (`BLOCK_BUSY`, `retry_after_ms`).
pub const BUSY_RETRY_MS: u64 = 1000;

/// The block the user is editing, as the MCP server sees it: page title and block uuid. The
/// editors publish it; the server consults it as its [`WriteGate`], so agent writes never race
/// the text being typed (BIT-SP-0007, `BLOCK_BUSY`).
#[derive(Debug, Default)]
pub struct EditingGate {
    block: Mutex<Option<(String, String)>>,
}

impl EditingGate {
    /// Marks `uuid` of page `page` as being edited.
    pub fn set(&self, page: &str, uuid: &str) {
        *self
            .block
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((page.to_lowercase(), uuid.to_ascii_lowercase()));
    }

    /// No block is being edited.
    pub fn clear(&self) {
        *self
            .block
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

impl WriteGate for EditingGate {
    fn busy(&self, page: &str, block_uuid: &str) -> Option<u64> {
        let guard = self
            .block
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .as_ref()
            .is_some_and(|(p, u)| {
                *p == page.to_lowercase() && *u == block_uuid.to_ascii_lowercase()
            })
            .then_some(BUSY_RETRY_MS)
    }
}

/// The core id of the `block_index`-th block (document order) of `snapshot`.
pub fn block_id_at(snapshot: &PageSnapshot, block_index: usize) -> Option<BlockId> {
    snapshot.blocks.get(block_index).map(|b| b.id)
}

/// Sets (or clears, with `None`) the protected block of `queue` for the page titled `title`.
/// Returns the id handed to core.
pub fn sync_editing_block(
    queue: &CommandQueue,
    title: &str,
    block_index: Option<usize>,
) -> Option<BlockId> {
    let id = block_index.and_then(|index| {
        let snapshot = queue.snapshot(&PageKey::from_title(title))?;
        block_id_at(&snapshot, index)
    });
    queue.set_editing_block(id);
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitacora_core::queue::{Request, Source};

    #[test]
    fn the_gate_reports_only_the_block_being_edited() {
        let gate = EditingGate::default();
        let uuid = "6f2c1b7a-0000-4000-8000-000000000001";
        assert_eq!(gate.busy("Home", uuid), None);
        gate.set("Home", uuid);
        assert_eq!(gate.busy("home", &uuid.to_uppercase()), Some(BUSY_RETRY_MS));
        assert_eq!(gate.busy("Other", uuid), None);
        assert_eq!(
            gate.busy("Home", "6f2c1b7a-0000-4000-8000-000000000002"),
            None
        );
        gate.clear();
        assert_eq!(gate.busy("Home", uuid), None);
    }

    #[test]
    fn focus_maps_the_block_index_to_the_core_block_and_clearing_unprotects() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path().join("g");
        std::fs::create_dir_all(root.join("pages")).expect("dirs");
        std::fs::write(root.join("pages/P.md"), "- one\n- two\n").expect("page");
        let mut cfg = bitacora_runtime::RuntimeConfig::new(&root);
        cfg.data_dir = Some(tmp.path().join("data"));
        cfg.global_config = Some(tmp.path().join("none.edn"));
        cfg.watch = None;
        let session = bitacora_runtime::Session::open(cfg).expect("session");
        let key = session.open_page("pages/P.md").expect("open");
        let queue = session.queue().clone();

        let snapshot = queue.snapshot(&key).expect("snapshot");
        assert_eq!(snapshot.blocks.len(), 2);
        let second = snapshot.blocks[1].id;
        assert_eq!(sync_editing_block(&queue, "P", Some(1)), Some(second));
        // Out of range, unknown page and clearing all leave nothing protected.
        assert_eq!(sync_editing_block(&queue, "P", Some(9)), None);
        assert_eq!(sync_editing_block(&queue, "Unknown", Some(0)), None);
        assert_eq!(sync_editing_block(&queue, "P", None), None);

        // The protection is live: with block `second` protected, an external edit of exactly
        // that block is reported as `EditingBlockChanged`.
        let events = session.subscribe();
        assert_eq!(sync_editing_block(&queue, "P", Some(1)), Some(second));
        std::fs::write(root.join("pages/P.md"), "- one\n- two changed\n").expect("edit");
        let bytes = std::fs::read(root.join("pages/P.md")).expect("read");
        queue
            .execute(
                Source::External,
                Request::ExternalChange {
                    key: key.clone(),
                    bytes,
                },
            )
            .expect("external change");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut reported = false;
        while std::time::Instant::now() < deadline && !reported {
            while let Ok(ev) = events.try_recv() {
                reported |= matches!(
                    ev,
                    bitacora_runtime::RuntimeEvent::Queue(
                        bitacora_core::queue::QueueEvent::EditingBlockChanged(_)
                    )
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(reported, "no EditingBlockChanged for the protected block");
        let _ = session.shutdown(std::time::Duration::from_secs(5));
    }
}
