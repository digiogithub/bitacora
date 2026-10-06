//! In-memory `GitBackend` for unit-testing the sync loop without a repository.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use super::{
    ActiveBackend, CommitMessage, CommitOpts, FetchOutcome, GitBackend, Oid, PushOutcome,
    RepoStatus, Result, TreeChange, TreeEdit,
};

/// A call recorded by [`FakeBackend`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeCall {
    /// `fetch(remote, branch)`.
    Fetch(String, String),
    /// `push(remote, branch)`.
    Push(String, String),
    /// `commit(subject)`.
    Commit(String),
    /// `update_ref(name)`.
    UpdateRef(String),
    /// Any other operation, by name.
    Other(&'static str),
}

#[derive(Default, Debug)]
struct State {
    calls: Vec<FakeCall>,
    fetches: VecDeque<Result<FetchOutcome>>,
    pushes: VecDeque<Result<PushOutcome>>,
    blobs: HashMap<(Oid, String), Vec<u8>>,
    status: RepoStatus,
    diffs: Vec<TreeChange>,
    next_oid: u64,
}

/// Scriptable fake: queue results, run the code under test, inspect [`FakeBackend::calls`].
#[derive(Default, Debug)]
pub struct FakeBackend {
    state: Mutex<State>,
}

impl FakeBackend {
    /// An empty fake.
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    /// Queues the next `fetch` result (default when empty: no change).
    pub fn queue_fetch(&self, r: Result<FetchOutcome>) {
        self.with(|s| s.fetches.push_back(r));
    }

    /// Queues the next `push` result (default when empty: pushed).
    pub fn queue_push(&self, r: Result<PushOutcome>) {
        self.with(|s| s.pushes.push_back(r));
    }

    /// Registers blob content for `read_blob`.
    pub fn set_blob(&self, commit: Oid, path: &str, content: &[u8]) {
        self.with(|s| {
            s.blobs.insert((commit, path.to_string()), content.to_vec());
        });
    }

    /// Sets the status returned by `status`.
    pub fn set_status(&self, status: RepoStatus) {
        self.with(|s| s.status = status);
    }

    /// Sets the changes returned by `diff_trees`.
    pub fn set_diff(&self, diffs: Vec<TreeChange>) {
        self.with(|s| s.diffs = diffs);
    }

    /// Calls recorded so far.
    pub fn calls(&self) -> Vec<FakeCall> {
        self.with(|s| s.calls.clone())
    }

    fn fresh_oid(s: &mut State) -> Oid {
        s.next_oid += 1;
        Oid(format!("{:040x}", s.next_oid))
    }
}

impl GitBackend for FakeBackend {
    fn fetch(&self, remote: &str, branch: &str) -> Result<FetchOutcome> {
        self.with(|s| {
            s.calls
                .push(FakeCall::Fetch(remote.to_string(), branch.to_string()));
            s.fetches.pop_front().unwrap_or(Ok(FetchOutcome {
                remote_head: None,
                updated: false,
            }))
        })
    }

    fn push(&self, remote: &str, branch: &str) -> Result<PushOutcome> {
        self.with(|s| {
            s.calls
                .push(FakeCall::Push(remote.to_string(), branch.to_string()));
            s.pushes
                .pop_front()
                .unwrap_or(Ok(PushOutcome { pushed: true }))
        })
    }

    fn merge_base(&self, _a: &Oid, _b: &Oid) -> Result<Option<Oid>> {
        self.with(|s| s.calls.push(FakeCall::Other("merge_base")));
        Ok(None)
    }

    fn read_blob(&self, commit: &Oid, path: &str) -> Result<Option<Vec<u8>>> {
        self.with(|s| {
            s.calls.push(FakeCall::Other("read_blob"));
            Ok(s.blobs.get(&(commit.clone(), path.to_string())).cloned())
        })
    }

    fn diff_trees(&self, _a: &Oid, _b: &Oid) -> Result<Vec<TreeChange>> {
        self.with(|s| {
            s.calls.push(FakeCall::Other("diff_trees"));
            Ok(s.diffs.clone())
        })
    }

    fn write_tree(&self, _base: &Oid, _edits: &[TreeEdit]) -> Result<Oid> {
        self.with(|s| {
            s.calls.push(FakeCall::Other("write_tree"));
            Ok(Self::fresh_oid(s))
        })
    }

    fn commit(&self, msg: &CommitMessage, _opts: CommitOpts) -> Result<Oid> {
        self.with(|s| {
            s.calls.push(FakeCall::Commit(msg.subject.clone()));
            Ok(Self::fresh_oid(s))
        })
    }

    fn commit_tree(&self, _tree: &Oid, _parents: &[Oid], _msg: &CommitMessage) -> Result<Oid> {
        self.with(|s| {
            s.calls.push(FakeCall::Other("commit_tree"));
            Ok(Self::fresh_oid(s))
        })
    }

    fn update_ref(&self, name: &str, _new: &Oid, _expected_old: Option<&Oid>) -> Result<()> {
        self.with(|s| s.calls.push(FakeCall::UpdateRef(name.to_string())));
        Ok(())
    }

    fn status(&self) -> Result<RepoStatus> {
        self.with(|s| {
            s.calls.push(FakeCall::Other("status"));
            Ok(s.status.clone())
        })
    }

    fn kind(&self) -> ActiveBackend {
        ActiveBackend::GixOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GitError;

    #[test]
    fn scripted_results_and_call_log() {
        let f = FakeBackend::new();
        f.queue_push(Err(GitError::NonFastForward));
        assert!(matches!(
            f.push("origin", "main"),
            Err(GitError::NonFastForward)
        ));
        assert!(f.push("origin", "main").is_ok());
        let c = f
            .commit(&CommitMessage::new("x"), CommitOpts::default())
            .unwrap();
        assert_eq!(c.as_hex().len(), 40);
        assert_eq!(f.calls().len(), 3);
    }
}
