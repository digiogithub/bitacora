//! The editing-block protection hook (BIT-T-0344): tells core which block the user is on, so an
//! external change to exactly that block is reported (`QueueEvent::EditingBlockChanged`)
//! instead of silently replacing what is being typed.
//!
//! The views are read-only until the block editor lands; their focus model (a click on a
//! block) already drives the hook, and the editor will call the same function when a block
//! gets the caret. A page that core has not loaded has no snapshot and so nothing to protect.

use bitacora_core::editor::BlockId;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, PageSnapshot};

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
