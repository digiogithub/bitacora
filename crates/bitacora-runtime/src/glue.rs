//! Index- and config-backed hooks for the sync engine, and the MCP sync status provider.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::UNIX_EPOCH;

use bitacora_config::EffectiveConfig;
use bitacora_index::ReaderPool;
use bitacora_mcp::SyncStatusProvider;
use bitacora_sync::engine::EngineHandle;
use bitacora_sync::merge::BlockLocation;
use bitacora_sync::state::SyncState;

/// Lookup handed to the sync engine.
pub(crate) type Locator = Arc<dyn Fn(&str) -> Option<BlockLocation> + Send + Sync>;

/// `uuid -> (file, first line of the block)` over the index (BIT-SP-0006.R14).
pub(crate) fn block_locator(readers: ReaderPool) -> Locator {
    Arc::new(move |uuid: &str| {
        let conn = readers.get().ok()?;
        let (path, content): (String, String) = conn
            .query_row(
                "SELECT f.path, b.content FROM blocks b JOIN files f ON f.id = b.file_id \
                 WHERE b.uuid = ?1",
                [uuid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?;
        let first_line = content.lines().next().unwrap_or_default().to_owned();
        Some(BlockLocation { path, first_line })
    })
}

/// Text a journal gets from `:default-templates {:journals "<name>"}`: the children of the block
/// carrying `template:: <name>`, rendered as an outline. `None` when no template is configured
/// or found. Best effort: a journal that differs only in whitespace handling is not recognised
/// as template-only and merges as normal content.
pub(crate) fn journal_template_text(
    config: &EffectiveConfig,
    readers: &ReaderPool,
) -> Option<String> {
    let name = config.default_journal_template();
    if name.trim().is_empty() {
        return None;
    }
    let conn = readers.get().ok()?;
    let (file_id, ord, end, depth): (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT b.file_id, b.ord, b.subtree_end, b.depth FROM blocks b \
             JOIN block_properties p ON p.block_id = b.id \
             WHERE p.key = 'template' AND lower(p.raw_value) = lower(?1) LIMIT 1",
            [name.trim()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT depth, content FROM blocks WHERE file_id = ?1 AND ord > ?2 AND ord <= ?3 \
             ORDER BY ord",
        )
        .ok()?;
    let rows = stmt
        .query_map([file_id, ord, end], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })
        .ok()?;
    let mut out = String::new();
    for row in rows.flatten() {
        let indent = "\t".repeat(usize::try_from(row.0 - depth - 1).unwrap_or(0));
        for (i, line) in row.1.lines().enumerate() {
            out.push_str(&indent);
            out.push_str(if i == 0 { "- " } else { "  " });
            out.push_str(line);
            out.push('\n');
        }
    }
    (!out.trim().is_empty()).then_some(out)
}

/// [`SyncStatusProvider`] reading the background engine's published status.
#[derive(Clone)]
pub(crate) struct SlotStatus(pub(crate) Arc<Mutex<Option<Arc<EngineHandle>>>>);

impl SyncStatusProvider for SlotStatus {
    fn status(&self) -> bitacora_mcp::SyncStatus {
        use bitacora_mcp::SyncState as Out;
        let guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(handle) = guard.as_ref() else {
            return bitacora_mcp::SyncStatus::default();
        };
        let s = handle.status();
        let state = match s.state {
            SyncState::Disabled => Out::Disabled,
            SyncState::Idle | SyncState::Dirty => Out::Idle,
            SyncState::Conflicted => Out::Conflicted,
            SyncState::Error(_) | SyncState::Offline => Out::Error,
            _ => Out::Syncing,
        };
        bitacora_mcp::SyncStatus {
            state,
            ahead: u32::try_from(s.ahead).unwrap_or(u32::MAX),
            behind: u32::try_from(s.behind).unwrap_or(u32::MAX),
            last_sync: s.last_sync.and_then(|t| {
                t.duration_since(UNIX_EPOCH)
                    .ok()
                    .and_then(|d| i64::try_from(d.as_millis()).ok())
            }),
            conflict_count: u32::try_from(s.conflicts).unwrap_or(u32::MAX),
            conflict_pages: s.conflict_pages,
            last_error: s.last_error,
        }
    }
}
