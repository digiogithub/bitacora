//! Onboarding integration tests against temp bare repositories.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use bitacora_sync::GitDetection;
use bitacora_sync::onboarding::{
    OnboardingConfig, OnboardingError, RemoteState, clone_graph, detect_separate_gitdir,
    enable_sync, migrate_separate_gitdir,
};
use bitacora_sync::repo_setup::Identity;
use bitacora_testkit::git::{clone_to, commit_file, git, git_available, init_bare, write_file};
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    root: PathBuf,
    remote: PathBuf,
}

impl Env {
    fn new() -> Self {
        assert!(git_available(), "tests need a system git");
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let remote = root.join("remote.git");
        init_bare(&remote);
        Self {
            _tmp: tmp,
            root,
            remote,
        }
    }
    fn url(&self) -> String {
        self.remote.to_string_lossy().into_owned()
    }
    fn graph(&self, name: &str) -> PathBuf {
        let g = self.root.join(name);
        write_file(&g, "pages/Home.md", "- home\n");
        write_file(&g, "journals/2026_10_06.md", "- today\n");
        write_file(&g, "logseq/config.edn", "{:meta/version 1}\n");
        write_file(&g, "logseq/bak/pages/Home.md", "- old\n");
        g
    }
}

/// Both backend selections: system git (hybrid) and no git (gix-only).
fn configs() -> Vec<(&'static str, OnboardingConfig)> {
    let mut v = Vec::new();
    let found = bitacora_sync::detect_git(None);
    assert!(matches!(found, GitDetection::Found { .. }), "git required");
    let mut c = OnboardingConfig::new(found);
    c.identity = Some(Identity {
        name: "Tester".into(),
        email: "tester@example.com".into(),
    });
    v.push(("hybrid", c.clone()));
    c.detection = GitDetection::Missing;
    v.push(("gix-only", c));
    v
}

fn global_gitconfig() -> Option<Vec<u8>> {
    let home = std::env::var_os("HOME")?;
    std::fs::read(Path::new(&home).join(".gitconfig")).ok()
}

#[test]
fn enable_sync_on_empty_remote_pushes_initial_commit() {
    let global_before = global_gitconfig();
    for (kind, config) in configs() {
        let env = Env::new();
        let graph = env.graph("g");
        let out = enable_sync(&graph, &env.url(), "main", &config).unwrap();
        assert!(out.initialized && out.committed, "{kind}");
        assert_eq!(out.remote, RemoteState::EmptyRemotePushed, "{kind}");
        assert_eq!(out.identity.email, "tester@example.com");

        // Remote has the commit, and volatile files stayed out of it.
        let tree = git(&env.remote, &["ls-tree", "-r", "--name-only", "main"]);
        assert!(tree.contains("pages/Home.md"), "{kind}: {tree}");
        assert!(tree.contains(".gitignore"), "{kind}: {tree}");
        assert!(
            !tree.contains("logseq/bak"),
            "{kind}: bak must be ignored: {tree}"
        );
        let body = git(&env.remote, &["log", "-1", "--format=%B", "main"]);
        assert!(body.contains("Bitacora-Kind: migrate"), "{kind}: {body}");
        assert_eq!(
            git(&graph, &["log", "-1", "--format=%ae", "main"]),
            "tester@example.com"
        );

        // Repo preparation: attributes (not committed) and repo-local identity.
        let attrs = std::fs::read_to_string(graph.join(".git/info/attributes")).unwrap();
        assert!(attrs.contains("*.md merge=binary"), "{kind}");
        assert!(attrs.contains("logseq/config.edn merge=binary"), "{kind}");
        assert!(attrs.contains("* -text"), "{kind}");
        assert_eq!(git(&graph, &["config", "--local", "user.name"]), "Tester");
        assert_eq!(
            git(&graph, &["config", "--local", "branch.main.remote"]),
            "origin"
        );
        assert_eq!(git(&graph, &["status", "--porcelain"]), "", "{kind}");

        // Idempotent: running again finds the repo in sync and changes nothing.
        let again = enable_sync(&graph, &env.url(), "main", &config).unwrap();
        assert!(!again.initialized && !again.committed, "{kind}");
        assert_eq!(again.remote, RemoteState::InSync, "{kind}");
    }
    assert_eq!(
        global_gitconfig(),
        global_before,
        "global git config was modified"
    );
}

#[test]
fn enable_sync_with_non_empty_remote_requests_merge_without_pushing() {
    for (kind, config) in configs() {
        let env = Env::new();
        // Another device already published unrelated history.
        let other = env.root.join("other");
        bitacora_testkit::git::init_repo(&other);
        git(&other, &["remote", "add", "origin", &env.url()]);
        let theirs = commit_file(&other, "pages/Home.md", "- theirs\n", "other device");
        git(&other, &["push", "-q", "origin", "main"]);

        let graph = env.graph("g");
        let out = enable_sync(&graph, &env.url(), "main", &config).unwrap();
        match out.remote {
            RemoteState::NeedsMerge {
                remote_head,
                merge_base,
                ..
            } => {
                assert_eq!(remote_head.as_hex(), theirs, "{kind}");
                assert_eq!(
                    merge_base, None,
                    "{kind}: unrelated histories merge on an empty base"
                );
            }
            other => panic!("{kind}: expected NeedsMerge, got {other:?}"),
        }
        // Nothing pushed; the user's work tree file is untouched.
        assert_eq!(git(&env.remote, &["rev-parse", "main"]), theirs, "{kind}");
        assert_eq!(
            std::fs::read_to_string(graph.join("pages/Home.md")).unwrap(),
            "- home\n"
        );
    }
}

#[test]
fn enable_sync_on_shared_history_pushes_local_commits() {
    for (kind, config) in configs() {
        let env = Env::new();
        let seed = env.root.join("seed");
        bitacora_testkit::git::init_repo(&seed);
        git(&seed, &["remote", "add", "origin", &env.url()]);
        commit_file(&seed, "pages/Home.md", "- home\n", "seed");
        git(&seed, &["push", "-q", "origin", "main"]);

        // A graph that is a clone with extra local edits (no `.git` pointer tricks).
        let graph = env.root.join("g");
        clone_to(&env.url(), &graph);
        write_file(&graph, "pages/New.md", "- new\n");
        let out = enable_sync(&graph, &env.url(), "main", &config).unwrap();
        assert!(!out.initialized, "{kind}");
        assert!(out.committed, "{kind}");
        assert_eq!(out.remote, RemoteState::LocalAheadPushed, "{kind}");
        assert_eq!(
            git(&env.remote, &["rev-parse", "main"]),
            git(&graph, &["rev-parse", "HEAD"]),
            "{kind}"
        );
    }
}

#[test]
fn enable_sync_rejects_a_different_existing_origin() {
    let env = Env::new();
    let graph = env.graph("g");
    let config = configs().remove(0).1;
    enable_sync(&graph, &env.url(), "main", &config).unwrap();
    let err = enable_sync(&graph, "/somewhere/else.git", "main", &config).unwrap_err();
    assert!(
        matches!(err, OnboardingError::RemoteMismatch { .. }),
        "{err:?}"
    );
    let err = enable_sync(&graph, "--upload-pack=evil", "main", &config).unwrap_err();
    assert!(
        matches!(err, OnboardingError::InvalidArgument(_)),
        "{err:?}"
    );
}

#[test]
fn clone_graph_prepares_repository() {
    for (kind, config) in configs() {
        let env = Env::new();
        let seed = env.root.join("seed");
        bitacora_testkit::git::init_repo(&seed);
        git(&seed, &["remote", "add", "origin", &env.url()]);
        commit_file(&seed, "pages/Home.md", "- home\n", "seed");
        git(&seed, &["push", "-q", "origin", "main"]);

        let dest = env.root.join("clone");
        let id = clone_graph(&env.url(), &dest, &config).unwrap();
        assert_eq!(id.name, "Tester");
        assert_eq!(
            std::fs::read_to_string(dest.join("pages/Home.md")).unwrap(),
            "- home\n",
            "{kind}"
        );
        let attrs = std::fs::read_to_string(dest.join(".git/info/attributes")).unwrap();
        assert!(attrs.contains("*.md merge=binary"), "{kind}");
        assert!(dest.join(".gitignore").is_file(), "{kind}");
        assert_eq!(
            git(&dest, &["config", "--local", "user.email"]),
            "tester@example.com"
        );

        let err = clone_graph(&env.url(), &dest, &config).unwrap_err();
        assert!(
            matches!(err, OnboardingError::DestinationNotEmpty(_)),
            "{kind}"
        );
    }
}

#[test]
fn separate_gitdir_is_detected_and_migrated() {
    let env = Env::new();
    let graph = env.root.join("logseq-graph");
    let gitdir = env.root.join("dot-logseq/git/graph.git");
    std::fs::create_dir_all(&graph).unwrap();
    std::fs::create_dir_all(gitdir.parent().unwrap()).unwrap();
    git(
        &graph,
        &[
            "init",
            "-q",
            "-b",
            "main",
            "--separate-git-dir",
            &gitdir.to_string_lossy(),
        ],
    );
    git(&graph, &["config", "--local", "user.name", "T"]);
    git(
        &graph,
        &["config", "--local", "user.email", "t@example.com"],
    );
    commit_file(&graph, "pages/Home.md", "- home\n", "first");
    assert!(graph.join(".git").is_file());
    assert_eq!(detect_separate_gitdir(&graph), Some(gitdir.clone()));

    let config = configs().remove(0).1;
    let err = enable_sync(&graph, &env.url(), "main", &config).unwrap_err();
    assert!(matches!(err, OnboardingError::SeparateGitdir(_)), "{err:?}");

    let new_dir = migrate_separate_gitdir(&graph).unwrap();
    assert!(new_dir.is_dir());
    assert!(graph.join(".git").is_dir());
    assert_eq!(detect_separate_gitdir(&graph), None);
    // Standard in-folder repo now: status works, history intact, no core.worktree left behind.
    assert_eq!(git(&graph, &["status", "--porcelain"]), "");
    assert_eq!(git(&graph, &["log", "-1", "--format=%s"]), "first");
    let worktree = bitacora_testkit::git::git_raw(&graph, &["config", "--local", "core.worktree"]);
    assert!(
        !worktree.status.success(),
        "core.worktree should be removed"
    );
    // The original gitdir is left in place.
    assert!(gitdir.is_dir());
    // And sync can now be enabled.
    let out = enable_sync(&graph, &env.url(), "main", &config).unwrap();
    assert_eq!(out.remote, RemoteState::EmptyRemotePushed);
}
