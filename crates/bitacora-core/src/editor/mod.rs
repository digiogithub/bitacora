//! The editing core: page model, invertible ops, transactions and commands
//! (BIT-SP-0004, `docs/design/block-editor.md` §2-3).
//!
//! Naming: `graph::Graph` is the read-only loaded-graph model; [`Workspace`] is the mutable
//! editing state the design calls `Graph` (pages with block trees, global block index).

pub mod backup;
pub mod cmd;
pub mod flush;
pub mod fsio;
pub mod lifecycle;
pub mod model;
pub mod op;
pub mod rename;
pub mod tx;
pub mod workspace;

pub use cmd::{Cmd, Refusal, Target, plan};
pub use flush::{FileStat, FileStore, FlushReport, FsStore, MemStore, TakeDisk, WrittenFile};
pub use lifecycle::{DayRollover, LifecycleError, Opened, auto_title_preamble, new_page_path};
pub use model::{
    Block, BlockId, DiskSnapshot, IdGen, ModelError, Origin, Page, Position, Serialized, Subtree,
    text_hash, text_is_representable,
};
pub use op::{Op, OpError};
pub use rename::{
    CONFIG_PATH, MergeMode, PageFile, RefLookup, RenameError, RenamePlan, RenameReport,
    RenameRequest,
};
pub use tx::{CoalesceKey, CommitError, CursorState, InvariantError, Transaction, TxId};
pub use workspace::Workspace;
