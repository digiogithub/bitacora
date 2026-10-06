//! `HybridBackend`: system git for network and ref writes, gix for reads (ADR-007).

use super::{
    ActiveBackend, CliBackend, CommitMessage, CommitOpts, FetchOutcome, GitBackend, GixBackend,
    Oid, PushOutcome, RepoStatus, Result, TreeChange, TreeEdit,
};

/// Delegates per ADR-007.
#[derive(Debug)]
pub struct HybridBackend {
    cli: CliBackend,
    gix: GixBackend,
}

impl HybridBackend {
    /// Combines both halves.
    pub fn new(cli: CliBackend, gix: GixBackend) -> Self {
        Self { cli, gix }
    }

    /// The CLI half (for operations outside the trait, such as `ls_remote`).
    pub fn cli(&self) -> &CliBackend {
        &self.cli
    }
}

impl GitBackend for HybridBackend {
    fn fetch(&self, remote: &str, branch: &str) -> Result<FetchOutcome> {
        self.cli.fetch(remote, branch)
    }
    fn push(&self, remote: &str, branch: &str) -> Result<PushOutcome> {
        self.cli.push(remote, branch)
    }
    fn merge_base(&self, a: &Oid, b: &Oid) -> Result<Option<Oid>> {
        self.gix.merge_base(a, b)
    }
    fn read_blob(&self, commit: &Oid, path: &str) -> Result<Option<Vec<u8>>> {
        self.gix.read_blob(commit, path)
    }
    fn diff_trees(&self, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>> {
        self.gix.diff_trees(a, b)
    }
    fn write_tree(&self, base: &Oid, edits: &[TreeEdit]) -> Result<Oid> {
        self.gix.write_tree(base, edits)
    }
    fn commit(&self, msg: &CommitMessage, opts: CommitOpts) -> Result<Oid> {
        self.cli.commit(msg, opts)
    }
    fn commit_tree(&self, tree: &Oid, parents: &[Oid], msg: &CommitMessage) -> Result<Oid> {
        self.cli.commit_tree(tree, parents, msg)
    }
    fn update_ref(&self, name: &str, new: &Oid, expected_old: Option<&Oid>) -> Result<()> {
        self.cli.update_ref(name, new, expected_old)
    }
    fn status(&self) -> Result<RepoStatus> {
        self.gix.status()
    }
    fn kind(&self) -> ActiveBackend {
        ActiveBackend::Hybrid
    }
}
