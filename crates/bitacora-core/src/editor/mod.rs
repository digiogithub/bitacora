//! The editing core: page model, invertible ops, transactions and commands
//! (BIT-SP-0004, `docs/design/block-editor.md` §2-3).
//!
//! Naming: `graph::Graph` is the read-only loaded-graph model; [`Workspace`] is the mutable
//! editing state the design calls `Graph` (pages with block trees, global block index).

pub mod cmd;
pub mod flush;
pub mod model;
pub mod op;
pub mod tx;
pub mod workspace;

pub use cmd::{Cmd, Refusal, Target, plan};
pub use flush::{FileStore, FlushReport, FsStore, MemStore, WrittenFile};
pub use model::{
    Block, BlockId, DiskSnapshot, IdGen, ModelError, Origin, Page, Position, Subtree, text_hash,
};
pub use op::{Op, OpError};
pub use tx::{CoalesceKey, CommitError, CursorState, InvariantError, Transaction, TxId};
pub use workspace::Workspace;
