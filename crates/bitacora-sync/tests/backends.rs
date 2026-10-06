//! Integration tests for the `GitBackend` implementations against temp repositories and bare
//! remotes. Every scenario runs against the hybrid (system git + gix) and the gix-only backend.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use bitacora_sync::backend::{DirtyKind, GitError};
use bitacora_sync::{
    ActiveBackend, CliBackend, CliConfig, CommitKind, CommitMessage, CommitOpts, GitBackend,
    GitDetection, GixBackend, Oid, TreeChange, TreeEdit, detect_git, select_backend,
};
use bitacora_testkit::git::{
    clone_to, commit_file, git, git_available, git_raw, init_bare, init_repo, write_file,
};
use tempfile::TempDir;

fn oid(hex: &str) -> Oid {
    Oid::from_hex(hex).unwrap()
}

/// All backends usable on this host for `repo`.
fn backends(repo: &Path) -> Vec<(&'static str, Box<dyn GitBackend>)> {
    let mut out: Vec<(&'static str, Box<dyn GitBackend>)> = Vec::new();
    if let det @ GitDetection::Found { .. } = detect_git(None) {
        out.push((
            "hybrid",
            select_backend(&det, repo, CliConfig::default()).unwrap(),
        ));
    }
    out.push(("gix", Box::new(GixBackend::open(repo).unwrap())));
    out
}

struct World {
    _tmp: TempDir,
    remote: PathBuf,
    root: PathBuf,
}

impl World {
    fn new() -> Self {
        assert!(git_available(), "tests need a system git");
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let remote = root.join("remote.git");
        init_bare(&remote);
        Self {
            _tmp: tmp,
            remote,
            root,
        }
    }

    fn url(&self) -> String {
        self.remote.to_string_lossy().into_owned()
    }

    /// A fresh clone of the remote named `name`.
    fn clone(&self, name: &str) -> PathBuf {
        let dest = self.root.join(name);
        clone_to(&self.url(), &dest);
        dest
    }

    /// A new repo with one commit and `origin` set to the bare remote.
    fn seeded(&self, name: &str) -> PathBuf {
        let dir = self.root.join(name);
        init_repo(&dir);
        git(&dir, &["remote", "add", "origin", &self.url()]);
        commit_file(&dir, "pages/a.md", "- hello\n", "seed");
        dir
    }
}

fn remote_head(remote: &Path) -> String {
    git(remote, &["rev-parse", "refs/heads/main"])
}

fn msg(subject: &str) -> CommitMessage {
    CommitMessage::new(subject).kind(CommitKind::Auto)
}

#[test]
fn push_fetch_roundtrip_both_backends() {
    for kind in ["hybrid", "gix"] {
        let w = World::new();
        let a = w.seeded("a");
        let (_, backend) = backends(&a)
            .into_iter()
            .find(|(n, _)| *n == kind)
            .unwrap_or_else(|| panic!("{kind} unavailable"));
        assert!(backend.push("origin", "main").unwrap().pushed, "{kind}");
        assert_eq!(remote_head(&w.remote), git(&a, &["rev-parse", "HEAD"]));
        // Pushing again is a no-op.
        assert!(!backend.push("origin", "main").unwrap().pushed, "{kind}");

        // Another device commits and pushes; fetching sees it.
        let b = w.clone("b");
        let b_head = commit_file(&b, "pages/b.md", "- from b\n", "b");
        git(&b, &["push", "-q", "origin", "main"]);
        let outcome = backend.fetch("origin", "main").unwrap();
        assert!(outcome.updated, "{kind}");
        assert_eq!(outcome.remote_head.unwrap().as_hex(), b_head, "{kind}");
        // No new changes: not updated.
        assert!(!backend.fetch("origin", "main").unwrap().updated, "{kind}");
    }
}

#[test]
fn rejected_push_is_non_fast_forward_and_remote_untouched() {
    for kind in ["hybrid", "gix"] {
        let w = World::new();
        let a = w.seeded("a");
        git(&a, &["push", "-q", "origin", "main"]);
        let b = w.clone("b");
        commit_file(&b, "pages/b.md", "- b\n", "b");
        git(&b, &["push", "-q", "origin", "main"]);
        let before = remote_head(&w.remote);
        commit_file(&a, "pages/a2.md", "- a2\n", "a2");
        let (_, backend) = backends(&a).into_iter().find(|(n, _)| *n == kind).unwrap();
        let err = backend.push("origin", "main").unwrap_err();
        assert!(matches!(err, GitError::NonFastForward), "{kind}: {err:?}");
        assert_eq!(remote_head(&w.remote), before, "{kind}: remote changed");
    }
}

#[test]
fn fetch_of_missing_branch_reports_remote_ref_not_found() {
    let w = World::new();
    let a = w.seeded("a");
    for (n, b) in backends(&a) {
        let err = b.fetch("origin", "main").unwrap_err();
        assert!(
            matches!(err, GitError::RemoteRefNotFound(_)),
            "{n}: {err:?}"
        );
    }
}

#[test]
fn commit_tree_two_parents_and_cas_update_ref() {
    for kind in ["hybrid", "gix"] {
        let w = World::new();
        let a = w.seeded("a");
        let c1 = oid(&git(&a, &["rev-parse", "HEAD"]));
        let (_, backend) = backends(&a).into_iter().find(|(n, _)| *n == kind).unwrap();
        let t2 = backend
            .write_tree(
                &c1,
                &[TreeEdit::Upsert {
                    path: "pages/two.md".into(),
                    content: b"- two\n".to_vec(),
                }],
            )
            .unwrap();
        let c2 = backend
            .commit_tree(&t2, std::slice::from_ref(&c1), &msg("two"))
            .unwrap();
        let t3 = backend
            .write_tree(
                &c1,
                &[TreeEdit::Upsert {
                    path: "pages/three.md".into(),
                    content: b"- three\n".to_vec(),
                }],
            )
            .unwrap();
        let c3 = backend
            .commit_tree(&t3, std::slice::from_ref(&c1), &msg("three"))
            .unwrap();
        // Merge tree combines both edits.
        let tm = backend
            .write_tree(
                &t2,
                &[TreeEdit::Upsert {
                    path: "pages/three.md".into(),
                    content: b"- three\n".to_vec(),
                }],
            )
            .unwrap();
        let merge = backend
            .commit_tree(&tm, &[c2.clone(), c3.clone()], &msg("merge"))
            .unwrap();
        let parents = git(&a, &["rev-list", "--parents", "-n1", merge.as_hex()]);
        assert_eq!(
            parents,
            format!("{} {} {}", merge, c2, c3),
            "{kind}: parents"
        );
        // CAS: create requires absence; stale expected value fails; correct value succeeds.
        backend
            .update_ref("refs/heads/merged", &merge, None)
            .unwrap();
        assert!(
            matches!(
                backend.update_ref("refs/heads/merged", &c2, None),
                Err(GitError::RefChanged(_))
            ),
            "{kind}: create-over-existing"
        );
        assert!(
            matches!(
                backend.update_ref("refs/heads/merged", &c2, Some(&c1)),
                Err(GitError::RefChanged(_))
            ),
            "{kind}: stale expected"
        );
        backend
            .update_ref("refs/heads/merged", &c2, Some(&merge))
            .unwrap();
        assert_eq!(git(&a, &["rev-parse", "refs/heads/merged"]), c2.as_hex());
        git(&a, &["fsck", "--no-dangling"]);
    }
}

#[test]
fn write_tree_matches_git_write_tree() {
    let w = World::new();
    let a = w.seeded("a");
    commit_file(&a, "pages/keep.md", "- keep\n", "keep");
    let base = oid(&git(&a, &["rev-parse", "HEAD"]));
    let edits = vec![
        TreeEdit::Upsert {
            path: "pages/a.md".into(),
            content: b"- changed\n".to_vec(),
        },
        TreeEdit::Upsert {
            path: "journals/2026_10_06.md".into(),
            content: b"- today\n".to_vec(),
        },
        TreeEdit::Remove {
            path: "pages/keep.md".into(),
        },
    ];
    let gix_tree = GixBackend::open(&a)
        .unwrap()
        .write_tree(&base, &edits)
        .unwrap();
    // The reference: apply the same edits in the work tree and ask git.
    write_file(&a, "pages/a.md", "- changed\n");
    write_file(&a, "journals/2026_10_06.md", "- today\n");
    std::fs::remove_file(a.join("pages/keep.md")).unwrap();
    git(&a, &["add", "-A"]);
    let expected = git(&a, &["write-tree"]);
    assert_eq!(gix_tree.as_hex(), expected);
    // Work tree and index were not touched by write_tree itself: HEAD unchanged.
    assert_eq!(git(&a, &["rev-parse", "HEAD"]), base.as_hex());
    // CLI write_tree agrees.
    if let GitDetection::Found { path, .. } = detect_git(None) {
        let cli = CliBackend::new(&a, path, CliConfig::default());
        assert_eq!(cli.write_tree(&base, &edits).unwrap(), gix_tree);
    }
}

#[test]
fn diff_trees_detects_renames_with_edits_and_id_sets() {
    let w = World::new();
    let a = w.seeded("a");
    let body: String = (0..20).map(|i| format!("- line number {i}\n")).collect();
    commit_file(&a, "pages/Old.md", &body, "add old");
    let base = oid(&git(&a, &["rev-parse", "HEAD"]));
    // Rename with a small edit (still >= 50% similar).
    git(&a, &["mv", "pages/Old.md", "pages/New.md"]);
    write_file(&a, "pages/New.md", &format!("{body}- extra\n"));
    // A page whose text is rewritten but keeps its block ids (strong-signal rename).
    let ids = "- first\n  id:: 6a1b2c3d-0000-4000-8000-000000000001\n- second\n  id:: 6a1b2c3d-0000-4000-8000-000000000002\n";
    write_file(&a, "pages/Ided.md", ids);
    git(&a, &["add", "-A"]);
    git(&a, &["commit", "-q", "-m", "ided"]);
    let with_ids = oid(&git(&a, &["rev-parse", "HEAD"]));
    git(&a, &["rm", "-q", "pages/Ided.md"]);
    write_file(
        &a,
        "pages/Renamed Ided.md",
        "- completely different words here\n  id:: 6a1b2c3d-0000-4000-8000-000000000001\n- other text altogether and more\n  id:: 6a1b2c3d-0000-4000-8000-000000000002\n- and a third unrelated line to dilute similarity\n- plus a fourth one\n",
    );
    git(&a, &["add", "-A"]);
    git(&a, &["commit", "-q", "-m", "rename ided"]);
    let head = oid(&git(&a, &["rev-parse", "HEAD"]));

    let gix = GixBackend::open(&a).unwrap();
    let first = gix.diff_trees(&base, &with_ids).unwrap();
    assert!(
        first.iter().any(|c| matches!(c,
        TreeChange::Renamed { from, to, similarity }
            if from == "pages/Old.md" && to == "pages/New.md" && *similarity >= 50)),
        "{first:?}"
    );
    assert!(
        first.iter().any(|c| matches!(c,
        TreeChange::Added { path } if path == "pages/Ided.md")),
        "{first:?}"
    );

    let second = gix.diff_trees(&with_ids, &head).unwrap();
    assert!(
        second.iter().any(|c| matches!(c,
            TreeChange::Renamed { from, to, .. }
                if from == "pages/Ided.md" && to == "pages/Renamed Ided.md")),
        "id-set rename not detected: {second:?}"
    );
}

#[test]
fn status_reports_dirty_and_unmerged_identically() {
    let w = World::new();
    let a = w.seeded("a");
    commit_file(&a, "pages/c.txt", "base\n", "c");
    commit_file(&a, "pages/gone.md", "- gone\n", "gone");
    // Make a conflict between two branches.
    git(&a, &["checkout", "-q", "-b", "side"]);
    commit_file(&a, "pages/c.txt", "side\n", "side");
    git(&a, &["checkout", "-q", "main"]);
    commit_file(&a, "pages/c.txt", "main\n", "main");
    let merge = git_raw(&a, &["merge", "side"]);
    assert!(!merge.status.success(), "expected a conflict");
    // Plus dirty paths.
    write_file(&a, "pages/a.md", "- modified\n");
    write_file(&a, "pages/new.md", "- untracked\n");
    std::fs::remove_file(a.join("pages/gone.md")).unwrap();

    let results: Vec<_> = backends(&a)
        .into_iter()
        .map(|(n, b)| (n, b.status().unwrap()))
        .collect();
    for (n, st) in &results {
        assert_eq!(st.branch.as_deref(), Some("main"), "{n}");
        assert_eq!(st.unmerged.len(), 1, "{n}: {st:?}");
        let u = &st.unmerged[0];
        assert_eq!(u.path, "pages/c.txt");
        assert!(
            u.base.is_some() && u.ours.is_some() && u.theirs.is_some(),
            "{n}"
        );
        let kinds: Vec<(&str, DirtyKind)> =
            st.dirty.iter().map(|d| (d.path.as_str(), d.kind)).collect();
        assert!(
            kinds.contains(&("pages/a.md", DirtyKind::Modified)),
            "{n}: {kinds:?}"
        );
        assert!(
            kinds.contains(&("pages/new.md", DirtyKind::Untracked)),
            "{n}: {kinds:?}"
        );
        assert!(
            kinds.contains(&("pages/gone.md", DirtyKind::Deleted)),
            "{n}: {kinds:?}"
        );
    }
    // Stage ids match git's own listing.
    let listing = git(&a, &["ls-files", "-u", "pages/c.txt"]);
    let (_, st) = &results[0];
    for line in listing.lines() {
        let id = line.split_whitespace().nth(1).unwrap();
        let stage = line.split_whitespace().nth(2).unwrap();
        let u = &st.unmerged[0];
        let got = match stage {
            "1" => &u.base,
            "2" => &u.ours,
            _ => &u.theirs,
        };
        assert_eq!(got.as_ref().unwrap().as_hex(), id);
    }
    for pair in results.windows(2) {
        assert_eq!(pair[0].1, pair[1].1, "backends disagree on status");
    }
}

#[test]
fn commit_with_stage_all_and_amend_leave_repo_clean() {
    for kind in ["hybrid", "gix"] {
        let w = World::new();
        let a = w.seeded("a");
        let (_, backend) = backends(&a).into_iter().find(|(n, _)| *n == kind).unwrap();
        write_file(&a, "pages/a.md", "- edited\n");
        write_file(&a, "pages/n.md", "- new\n");
        let message = CommitMessage::new("bitacora: edit 2 pages")
            .trailer("Bitacora-Device", "test")
            .kind(CommitKind::Auto);
        let first = backend
            .commit(
                &message,
                CommitOpts {
                    stage_all: true,
                    ..CommitOpts::default()
                },
            )
            .unwrap();
        assert_eq!(git(&a, &["rev-parse", "HEAD"]), first.as_hex(), "{kind}");
        assert_eq!(
            git(&a, &["status", "--porcelain"]),
            "",
            "{kind}: dirty after commit"
        );
        let body = git(&a, &["log", "-1", "--format=%B"]);
        assert!(body.contains("Bitacora-Kind: auto"), "{kind}: {body}");
        // Nothing left to commit.
        assert!(
            backend
                .commit(
                    &message,
                    CommitOpts {
                        stage_all: true,
                        ..CommitOpts::default()
                    }
                )
                .is_err()
        );
        // Amend squashes into the same parent.
        write_file(&a, "pages/n.md", "- new again\n");
        let amended = backend
            .commit(
                &message,
                CommitOpts {
                    stage_all: true,
                    amend: true,
                    ..CommitOpts::default()
                },
            )
            .unwrap();
        assert_ne!(amended, first, "{kind}");
        assert_eq!(git(&a, &["rev-list", "--count", "HEAD"]), "2", "{kind}");
        assert_eq!(git(&a, &["status", "--porcelain"]), "", "{kind}");
        git(&a, &["fsck", "--no-dangling"]);
    }
}

#[test]
fn read_blob_and_merge_base_agree_with_git() {
    let w = World::new();
    let a = w.seeded("a");
    let base = oid(&git(&a, &["rev-parse", "HEAD"]));
    git(&a, &["checkout", "-q", "-b", "x"]);
    let x = oid(&commit_file(&a, "pages/x.md", "- x\n", "x"));
    git(&a, &["checkout", "-q", "main"]);
    let y = oid(&commit_file(&a, "pages/y.md", "- y\n", "y"));
    for (n, b) in backends(&a) {
        assert_eq!(b.merge_base(&x, &y).unwrap(), Some(base.clone()), "{n}");
        assert_eq!(
            b.read_blob(&x, "pages/x.md").unwrap().as_deref(),
            Some(&b"- x\n"[..]),
            "{n}"
        );
        assert_eq!(b.read_blob(&x, "pages/y.md").unwrap(), None, "{n}");
        assert_eq!(
            b.read_blob(&x, "pages").unwrap(),
            None,
            "{n}: directory is not a blob"
        );
    }
}

#[test]
fn unrelated_histories_have_no_merge_base() {
    let w = World::new();
    let a = w.seeded("a");
    let one = oid(&git(&a, &["rev-parse", "HEAD"]));
    git(&a, &["checkout", "-q", "--orphan", "other"]);
    git(&a, &["rm", "-rfq", "."]);
    let two = oid(&commit_file(&a, "z.md", "- z\n", "orphan"));
    for (n, b) in backends(&a) {
        assert_eq!(b.merge_base(&one, &two).unwrap(), None, "{n}");
    }
}

#[test]
fn clone_with_both_backends() {
    let w = World::new();
    let a = w.seeded("a");
    git(&a, &["push", "-q", "origin", "main"]);
    let det = detect_git(None);
    let GitDetection::Found { path, .. } = det else {
        panic!("git required");
    };
    let via_cli = w.root.join("cli-clone");
    CliBackend::clone_repo(&path, &w.url(), &via_cli, &CliConfig::default()).unwrap();
    assert!(via_cli.join("pages/a.md").is_file());
    let via_gix = w.root.join("gix-clone");
    GixBackend::clone_repo(&w.url(), &via_gix).unwrap();
    assert!(via_gix.join("pages/a.md").is_file());
    assert_eq!(
        git(&via_cli, &["rev-parse", "HEAD"]),
        git(&via_gix, &["rev-parse", "HEAD"])
    );
}

#[test]
fn ls_remote_reports_tip() {
    let w = World::new();
    let a = w.seeded("a");
    let GitDetection::Found { path, .. } = detect_git(None) else {
        panic!("git required");
    };
    let cli = CliBackend::new(&a, path, CliConfig::default());
    assert_eq!(cli.ls_remote("origin", "main").unwrap(), None);
    git(&a, &["push", "-q", "origin", "main"]);
    let tip = oid(&git(&a, &["rev-parse", "HEAD"]));
    assert_eq!(cli.ls_remote("origin", "main").unwrap(), Some(tip.clone()));
    let gix = GixBackend::open(&a).unwrap();
    assert_eq!(gix.ls_remote("origin", "main").unwrap(), Some(tip));
}

#[test]
fn select_backend_kinds() {
    let w = World::new();
    let a = w.seeded("a");
    let det = detect_git(None);
    if matches!(det, GitDetection::Found { .. }) {
        let b = select_backend(&det, &a, CliConfig::default()).unwrap();
        assert_eq!(b.kind(), ActiveBackend::Hybrid);
    }
    let b = select_backend(&GitDetection::Missing, &a, CliConfig::default()).unwrap();
    assert_eq!(b.kind(), ActiveBackend::GixOnly);
    assert!(matches!(
        GixBackend::open(&w.root.join("not-a-repo")),
        Err(GitError::NotARepo)
    ));
}

/// A tiny HTTP server that answers every request with `401 Unauthorized`.
fn serve_401() -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    listener.set_nonblocking(false).unwrap();
    let handle = std::thread::spawn(move || {
        // git may retry once; serve a couple of connections then stop.
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"x\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    (format!("http://{addr}/repo.git"), handle)
}

#[test]
fn cli_classifies_auth_and_network_failures_without_prompting() {
    let w = World::new();
    let a = w.seeded("a");
    let GitDetection::Found { path, .. } = detect_git(None) else {
        panic!("git required");
    };
    let cli = CliBackend::new(&a, path, CliConfig::default());

    let (url, server) = serve_401();
    git(&a, &["remote", "add", "authfail", &url]);
    let err = cli.fetch("authfail", "main").unwrap_err();
    assert!(matches!(err, GitError::Auth { .. }), "{err:?}");
    drop(server);

    // Nothing listens on port 1.
    git(
        &a,
        &["remote", "add", "down", "http://127.0.0.1:1/repo.git"],
    );
    let err = cli.fetch("down", "main").unwrap_err();
    assert!(matches!(err, GitError::Network(_)), "{err:?}");

    // Not a repository.
    let plain = w.root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let GitDetection::Found { path, .. } = detect_git(None) else {
        unreachable!()
    };
    let cli = CliBackend::new(&plain, path, CliConfig::default());
    // GIT_CEILING_DIRECTORIES is not set, but the temp dir is outside any repo.
    let err = cli.fetch("origin", "main").unwrap_err();
    assert!(
        matches!(err, GitError::NotARepo | GitError::Other { .. }),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn cli_runs_git_with_non_interactive_environment() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("env.txt");
    let script = tmp.path().join("fake-git");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n{{ env; echo \"ARGS: $*\"; }} > '{}'\nexit 0\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let askpass = tmp.path().join("askpass");
    let cli = CliBackend::new(
        tmp.path(),
        script,
        CliConfig {
            askpass: Some(askpass.clone()),
            ..CliConfig::default()
        },
    );
    assert_eq!(cli.ls_remote("origin", "main").unwrap(), None);
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("GIT_TERMINAL_PROMPT=0"), "{text}");
    assert!(text.contains("LC_ALL=C"), "{text}");
    assert!(
        text.contains(&format!("GIT_ASKPASS={}", askpass.display())),
        "{text}"
    );
    assert!(
        text.contains("-c core.quotepath=false -c core.autocrlf=false ls-remote"),
        "{text}"
    );
}

#[test]
fn cli_push_never_forces() {
    // The push refspec has no leading `+` and no force flag: a diverged remote is rejected.
    let w = World::new();
    let a = w.seeded("a");
    git(&a, &["push", "-q", "origin", "main"]);
    let b = w.clone("b");
    commit_file(&b, "pages/b.md", "- b\n", "b");
    git(&b, &["push", "-q", "origin", "main"]);
    let remote_before = remote_head(&w.remote);
    // Rewrite local history so a force push would succeed.
    git(
        &a,
        &[
            "commit",
            "-q",
            "--amend",
            "-m",
            "rewritten",
            "--allow-empty",
        ],
    );
    let GitDetection::Found { path, .. } = detect_git(None) else {
        panic!("git required");
    };
    let cli = CliBackend::new(&a, path, CliConfig::default());
    assert!(matches!(
        cli.push("origin", "main"),
        Err(GitError::NonFastForward)
    ));
    assert_eq!(remote_head(&w.remote), remote_before);
}

// ---- Cross-check: GixBackend vs git CLI on generated histories --------------------------------

/// Small deterministic generator (no external rand dependency).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn page_text(rng: &mut Lcg, lines: u64) -> String {
    (0..lines)
        .map(|_| format!("- block {:x}\n", rng.next()))
        .collect()
}

fn tracked_files(repo: &Path, rev: &str) -> Vec<String> {
    let out = git(repo, &["ls-tree", "-r", "--name-only", rev]);
    out.lines().map(str::to_string).collect()
}

/// Order- and similarity-insensitive classification used for comparing backends.
fn classify(changes: &[TreeChange]) -> Vec<String> {
    let mut v: Vec<String> = changes
        .iter()
        .map(|c| match c {
            TreeChange::Added { path } => format!("A {path}"),
            TreeChange::Deleted { path } => format!("D {path}"),
            TreeChange::Modified { path } => format!("M {path}"),
            TreeChange::Renamed { from, to, .. } => format!("R {from} -> {to}"),
        })
        .collect();
    v.sort();
    v
}

#[test]
fn gix_matches_git_cli_on_generated_histories() {
    let GitDetection::Found { path: git_path, .. } = detect_git(None) else {
        panic!("git required");
    };
    for seed in 1..=6u64 {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("r");
        init_repo(&repo);
        let mut rng = Lcg(seed * 7919);
        let mut commits: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for step in 0..14 {
            match rng.below(5) {
                // add a page
                0 | 1 => {
                    let name = format!("pages/p{step}.md");
                    let lines = 8 + rng.below(8);
                    write_file(&repo, &name, &page_text(&mut rng, lines));
                    files.push(name);
                }
                // edit a page
                2 if !files.is_empty() => {
                    let f = files[rng.below(files.len() as u64) as usize].clone();
                    let old = std::fs::read_to_string(repo.join(&f)).unwrap_or_default();
                    write_file(&repo, &f, &format!("{old}- edit {step}\n"));
                }
                // rename a page (pure move)
                3 if !files.is_empty() => {
                    let idx = rng.below(files.len() as u64) as usize;
                    let from = files[idx].clone();
                    if repo.join(&from).exists() {
                        let to = format!("pages/moved{step}.md");
                        git(&repo, &["mv", &from, &to]);
                        files[idx] = to;
                    }
                }
                // delete a page
                _ if files.len() > 2 => {
                    let idx = rng.below(files.len() as u64) as usize;
                    let f = files.remove(idx);
                    let _ = std::fs::remove_file(repo.join(&f));
                }
                _ => {
                    write_file(&repo, &format!("pages/q{step}.md"), &page_text(&mut rng, 9));
                    files.push(format!("pages/q{step}.md"));
                }
            }
            git(&repo, &["add", "-A"]);
            if git_raw(&repo, &["commit", "-q", "-m", &format!("step {step}")])
                .status
                .success()
            {
                commits.push(git(&repo, &["rev-parse", "HEAD"]));
            }
        }
        // A side branch for merge-base coverage.
        let mid = commits[commits.len() / 2].clone();
        git(&repo, &["checkout", "-q", "-b", "side", &mid]);
        write_file(&repo, "pages/side.md", &page_text(&mut rng, 10));
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "side"]);
        commits.push(git(&repo, &["rev-parse", "HEAD"]));

        let gix = GixBackend::open(&repo).unwrap();
        let cli = CliBackend::new(&repo, git_path.clone(), CliConfig::default());
        for a in &commits {
            for b in &commits {
                let (oa, ob) = (oid(a), oid(b));
                assert_eq!(
                    gix.merge_base(&oa, &ob).unwrap(),
                    cli.merge_base(&oa, &ob).unwrap(),
                    "seed {seed}: merge_base {a} {b}"
                );
                let g = classify(&gix.diff_trees(&oa, &ob).unwrap());
                let c = classify(&cli.diff_trees(&oa, &ob).unwrap());
                assert_eq!(g, c, "seed {seed}: diff {a} {b}");
            }
            for f in tracked_files(&repo, a) {
                assert_eq!(
                    gix.read_blob(&oid(a), &f).unwrap(),
                    cli.read_blob(&oid(a), &f).unwrap(),
                    "seed {seed}: blob {a}:{f}"
                );
            }
            assert_eq!(gix.read_blob(&oid(a), "pages/nope.md").unwrap(), None);
        }
    }
}
