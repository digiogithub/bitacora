//! `CliBackend`: drives the system `git` binary (ADR-007, ADR-020).
//!
//! Every invocation runs non-interactively (`GIT_TERMINAL_PROMPT=0`, `LC_ALL=C`, quotepath and
//! autocrlf disabled) and never touches global git config. Pushes are never forced.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    ActiveBackend, CommitInfo, CommitMessage, CommitOpts, DirtyKind, DirtyPath, FetchOutcome,
    GitBackend, GitError, Oid, PushOutcome, RepoStatus, Result, TreeChange, TreeEdit,
    UnmergedEntry, check_ref_arg, classify_failure,
};

const NULL_OID: &str = "0000000000000000000000000000000000000000";

/// Tunables for [`CliBackend`].
#[derive(Debug, Clone)]
pub struct CliConfig {
    /// Helper executable for `GIT_ASKPASS` (the in-app askpass); `None` leaves it unset so git
    /// fails fast instead of prompting.
    pub askpass: Option<PathBuf>,
    /// Timeout for fetch/push/clone/ls-remote (default 120 s).
    pub network_timeout: Duration,
    /// Timeout for local operations (default 60 s).
    pub local_timeout: Duration,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            askpass: None,
            network_timeout: Duration::from_secs(120),
            local_timeout: Duration::from_secs(60),
        }
    }
}

/// Captured output of a successful git run.
#[derive(Debug)]
pub(crate) struct Output {
    pub stdout: Vec<u8>,
}

impl Output {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }
}

/// Backend running the system git binary inside one repository.
#[derive(Debug, Clone)]
pub struct CliBackend {
    git: PathBuf,
    repo: PathBuf,
    config: CliConfig,
}

impl CliBackend {
    /// Creates a backend for the repository whose work tree is `repo`.
    pub fn new(repo: &Path, git: PathBuf, config: CliConfig) -> Self {
        Self {
            git,
            repo: repo.to_path_buf(),
            config,
        }
    }

    /// Clones `url` into `dest` (`git clone`), non-interactively.
    pub fn clone_repo(git: &Path, url: &str, dest: &Path, config: &CliConfig) -> Result<()> {
        if url.starts_with('-') {
            return Err(GitError::other("invalid clone url"));
        }
        let mut args = vec!["clone".to_string(), "--".into(), url.to_string()];
        args.push(dest.to_string_lossy().into_owned());
        run(git, None, config, &args, None, config.network_timeout).map(|_| ())
    }

    /// `git ls-remote <remote> refs/heads/<branch>`; returns the remote tip if the branch exists.
    pub fn ls_remote(&self, remote: &str, branch: &str) -> Result<Option<Oid>> {
        check_ref_arg(remote)?;
        check_ref_arg(branch)?;
        let out = self.network(&["ls-remote", remote, &format!("refs/heads/{branch}")])?;
        match out.text().split_whitespace().next() {
            Some(hex) => Ok(Some(Oid::from_hex(hex)?)),
            None => Ok(None),
        }
    }

    fn local(&self, args: &[&str]) -> Result<Output> {
        self.exec(args, None, self.config.local_timeout)
    }

    fn network(&self, args: &[&str]) -> Result<Output> {
        self.exec(args, None, self.config.network_timeout)
    }

    fn exec(&self, args: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> Result<Output> {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        run(
            &self.git,
            Some(&self.repo),
            &self.config,
            &args,
            stdin,
            timeout,
        )
    }

    fn exec_env(&self, args: &[&str], env: &[(&str, &Path)]) -> Result<Output> {
        let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        run_with_env(
            &self.git,
            Some(&self.repo),
            &self.config,
            &args,
            None,
            self.config.local_timeout,
            env,
        )
    }

    fn rev_parse(&self, rev: &str) -> Result<Option<Oid>> {
        match self.local(&["rev-parse", "--verify", "--quiet", rev]) {
            Ok(out) => Ok(Some(Oid::from_hex(&out.text())?)),
            Err(GitError::Other { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn run(
    git: &Path,
    cwd: Option<&Path>,
    config: &CliConfig,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<Output> {
    run_with_env(git, cwd, config, args, stdin, timeout, &[])
}

fn run_with_env(
    git: &Path,
    cwd: Option<&Path>,
    config: &CliConfig,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
    extra_env: &[(&str, &Path)],
) -> Result<Output> {
    let mut cmd = Command::new(git);
    cmd.arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg("core.autocrlf=false")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // GIT_ASKPASS: the in-app helper if configured, otherwise empty so git neither prompts on a
    // terminal nor inherits a stray helper that would block.
    cmd.env(
        "GIT_ASKPASS",
        config.askpass.as_deref().unwrap_or(Path::new("")),
    );
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    // ETXTBSY (26) happens on Linux when another thread is forking while the binary was just
    // written (e.g. a wrapper script); a short retry makes this robust.
    let mut attempts = 0;
    let mut child = loop {
        match cmd.spawn() {
            Ok(child) => break child,
            Err(e) if e.raw_os_error() == Some(26) && attempts < 20 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(e.into()),
        }
    };
    if let (Some(data), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let data = data.to_vec();
        thread::spawn(move || {
            let _ = pipe.write_all(&data);
        });
    }
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let err_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(GitError::Network(format!(
                "git {} timed out after {}s",
                args.first().map(String::as_str).unwrap_or(""),
                timeout.as_secs()
            )));
        }
        thread::sleep(Duration::from_millis(5));
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    if status.success() {
        Ok(Output { stdout })
    } else {
        let mut message = stderr;
        if message.trim().is_empty() {
            message = String::from_utf8_lossy(&stdout).into_owned();
        }
        Err(classify_failure(&message))
    }
}

fn split_nul(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

impl GitBackend for CliBackend {
    fn fetch(&self, remote: &str, branch: &str) -> Result<FetchOutcome> {
        check_ref_arg(remote)?;
        check_ref_arg(branch)?;
        let tracking = format!("refs/remotes/{remote}/{branch}");
        let before = self.rev_parse(&tracking)?;
        let spec = format!("+refs/heads/{branch}:{tracking}");
        self.network(&["fetch", "--prune", "--no-tags", remote, &spec])?;
        let after = self.rev_parse(&tracking)?;
        Ok(FetchOutcome {
            updated: before != after,
            remote_head: after,
        })
    }

    fn push(&self, remote: &str, branch: &str) -> Result<PushOutcome> {
        check_ref_arg(remote)?;
        check_ref_arg(branch)?;
        let before = self.rev_parse(&format!("refs/remotes/{remote}/{branch}"))?;
        let head = self.rev_parse("HEAD")?;
        let spec = format!("HEAD:refs/heads/{branch}");
        // Deliberately no --force / --force-with-lease / leading '+'.
        self.network(&["push", remote, &spec])?;
        Ok(PushOutcome {
            pushed: head.is_some() && before != head,
        })
    }

    fn merge_base(&self, a: &Oid, b: &Oid) -> Result<Option<Oid>> {
        match self.local(&["merge-base", a.as_hex(), b.as_hex()]) {
            Ok(out) => Ok(Some(Oid::from_hex(&out.text())?)),
            // exit 1 with no output means "no common ancestor", which the classifier maps to Other.
            Err(GitError::Other { stderr }) if stderr.is_empty() => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn read_blob(&self, commit: &Oid, path: &str) -> Result<Option<Vec<u8>>> {
        let spec = format!("{}:{path}", commit.as_hex());
        if self.local(&["cat-file", "-e", &spec]).is_err() {
            return Ok(None);
        }
        Ok(Some(self.local(&["cat-file", "blob", &spec])?.stdout))
    }

    fn diff_trees(&self, a: &Oid, b: &Oid) -> Result<Vec<TreeChange>> {
        let out = self.local(&[
            "diff-tree",
            "-r",
            "-z",
            "--name-status",
            "-M50%",
            a.as_hex(),
            b.as_hex(),
        ])?;
        let fields = split_nul(&out.stdout);
        let mut it = fields.into_iter();
        let mut changes = Vec::new();
        while let Some(status) = it.next() {
            match status.chars().next() {
                Some('A') => changes.push(TreeChange::Added {
                    path: it.next().unwrap_or_default(),
                }),
                Some('D') => changes.push(TreeChange::Deleted {
                    path: it.next().unwrap_or_default(),
                }),
                Some('M' | 'T') => changes.push(TreeChange::Modified {
                    path: it.next().unwrap_or_default(),
                }),
                Some('R') => {
                    let similarity = status[1..].parse().unwrap_or(0);
                    let from = it.next().unwrap_or_default();
                    let to = it.next().unwrap_or_default();
                    changes.push(TreeChange::Renamed {
                        from,
                        to,
                        similarity,
                    });
                }
                _ => {}
            }
        }
        Ok(changes)
    }

    fn write_tree(&self, base: &Oid, edits: &[TreeEdit]) -> Result<Oid> {
        // A throw-away index file keeps the real index and work tree untouched.
        let dir = tempfile::tempdir()?;
        let index = dir.path().join("index");
        let env = [("GIT_INDEX_FILE", index.as_path())];
        self.exec_env(&["read-tree", base.as_hex()], &env)?;
        for edit in edits {
            match edit {
                TreeEdit::Upsert { path, content } => {
                    let file = dir.path().join("blob");
                    std::fs::write(&file, content)?;
                    let blob = self
                        .local(&["hash-object", "-w", "--no-filters", &file.to_string_lossy()])?
                        .text();
                    let cacheinfo = format!("100644,{blob},{path}");
                    self.exec_env(&["update-index", "--add", "--cacheinfo", &cacheinfo], &env)?;
                }
                TreeEdit::Remove { path } => {
                    self.exec_env(&["update-index", "--force-remove", "--", path], &env)?;
                }
            }
        }
        let out = self.exec_env(&["write-tree"], &env)?;
        Oid::from_hex(&out.text())
    }

    fn commit(&self, msg: &CommitMessage, opts: CommitOpts) -> Result<Oid> {
        if opts.stage_all {
            self.local(&["add", "-A"])?;
        }
        let mut args = vec!["commit", "-F", "-"];
        if opts.amend {
            args.push("--amend");
        }
        if opts.allow_empty {
            args.push("--allow-empty");
        }
        self.exec(
            &args,
            Some(msg.render().as_bytes()),
            self.config.local_timeout,
        )?;
        self.rev_parse("HEAD")?
            .ok_or_else(|| GitError::other("HEAD missing after commit"))
    }

    fn commit_tree(&self, tree: &Oid, parents: &[Oid], msg: &CommitMessage) -> Result<Oid> {
        let mut args = vec!["commit-tree".to_string(), tree.as_hex().to_string()];
        for p in parents {
            args.push("-p".into());
            args.push(p.as_hex().to_string());
        }
        args.push("-F".into());
        args.push("-".into());
        let out = run(
            &self.git,
            Some(&self.repo),
            &self.config,
            &args,
            Some(msg.render().as_bytes()),
            self.config.local_timeout,
        )?;
        Oid::from_hex(&out.text())
    }

    fn update_ref(&self, name: &str, new: &Oid, expected_old: Option<&Oid>) -> Result<()> {
        check_ref_arg(name)?;
        let old = expected_old.map_or(NULL_OID, Oid::as_hex);
        match self.local(&["update-ref", name, new.as_hex(), old]) {
            Ok(_) => Ok(()),
            Err(GitError::Other { stderr })
                if stderr.contains("cannot lock ref")
                    || stderr.contains("already exists")
                    || stderr.contains("but expected") =>
            {
                Err(GitError::RefChanged(name.to_string()))
            }
            Err(e) => Err(e),
        }
    }

    fn status(&self) -> Result<RepoStatus> {
        let head = self.rev_parse("HEAD")?;
        let branch = self
            .local(&["symbolic-ref", "--quiet", "--short", "HEAD"])
            .ok()
            .map(|o| o.text())
            .filter(|s| !s.is_empty());
        let porcelain = self.local(&["status", "--porcelain=v1", "-z", "-uall", "--no-renames"])?;
        let mut dirty = Vec::new();
        for entry in split_nul(&porcelain.stdout) {
            if entry.len() < 4 {
                continue;
            }
            let (xy, path) = entry.split_at(2);
            let path = path.trim_start().to_string();
            if xy.contains('U') || xy == "AA" || xy == "DD" {
                continue; // reported through `unmerged`
            }
            let kind = if xy == "??" {
                DirtyKind::Untracked
            } else if xy.contains('D') {
                DirtyKind::Deleted
            } else {
                DirtyKind::Modified
            };
            dirty.push(DirtyPath { path, kind });
        }
        dirty.sort_by(|a, b| a.path.cmp(&b.path));
        let unmerged_raw = self.local(&["ls-files", "-u", "-z"])?;
        let mut unmerged: Vec<UnmergedEntry> = Vec::new();
        for entry in split_nul(&unmerged_raw.stdout) {
            // "<mode> <oid> <stage>\t<path>"
            let Some((meta, path)) = entry.split_once('\t') else {
                continue;
            };
            let mut parts = meta.split_whitespace();
            let _mode = parts.next();
            let oid = Oid::from_hex(parts.next().unwrap_or_default())?;
            let stage = parts.next().unwrap_or("0");
            if unmerged.last().is_none_or(|u| u.path != path) {
                unmerged.push(UnmergedEntry {
                    path: path.to_string(),
                    base: None,
                    ours: None,
                    theirs: None,
                });
            }
            if let Some(last) = unmerged.last_mut() {
                match stage {
                    "1" => last.base = Some(oid),
                    "2" => last.ours = Some(oid),
                    "3" => last.theirs = Some(oid),
                    _ => {}
                }
            }
        }
        Ok(RepoStatus {
            head,
            branch,
            dirty,
            unmerged,
        })
    }

    fn resolve_ref(&self, name: &str) -> Result<Option<Oid>> {
        self.rev_parse(name)
    }

    fn commit_info(&self, commit: &Oid) -> Result<CommitInfo> {
        let out = self.local(&[
            "log",
            "-1",
            "--format=%H%x00%T%x00%P%x00%ct%x00%B",
            commit.as_hex(),
        ])?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut parts = text.splitn(5, '\0');
        let id = Oid::from_hex(parts.next().unwrap_or_default())?;
        let tree = Oid::from_hex(parts.next().unwrap_or_default())?;
        let parents = parts
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .map(Oid::from_hex)
            .collect::<Result<Vec<_>>>()?;
        let committer_time = parts.next().unwrap_or_default().trim().parse().unwrap_or(0);
        let message = parts.next().unwrap_or_default().trim_end().to_string() + "\n";
        Ok(CommitInfo {
            id,
            tree,
            parents,
            committer_time,
            message,
        })
    }

    fn reset_index(&self, commit: &Oid) -> Result<()> {
        self.local(&["read-tree", commit.as_hex()])?;
        // Refresh stat data so the next `status` does not rehash every file.
        let _ = self.local(&["update-index", "-q", "--refresh"]);
        Ok(())
    }

    fn kind(&self) -> ActiveBackend {
        ActiveBackend::Hybrid
    }
}
