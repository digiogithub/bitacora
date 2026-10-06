//! The editing core: page model, invertible ops, transactions and commands
//! (BIT-SP-0004, `docs/design/block-editor.md` §2-3).
//!
//! Naming: `graph::Graph` is the read-only loaded-graph model; [`Workspace`] is the mutable
//! editing state the design calls `Graph` (pages with block trees, global block index).

pub mod backup;
pub mod clipboard;
pub mod cmd;
pub mod complete;
pub mod external;
pub mod flush;
pub mod fsio;
pub mod history;
pub mod lifecycle;
pub mod model;
pub mod op;
pub mod outline;
pub mod projection;
pub mod rename;
pub mod settings;
pub mod split;
pub mod text;
pub mod tx;
pub mod workspace;

pub use clipboard::{
    ClipBlock, ClipboardPayload, PRIVATE_MIME, PasteKind, classify_paste, export_blocks,
    parse_outline, parse_private, to_markdown,
};
pub use cmd::{Cmd, Planned, Refusal, Target, plan, plan_full};
pub use complete::{
    BlockSuggestion, Completion, CompletionProvider, PageSuggestion, WorkspaceProvider,
    block_candidates, block_embed_text, block_ref_text, complete_page, page_candidates,
    trigger_range,
};
pub use external::{
    BlockDiff, ConflictNotice, DiffKind, EditingConflict, ExternalEvent, ExternalOutcome,
    ReloadReport, align, diff_blocks,
};
pub use flush::{FileStat, FileStore, FlushReport, FsStore, MemStore, TakeDisk, WrittenFile};
pub use history::{History, HistoryConfig, HistoryError, HistoryStep};
pub use lifecycle::{DayRollover, LifecycleError, Opened, auto_title_preamble, new_page_path};
pub use model::{
    Block, BlockId, DiskSnapshot, IdGen, ModelError, Origin, Page, Position, Serialized, Subtree,
    text_hash, text_is_representable,
};
pub use op::{Op, OpError};
pub use projection::{EditProjection, HiddenKeys};
pub use rename::{
    CONFIG_PATH, MergeMode, PageFile, RefLookup, RenameError, RenamePlan, RenameReport,
    RenameRequest,
};
pub use settings::{EditorSettings, Workflow};
pub use split::{EnterAction, enter_action, splits_before};
pub use tx::{CoalesceKey, CommitError, CursorState, InvariantError, Transaction, TxId};
pub use workspace::Workspace;
