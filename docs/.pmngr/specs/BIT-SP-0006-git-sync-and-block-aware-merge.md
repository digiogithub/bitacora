---
id: BIT-SP-0006
type: spec
title: Git sync and block-aware merge
status: backlog
author: mcp
labels: [git, sync, merge]
created: 2026-10-06T14:21:35Z
updated: 2026-10-06T19:34:02Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/autocommit.rs
        - crates/bitacora-sync/src/engine.rs
      tests: [crates/bitacora-sync/tests/auto_commit.rs]
    verified: {rev: "sha256:85ca269e0992b4ec", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/commit_msg.rs
        - crates/bitacora-sync/src/autocommit.rs
      tests:
        - crates/bitacora-sync/src/commit_msg.rs
        - crates/bitacora-sync/tests/auto_commit.rs
    verified: {rev: "sha256:56b197b1fdc6ec3b", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/repo_setup.rs
        - crates/bitacora-sync/src/onboarding.rs
      tests:
        - crates/bitacora-sync/src/repo_setup.rs
        - crates/bitacora-sync/tests/onboarding.rs
    verified: {rev: "sha256:5fb1017457cf27ae", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R4:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/engine.rs
        - crates/bitacora-sync/src/merge.rs
        - crates/bitacora-sync/src/state.rs
      tests: [crates/bitacora-sync/tests/sync_engine.rs]
    verified: {rev: "sha256:8bc8f706bf0e1632", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/engine.rs
        - crates/bitacora-sync/src/state.rs
      tests: [crates/bitacora-sync/tests/sync_engine.rs]
    verified: {rev: "sha256:fe08756c4c01b38c", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R6:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/backend/mod.rs
        - crates/bitacora-sync/src/backend/cli.rs
        - crates/bitacora-sync/src/backend/detect.rs
        - crates/bitacora-sync/src/backend/gix_net.rs
        - crates/bitacora-sync/src/backend/gix_read.rs
        - crates/bitacora-sync/src/backend/gix_trees.rs
        - crates/bitacora-sync/src/backend/git2_push.rs
        - crates/bitacora-sync/src/credentials.rs
        - crates/bitacora-sync/src/askpass.rs
        - crates/bitacora-sync/src/bin/askpass.rs
      tests:
        - crates/bitacora-sync/tests/backends.rs
        - crates/bitacora-sync/src/backend/detect.rs
        - crates/bitacora-sync/tests/git2_push.rs
        - crates/bitacora-sync/tests/askpass.rs
    verified: {rev: "sha256:a2d25d6021befccb", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/writer.rs
        - crates/bitacora-sync/src/engine.rs
        - crates/bitacora-runtime/src/writer.rs
      tests:
        - crates/bitacora-sync/src/writer.rs
        - crates/bitacora-sync/tests/sync_engine.rs
        - crates/bitacora-sync/tests/auto_commit.rs
        - crates/bitacora-runtime/tests/sync.rs
        - crates/bitacora-runtime/tests/session.rs
  R8:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/merge/markers.rs
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/src/engine.rs
        - crates/bitacora-sync/src/resolve.rs
        - crates/bitacora-merge/src/page.rs
      tests:
        - crates/bitacora-sync/src/merge/markers.rs
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
        - crates/bitacora-sync/tests/e2e_conflict.rs
        - crates/bitacora-merge/tests/corpus.rs
        - crates/bitacora-merge/tests/golden.rs
  R9:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/model.rs
        - crates/bitacora-merge/src/matcher.rs
      tests:
        - crates/bitacora-merge/src/matcher.rs
        - crates/bitacora-merge/src/model.rs
        - crates/bitacora-merge/tests/matcher_accuracy.rs
        - crates/bitacora-merge/tests/corpus.rs
        - crates/bitacora-merge/tests/golden.rs
        - crates/bitacora-merge/tests/merge_matrix.rs
    verified: {rev: "sha256:bc9e9633311450b3", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/fields.rs
        - crates/bitacora-merge/src/block.rs
      tests:
        - crates/bitacora-merge/src/fields.rs
        - crates/bitacora-merge/src/block.rs
        - crates/bitacora-merge/tests/golden.rs
        - crates/bitacora-merge/tests/corpus.rs
        - crates/bitacora-merge/tests/merge_matrix.rs
    verified: {rev: "sha256:7c7fbc9f08fb5fe9", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/meta.rs
        - crates/bitacora-markdown/src/classify.rs
        - crates/bitacora-merge/src/page.rs
      tests:
        - crates/bitacora-merge/src/meta.rs
        - crates/bitacora-merge/src/block.rs
        - crates/bitacora-markdown/src/classify.rs
        - crates/bitacora-merge/tests/merge_matrix.rs
        - crates/bitacora-merge/tests/golden.rs
        - crates/bitacora-merge/tests/corpus.rs
    verified: {rev: "sha256:50feaf0eceab2334", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R12:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/structure.rs
        - crates/bitacora-merge/src/page.rs
        - crates/bitacora-merge/src/matcher.rs
      tests:
        - crates/bitacora-merge/tests/merge_matrix.rs
        - crates/bitacora-merge/src/structure.rs
        - crates/bitacora-merge/tests/golden.rs
        - crates/bitacora-merge/tests/corpus.rs
    verified: {rev: "sha256:d99d7600943a5665", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R13:
    status: backlog
    trace:
      code: [crates/bitacora-merge/src/marker.rs]
      tests:
        - crates/bitacora-merge/src/marker.rs
        - crates/bitacora-merge/src/block.rs
    verified: {rev: "sha256:6ba018bc1529a33a", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/page.rs
        - crates/bitacora-merge/src/resolve.rs
        - crates/bitacora-sync/src/merge/mod.rs
      tests:
        - crates/bitacora-merge/tests/merge_matrix.rs
        - crates/bitacora-merge/src/resolve.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
        - crates/bitacora-merge/tests/corpus.rs
        - crates/bitacora-merge/tests/golden.rs
  R15:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/store.rs
        - crates/bitacora-sync/src/state.rs
        - crates/bitacora-sync/src/engine.rs
        - crates/bitacora-sync/src/resolve.rs
      tests:
        - crates/bitacora-sync/src/store.rs
        - crates/bitacora-sync/src/resolve.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
        - crates/bitacora-sync/tests/sync_engine.rs
  R16:
    status: backlog
  R17:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/merge/policy.rs
        - crates/bitacora-sync/src/merge/edn.rs
        - crates/bitacora-sync/src/merge/files.rs
        - crates/bitacora-sync/src/merge/mod.rs
      tests:
        - crates/bitacora-sync/src/merge/policy.rs
        - crates/bitacora-sync/src/merge/edn.rs
        - crates/bitacora-sync/src/merge/files.rs
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
        - crates/bitacora-sync/tests/sync_engine.rs
  R18:
    status: backlog
    trace:
      code:
        - crates/bitacora-merge/src/page.rs
        - crates/bitacora-merge/src/meta.rs
      tests:
        - crates/bitacora-merge/src/page.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
        - crates/bitacora-sync/tests/sync_engine.rs
  R19:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/src/merge/renames.rs
        - crates/bitacora-sync/src/resolve.rs
      tests:
        - crates/bitacora-sync/src/merge/renames.rs
        - crates/bitacora-sync/src/resolve.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
  R20:
    status: backlog
    trace:
      code: [crates/bitacora-sync/src/repo_setup.rs]
      tests: [crates/bitacora-sync/src/repo_setup.rs]
    verified: {rev: "sha256:0822359e18053b7c", commit: 25c13e48dc591ad5061edb5019d652abb345b4e4, at: 2026-10-06T18:25:15Z, by: claude}
  R21:
    status: backlog
    trace:
      code:
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/src/store.rs
        - crates/bitacora-sync/src/engine.rs
      tests:
        - crates/bitacora-sync/src/merge/mod.rs
        - crates/bitacora-sync/tests/merge_matrix.rs
  R22:
    status: backlog
---

## Purpose
The graph stays in sync with a git remote automatically; metadata conflicts resolve themselves and content conflicts are resolved visually per block, never as conflict markers in files.

## Scope
Git backend, sync loop, auth, block-aware 3-way merge, auto-resolution rules, conflict resolver UI, special files. Source: [[git-sync-merge]], [[05-git-and-apis]]. Implemented by BIT-EP-0011 and BIT-EP-0012.

## Requirements

### BIT-SP-0006.R1 — Auto-commit after idle debounce with a hard cap

The sync engine SHALL commit local changes after an idle debounce (`sync.commit_idle_secs`, default 20 s, min 5 s, max 10 min) measured from the last flushed write, with a hard cap (`sync.commit_max_secs`, default 300 s) of continuous editing; it SHALL NOT commit on a blind fixed interval. It SHALL also commit on graph open (after recovery), on graph/app close (10 s budget, then best-effort push) and on manual "Sync now". Commits SHALL be skipped when only ignored or volatile files changed.

#### Scenario: Idle commit
- GIVEN the user edits `pages/Ideas.md` at t=0 s and stops typing
- WHEN 20 s elapse without further flushed writes
- THEN exactly one commit is created containing `pages/Ideas.md`

#### Scenario: Continuous editing cap
- GIVEN the user flushes a write every 10 s for 6 minutes
- WHEN t reaches 300 s
- THEN a commit is created even though the idle debounce never expired

#### Scenario: Only ignored files changed
- GIVEN only `logseq/bak/pages/Ideas/2026-10-06.md` changed
- WHEN the debounce fires
- THEN no commit is created

### BIT-SP-0006.R2 — Machine-parseable commit messages with kinds; squash unpushed auto commits

Commit messages SHALL have a first line ≤ 72 chars and trailers `Bitacora-Device`, `Bitacora-Kind` (`auto|manual|merge|resolve|agent|migrate`) and `Bitacora-Pages`; agent commits SHALL add `Bitacora-Agent: <client>`. The engine SHOULD amend the previous commit instead of creating a new one when it is `Kind: auto`, from this device, unpushed and younger than 30 min (`sync.squash_auto_commits`); it SHALL never amend a pushed commit.

#### Scenario: Auto commit message
- WHEN the engine commits edits to 3 pages on device "laptop-jose"
- THEN the message is:
  - `bitacora: edit 3 pages (journals/2026_10_06, Project X, Ideas)`
  - blank line
  - `Bitacora-Device: laptop-jose`
  - `Bitacora-Kind: auto`
  - `Bitacora-Pages: journals/2026_10_06.md; pages/Project X.md; pages/Ideas.md`

#### Scenario: Squash within session
- GIVEN HEAD is an unpushed `Kind: auto` commit from this device made 10 min ago
- WHEN the next auto commit happens
- THEN HEAD is amended (same parent) and its `Bitacora-Pages` is the union of both page sets

#### Scenario: Never amend pushed commits
- GIVEN HEAD equals `origin/main`
- WHEN the next auto commit happens
- THEN a new commit with parent HEAD is created

### BIT-SP-0006.R3 — Repo-local identity and standard in-folder .git

The system SHALL keep the repository as a standard `.git` directory inside the graph folder (never a separate gitdir such as Logseq's `~/.logseq/git/...`), SHALL write `user.name`/`user.email` only to the repo-local config from Bitacora settings, falling back to `Bitacora <device@hostname>`, and SHALL NOT write global or system git config.

#### Scenario: Missing identity
- GIVEN no `user.email` is configured anywhere
- WHEN the first commit is made on host "laptop"
- THEN `.git/config` gets `user.name = Bitacora` and `user.email = device@laptop` and `~/.gitconfig` is unchanged

#### Scenario: Init a new graph repo
- GIVEN a graph folder without `.git`
- WHEN the user enables sync with a remote URL
- THEN `<graph>/.git` is a directory (not a gitdir pointer file)

### BIT-SP-0006.R4 — Fetch, integrate and push with bounded retry

The engine SHALL fetch `origin/<branch>` periodically (default 120 s foreground, 600 s background), on network-up/wake and on demand. It SHALL fast-forward when HEAD is an ancestor of the remote, push when the remote is an ancestor of HEAD, and on divergence run the block-aware merge and create a two-parent merge commit. It SHALL use plain push only (no force, no `--force-with-lease`); a non-fast-forward rejection SHALL loop back to fetch up to 5 attempts with 1–8 s jitter, then enter `Error(PushRejectedLoop)`.

#### Scenario: Fast-forward
- GIVEN local `main` = A and `origin/main` = A→B
- WHEN sync runs
- THEN HEAD becomes B, changed files are updated in the work tree, the index is refreshed, and no commit is created

#### Scenario: Divergence auto-resolved
- GIVEN local A→C and remote A→B touching different blocks of `pages/P.md`
- WHEN sync runs
- THEN a merge commit M with parents [C, B] and `Bitacora-Kind: merge` is created and pushed

#### Scenario: Push race
- GIVEN another device pushes between our fetch and our push 5 times in a row
- WHEN sync retries
- THEN after the 5th rejection the state is `Error(PushRejectedLoop)` and no force push was attempted

### BIT-SP-0006.R5 — Work fully offline with back-off

Network failures during fetch/push SHALL set state `Offline` without losing local work; local commits SHALL continue. Retries SHALL back off 30 s, 1 m, 2 m, 5 m, 10 m (cap) and reset on OS network-change events. The status bar SHALL show "N local commits not synced".

#### Scenario: Offline editing
- GIVEN the remote host is unreachable
- WHEN the user edits for 30 minutes
- THEN auto commits are created locally, state is `Offline`, and the status shows "3 local commits not synced"

#### Scenario: Network returns
- GIVEN state `Offline` with next retry in 10 min
- WHEN the OS reports network up
- THEN a sync starts immediately and the back-off resets to 30 s

### BIT-SP-0006.R6 — System git CLI for network and ref writes when installed, gix otherwise, behind GitBackend

Per ADR-007 as amended by ADR-020, git SHALL NOT be bundled. At startup the system SHALL detect a system `git` (setting `sync.git_binary`, else `PATH`) and check its version (≥ 2.38). When a suitable git is found, the system SHALL run `fetch`, `push`, `commit`, `commit-tree`, `update-ref`, `clone` and `ls-remote` through the git CLI with `GIT_TERMINAL_PROMPT=0`, `GIT_ASKPASS=<bitacora askpass helper>`, `LC_ALL=C`, `-c core.quotepath=false`, `-c core.autocrlf=false`, so SSH agents, `~/.ssh/config`, credential helpers and Git Credential Manager work on Linux, macOS and Windows. When no suitable git is found, the system SHALL use a pure-Rust `gix` backend for all operations, including fetch, push, clone, commit and ref updates, with HTTPS credentials from the OS keyring / in-app prompt and SSH via the gix transport. It SHOULD use `gix` for status, blob/tree reads, merge-base, tree diffs with rename detection and writing merged trees in both modes. All backends SHALL sit behind a `GitBackend` trait (git2 only as optional fallback implementation). The UI SHALL show which backend is active and SHALL suggest installing git when authentication fails on the gix fallback.

#### Scenario: SSH host alias
- GIVEN system git is installed, remote `git@work:me/notes.git` and `~/.ssh/config` defines `Host work` with an `IdentityFile`
- WHEN sync fetches
- THEN the fetch succeeds using the CLI's ssh configuration

#### Scenario: Missing credentials
- GIVEN system git is installed, an HTTPS remote and no credential helper
- WHEN push needs a password
- THEN git invokes the Bitacora askpass helper, the app shows a credential prompt, and git never blocks on a terminal prompt

#### Scenario: Old git
- GIVEN the system git is 2.30
- WHEN sync is enabled
- THEN the gix-only backend is selected and the status shows "gix (system git 2.30 < 2.38 ignored)"

#### Scenario: No git installed
- GIVEN no `git` on `PATH` and an HTTPS remote whose token is stored in the OS keyring
- WHEN sync runs
- THEN fetch, merge commit and push complete through the gix backend and the status indicator shows the gix backend as active

#### Scenario: Auth fails on fallback
- GIVEN the gix backend is active and SSH auth fails
- WHEN the error is shown
- THEN the message suggests installing git to use the system SSH configuration

### BIT-SP-0006.R7 — Serialize sync with the graph write lock and flush editors first

The engine SHALL hold the graph write lock (the same lock used by the core writer and MCP writes) while staging and while rewriting files during a merge, SHALL request `flush_all` from open editors and wait before merging, and SHALL NOT start a sync while a write transaction is pending. Merge output SHALL be applied through the core writer so the index updates and editors reload.

#### Scenario: Pending write transaction
- GIVEN an MCP `insert_block` transaction is in progress
- WHEN the periodic sync tick fires
- THEN the sync waits until the transaction commits, then proceeds

#### Scenario: Unsaved typing
- GIVEN the user has typed "abc" in block u1 not yet written
- WHEN a merge starts
- THEN "abc" is flushed and included in the "ours" side of the merge

### BIT-SP-0006.R8 — Never write conflict markers; detect external ones

The system SHALL NEVER write lines starting with `<<<<<<< `, `=======` or `>>>>>>> ` into graph files as a result of a merge. It SHALL maintain `.git/info/attributes` with `*.md merge=binary`, `logseq/config.edn merge=binary` and `* -text`. When it finds unmerged index entries (e.g. from a user's `git pull`) or well-formed marker regions in tracked files, it SHALL split them into base/ours/theirs, run its own merge, write a clean result and register remaining conflicts in the resolver; malformed markers SHALL produce a `external_markers` conflict and marker lines SHALL never be indexed as block content.

#### Scenario: External marker region
- GIVEN `pages/P.md` contains:
  - `- shared`
  - `<<<<<<< HEAD`
  - `- mine`
  - `=======`
  - `- theirs`
  - `>>>>>>> origin/main`
- WHEN Bitacora opens the graph
- THEN the file is rewritten as `- shared` / `- mine` / `- theirs` (both inserts kept, ours first) and no marker line remains

#### Scenario: User ran git pull
- GIVEN the user ran `git pull` producing an unmerged `pages/P.md` (binary driver kept ours)
- WHEN the engine ticks
- THEN it reads stages 1/2/3 from the index, runs the block merge and writes a clean file

#### Scenario: Attributes present
- WHEN sync is enabled on a repo
- THEN `.git/info/attributes` contains `*.md merge=binary`

### BIT-SP-0006.R9 — Match blocks across base, ours and theirs

The merge SHALL match blocks by equal `id::` first; then, for blocks without `id::`, by exact normalized content under the same parent, then LCS of children content hashes per parent, then fuzzy similarity ≥ 0.6 (token Jaccard / normalized Levenshtein on the first line) one-to-one best-first; otherwise blocks are distinct. Normalization for comparison SHALL trim trailing whitespace, unify CRLF, unify indentation unit and compare properties as sets. A duplicate `id::` within one file SHALL get a fresh uuid for the second occurrence.

#### Scenario: Match by id
- GIVEN base `- Plan\n  id:: 66aa…`, theirs `- Plan for Q4\n  id:: 66aa…`
- WHEN matching runs
- THEN both are the same block with a content change on theirs

#### Scenario: Fuzzy match without id
- GIVEN base child `- Buy milk and eggs`, ours `- Buy milk and eggs today`
- WHEN matching runs
- THEN they match (similarity ≥ 0.6) and ours is a content edit, not delete + insert

#### Scenario: Added id is metadata
- GIVEN base `- Idea A`, ours `- Idea A\n  id:: 66bb…`, theirs unchanged
- WHEN matching runs
- THEN they match and the change is classified as an `id::` addition

### BIT-SP-0006.R10 — Three-way content merge with in-block line diff3

For each matched block the content SHALL merge 3-way: if ours == base take theirs; if theirs == base take ours; if equal take either; else apply line-level diff3 inside the block, and only overlapping line edits SHALL produce `CONFLICT(content)`. User properties (`key:: value`) SHALL merge per key; the same key changed differently on both sides SHALL produce `CONFLICT(property)`.

#### Scenario: Different lines of a multi-line block
- GIVEN base `- Notes\n  line a`, ours `- Notes\n  line a\n  line b`, theirs `- Notes (draft)\n  line a`
- WHEN merged
- THEN output is `- Notes (draft)\n  line a\n  line b` with no conflict

#### Scenario: Same line edited both sides
- GIVEN base `- Meet at 10`, ours `- Meet at 11`, theirs `- Meet at 12`
- WHEN merged
- THEN a `content` conflict is recorded and the work tree holds `- Meet at 11`

#### Scenario: Property per key
- GIVEN base `status:: open`, ours adds `owner:: ana`, theirs changes `status:: done`
- WHEN merged
- THEN the block has `status:: done` and `owner:: ana`, no conflict

### BIT-SP-0006.R11 — Auto-resolve metadata-only differences deterministically

Per ADR-009 and the content-vs-metadata classification in [[02-markdown-block-syntax]] §5.4, the merge SHALL never raise a conflict for metadata and SHALL apply: `collapsed::` prefer ours, else theirs if ours unchanged; `id::` union, and if both added different ids keep the one referenced elsewhere (else lexicographically smaller) and rewrite `((loser))` refs; `:LOGBOOK:` CLOCK lines set-union sorted by start, keeping the latest open clock; `card-*` group taken whole from the side with later `card-last-reviewed`; other metadata keys (`query-*`, `filters`, `hl-*`, marker timestamps) last-writer-wins per key by commit time; property order = ours order plus new theirs keys appended; whitespace/indent style ignored for equality, output in ours' style.

#### Scenario: collapsed vs content
- GIVEN base `- Topic`, ours `- Topic\n  collapsed:: true`, theirs `- Topic (v2)`
- WHEN merged
- THEN output `- Topic (v2)\n  collapsed:: true`, no conflict

#### Scenario: LOGBOOK union
- GIVEN ours and theirs each added a different `CLOCK: [2026-10-05 Mon 09:00]--[…] =>  01:00:00` line to the same block's `:LOGBOOK:`
- WHEN merged
- THEN the drawer contains both CLOCK lines sorted by start time

#### Scenario: card-* group
- GIVEN ours `card-last-reviewed:: 2026-10-01…`, `card-repeats:: 3` and theirs `card-last-reviewed:: 2026-10-04…`, `card-repeats:: 4`
- WHEN merged
- THEN all `card-*` values come from theirs; no field mixing

#### Scenario: Both added different ids
- GIVEN ours added `id:: aaaa…` and theirs added `id:: bbbb…` to the same block, and `((bbbb…))` is referenced on another page
- WHEN merged
- THEN output keeps `id:: bbbb…` and any `((aaaa…))` in the merge result is rewritten to `((bbbb…))`

### BIT-SP-0006.R12 — Merge structure: inserts, deletes, moves and reorders

The merge SHALL insert one-side additions after the match of their left sibling (or at parent end); keep both-side inserts at the same place (ours first, dedupe identical normalized content); delete blocks deleted on one side and unchanged on the other; raise `CONFLICT(delete_vs_modify)` when one side deletes and the other modifies (including a modified child); re-attach a child added under a parent deleted on the other side to the nearest surviving ancestor with an info note; merge sibling order 3-way and, when both moved a block to different places, take ours with an info note — moves and reorders SHALL never be conflicts.

#### Scenario: Both inserted at the same place
- GIVEN base `- A\n- C`, ours `- A\n- B1\n- C`, theirs `- A\n- B2\n- C`
- WHEN merged
- THEN output `- A\n- B1\n- B2\n- C`

#### Scenario: Delete vs modify
- GIVEN base `- Draft`, ours deletes it, theirs `- Draft v2`
- WHEN merged
- THEN `CONFLICT(delete_vs_modify)` is recorded; work tree keeps the modified block until resolved

#### Scenario: Reorder vs edit
- GIVEN base `- A\n- B`, ours `- B\n- A`, theirs `- A!\n- B`
- WHEN merged
- THEN output `- B\n- A!` with no conflict

### BIT-SP-0006.R13 — Task marker and SCHEDULED/DEADLINE merge rules

When one side changed only the task marker and the other only the text, the merge SHALL combine both. When both changed the marker and the text is otherwise identical, it SHALL take the more advanced state (`DONE` > `CANCELED` > `DOING`/`NOW` > `TODO`/`LATER`); otherwise it is a content conflict. `SCHEDULED:` / `DEADLINE:` lines SHALL merge per line 3-way, and both changing the same line SHALL be `CONFLICT(property)`.

#### Scenario: Marker vs text
- GIVEN base `- TODO write report`, ours `- DONE write report`, theirs `- TODO write final report`
- WHEN merged
- THEN output `- DONE write final report`

#### Scenario: Both changed marker
- GIVEN base `- TODO call`, ours `- DOING call`, theirs `- DONE call`
- WHEN merged
- THEN output `- DONE call`, no conflict

#### Scenario: Both rescheduled
- GIVEN base `SCHEDULED: <2026-10-06 Tue>`, ours `<2026-10-07 Wed>`, theirs `<2026-10-08 Thu>`
- WHEN merged
- THEN `CONFLICT(property)` on the SCHEDULED line

### BIT-SP-0006.R14 — Byte-preserving merge output and no post-pull write loop

Merge serialization SHALL reuse the original raw bytes (from ours) of every block whose merged value equals ours, so a merge never reformats a file, and SHALL keep ours' line endings, BOM and indentation style; the output SHALL round-trip through the parser. Side-effect writes caused by integrating remote changes (e.g. adding `id::` to newly referenced blocks, ref rewrites after renames) SHALL be included in the merge commit, not create a new dirty→commit loop.

#### Scenario: Untouched blocks unchanged
- GIVEN ours uses tab indentation and CRLF and theirs changed only block 7 of 40
- WHEN merged
- THEN `diff ours output` shows changes only within block 7's lines, with CRLF preserved

#### Scenario: id added after pull
- GIVEN theirs adds `((u5))` referencing a block u5 in our page that lacks `id::`
- WHEN the merge completes
- THEN `id:: u5` is written in the same merge commit and no extra auto commit follows

### BIT-SP-0006.R15 — Persist conflicted state; keep ours; push nothing until resolved

When content conflicts remain the engine SHALL enter `Conflicted`, write the work tree with ours for conflicting regions and all auto-resolved remote changes elsewhere, store the pending merge in `refs/bitacora/pending-merge` plus `.git/bitacora/merge-state.json` (base, ours, theirs commits, conflict records with kind, path, block key, breadcrumb, base/ours/theirs text, resolution), restore it on restart, keep committing local edits, push nothing until all conflicts are resolved, and then create a `Kind: resolve` merge commit and push.

#### Scenario: Restart while conflicted
- GIVEN 2 unresolved conflicts and the app is quit
- WHEN the app starts again
- THEN state is `Conflicted` with the same 2 conflicts and any resolutions already chosen

#### Scenario: Edits elsewhere while conflicted
- GIVEN state `Conflicted`
- WHEN the user edits a non-conflicting page
- THEN a local auto commit is created and nothing is pushed

#### Scenario: Last resolution
- WHEN the user resolves the last conflict
- THEN files are written, a merge commit with parents [local HEAD, remote] and `Bitacora-Kind: resolve` is created and pushed, and `merge-state.json` is removed

### BIT-SP-0006.R16 — Per-block visual conflict resolver

The UI SHALL show a "Conflicts (N)" banner and a resolver with one card per conflict grouped by page, showing breadcrumb, side-by-side rendered ours | theirs with word-level diff against base, author/time of theirs, and actions Keep mine / Keep theirs / Keep both (theirs as next sibling with a regenerated `id::`) / Edit (inline editor seeded with both versions, no markers); delete-vs-modify offers Keep (modified) / Delete; bulk "resolve all on this page with mine/theirs" and keyboard shortcuts SHALL exist. Editing a conflicted block in the normal editor SHALL count as an Edit resolution.

#### Scenario: Keep both
- GIVEN conflict on block u1 with ours `- Meet at 11` and theirs `- Meet at 12`
- WHEN the user clicks Keep both
- THEN the page has `- Meet at 11` followed by sibling `- Meet at 12` with a new `id::` only if the original had one

#### Scenario: Bulk resolve
- GIVEN page P has 4 conflicts
- WHEN the user picks "Resolve all on this page with theirs"
- THEN all 4 conflicts on P are resolved with theirs and the banner count drops by 4

### BIT-SP-0006.R17 — Merge policies for non-page files

The merge SHALL dispatch by path: `pages/**/*.md`, `journals/*.md` → block merge; `logseq/config.edn` → EDN-aware 3-way map merge preserving comments, with same-key conflicts as `CONFLICT(config)`; `logseq/custom.css`, `custom.js`, `.gitignore` and other text → line diff3; whiteboards/excalidraw/tldr → whole-file, if both changed keep ours and save theirs as `<name> (conflict <device> <date>).<ext>`; `assets/**` binary → identical no-op, one-side change taken, both changed → ours at path and theirs as `name (conflict-<short-sha>).ext`; `logseq/bak/**`, `.recycle/**`, `version-files/**`, `graphs-txid.edn`, `pages-metadata.edn` → ignored (take ours if tracked). Non-UTF-8 `.md` SHALL use the binary policy.

#### Scenario: config.edn different keys
- GIVEN ours changes `:journal/page-title-format` and theirs adds `:default-templates {:journals "daily"}`
- WHEN merged
- THEN both changes appear and comments in ours are preserved

#### Scenario: Asset collision
- GIVEN both sides changed `assets/diagram_1696000000.png`
- WHEN merged
- THEN ours stays at the path, theirs is saved as `assets/diagram_1696000000 (conflict-c3d4e5f).png`, and a notification is shown

### BIT-SP-0006.R18 — Add/add journals and lazy journal creation

For add/add of any page (notably `journals/<date>.md`) the merge SHALL use an empty base and union the blocks (ours first, dedupe identical); a side whose content is only the default journal template, `-`, `*` or empty SHOULD be treated as unchanged and the other side taken. Bitacora SHOULD NOT create today's journal file on disk until the user types into it.

#### Scenario: Both devices wrote today's journal
- GIVEN no base, ours `- call Ana`, theirs `- buy milk`
- WHEN merged
- THEN output `- call Ana\n- buy milk`

#### Scenario: Empty template side
- GIVEN ours `-` (empty journal) and theirs `- buy milk`
- WHEN merged
- THEN output is theirs byte-for-byte

#### Scenario: Opening today
- WHEN the user opens today's journal and types nothing
- THEN no `journals/2026_10_06.md` file exists on disk

### BIT-SP-0006.R19 — Handle renames and file-level delete/modify

The merge SHALL detect renames (gix rename tracking, similarity ≥ 50%, identical `id::` sets as a strong signal); renamed on one side and modified on the other SHALL apply the modifications at the new path; different renames on both sides SHALL raise `CONFLICT(rename_rename)` with the other title suggested as `alias::`; links `[[Old Name]]` added by the other side after a rename SHALL be rewritten in a post-merge fix-up pass (logged as info). File deleted on one side and modified on the other SHALL raise `CONFLICT(file_delete_vs_modify)` defaulting to "restore". Case-only renames SHALL go via a temp name.

#### Scenario: Rename vs modify
- GIVEN ours renamed `pages/Old.md` → `pages/New.md` (with `title:: New`) and theirs edited a block in `pages/Old.md`
- WHEN merged
- THEN `pages/New.md` contains theirs' edit and `pages/Old.md` does not exist

#### Scenario: New link to old name
- GIVEN ours renamed "Old" → "New" and theirs added `- see [[Old]]` in `pages/X.md`
- WHEN merged
- THEN `pages/X.md` contains `- see [[New]]` and the merge commit notes the rewrite

#### Scenario: Rename/rename
- GIVEN ours renamed "Old" → "A" and theirs "Old" → "B"
- WHEN merged
- THEN a `rename_rename` conflict offers "A" (with `alias:: B`) or "B" (with `alias:: A`)

### BIT-SP-0006.R20 — Write and maintain a default .gitignore

The system SHALL write a default `.gitignore` when absent, and append missing entries when present, covering `.DS_Store`, `Thumbs.db`, `logseq/bak/`, `logseq/.recycle/`, `logseq/version-files/`, `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn`, `logseq/.bitacora/`, `.trash/`, `*~`, without reordering or removing user lines.

#### Scenario: Existing gitignore
- GIVEN `.gitignore` contains `secret.md` and `logseq/bak/`
- WHEN sync is enabled
- THEN the file keeps `secret.md` and `logseq/bak/` in place and gains the missing entries under a `# Bitacora / Logseq volatile files` header

#### Scenario: Idempotent
- WHEN sync is enabled a second time
- THEN `.gitignore` is unchanged

### BIT-SP-0006.R21 — Remember block-level resolutions when the remote moves

While `Conflicted`, periodic fetch SHALL continue; if the remote moves, the merge SHOULD be recomputed against the new remote with the same base and previously chosen resolutions re-applied by block identity (block-level rerere memo keyed by path, block key and the base/ours/theirs content hashes).

#### Scenario: Remote moves during resolution
- GIVEN conflicts c1 (block u1) and c2 (block u2); the user resolved c1 with "theirs"
- WHEN the remote gains a commit touching an unrelated page and the engine refetches
- THEN the recomputed merge shows only c2 as unresolved and c1's resolution is applied

#### Scenario: Conflicting block changed again
- GIVEN c1 resolved, then theirs edits u1 again
- WHEN recomputed
- THEN c1 reappears unresolved because its theirs hash changed

### BIT-SP-0006.R22 — Per-page history with block-level diff and selective restore

The app SHOULD show a per-page history from git (commits touching the page path, following renames) with device, kind and time, a block-level diff between any version and the current page, and selective restore of individual blocks (as a normal undoable op) or of the whole page.

#### Scenario: Restore one block
- GIVEN block u1 on "Project X" was deleted 3 commits ago
- WHEN the user opens Page history, selects that commit and clicks "Restore block" on u1
- THEN u1 is reinserted at its former position (or parent end) as one undo transaction

#### Scenario: Diff view
- WHEN the user selects a version
- THEN added/removed/changed blocks are highlighted, and metadata-only changes (e.g. `collapsed::`) are hidden by default
