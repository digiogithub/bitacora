---
id: BIT-SP-0005
type: spec
title: Write safety and external change handling
status: backlog
author: mcp
labels: [core, io]
created: 2026-10-06T14:21:35Z
updated: 2026-10-07T00:17:04Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/queue.rs#CommandQueue
        - crates/bitacora-core/src/editor/flush.rs#FileStore
      tests:
        - crates/bitacora-core/tests/command_queue.rs
        - crates/bitacora-core/tests/single_writer_guard.rs
    verified: {rev: "sha256:eee4f5ef9ee8b32c", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/write_queue.rs#WriteQueue
        - crates/bitacora-core/src/queue.rs#QueueConfig
      tests:
        - crates/bitacora-core/src/write_queue.rs
        - crates/bitacora-core/tests/write_pipeline.rs
    verified: {rev: "sha256:7a982699299bfa0d", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/model.rs#Page
        - crates/bitacora-core/src/editor/flush.rs#Workspace
      tests: [crates/bitacora-core/tests/write_pipeline.rs]
    verified: {rev: "sha256:47a1cca3effadacb", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R4:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/flush.rs#Workspace]
      tests: [crates/bitacora-core/tests/write_pipeline.rs]
    verified: {rev: "sha256:a5c79161c10017e5", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R5:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/fsio.rs#atomic_write]
      tests:
        - crates/bitacora-core/tests/crash_safety.rs
        - crates/bitacora-core/tests/write_pipeline.rs
    verified: {rev: "sha256:134d734aeee6a844", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R6:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/model.rs#Page]
      tests: [crates/bitacora-core/tests/write_pipeline.rs]
    verified: {rev: "sha256:958de85ea9592a04", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R7:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/model.rs#Page
        - crates/bitacora-core/src/editor/flush.rs#Workspace
      tests: [crates/bitacora-core/tests/write_pipeline.rs]
    verified: {rev: "sha256:942a0e5f2ade0020", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R8:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/flush.rs#Workspace
        - crates/bitacora-core/src/queue.rs#QueueEvent
      tests: [crates/bitacora-core/tests/write_pipeline.rs]
    verified: {rev: "sha256:9d9b3e7144412ddf", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R9:
    status: backlog
    trace:
      code: [crates/bitacora-core/src/editor/backup.rs]
      tests:
        - crates/bitacora-core/src/editor/backup.rs
        - crates/bitacora-core/tests/write_pipeline.rs
    verified: {rev: "sha256:4e58bed493ec6c58", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R10:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/flush.rs#Workspace
        - crates/bitacora-core/src/write_queue.rs#WriteQueue
        - crates/bitacora-core/src/queue.rs#QueueEvent
      tests:
        - crates/bitacora-core/tests/write_pipeline.rs
        - crates/bitacora-core/src/write_queue.rs
    verified: {rev: "sha256:d407b925c9858e02", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R11:
    status: backlog
    trace:
      code:
        - crates/bitacora-watch/src/watcher.rs#GraphWatcher
        - crates/bitacora-watch/src/ignore.rs#IgnoreRules
        - crates/bitacora-watch/src/process.rs
      tests: [crates/bitacora-watch/tests/watch.rs]
    verified: {rev: "sha256:d02fb21b8b038531", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R12:
    status: backlog
    trace:
      code: [crates/bitacora-watch/src/echo.rs#EchoFilter]
      tests:
        - crates/bitacora-watch/src/echo.rs
        - crates/bitacora-watch/tests/watch.rs
    verified: {rev: "sha256:46e9a185908a353b", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R13:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/external.rs
        - crates/bitacora-core/src/editor/model.rs
        - crates/bitacora-core/src/queue.rs
      tests:
        - crates/bitacora-core/tests/external_changes.rs
        - crates/bitacora-runtime/tests/external.rs
    verified: {rev: "sha256:9a8302107540a051", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R14:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/external.rs
        - crates/bitacora-core/src/queue.rs
      tests: [crates/bitacora-core/tests/external_changes.rs]
    verified: {rev: "sha256:e20056dd9eb3d2fc", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R15:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/external.rs
        - crates/bitacora-core/src/editor/flush.rs
        - crates/bitacora-core/src/queue.rs
        - crates/bitacora-runtime/src/session.rs
      tests:
        - crates/bitacora-core/tests/external_changes.rs
        - crates/bitacora-runtime/tests/external.rs
    verified: {rev: "sha256:8769be9ce565abe6", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
  R16:
    status: backlog
    trace:
      code:
        - crates/bitacora-core/src/editor/external.rs
        - crates/bitacora-core/src/editor/flush.rs
        - crates/bitacora-core/src/queue.rs
        - crates/bitacora-app/src/views/disk_conflict.rs
        - crates/bitacora-app/src/editing.rs
      tests:
        - crates/bitacora-core/tests/external_changes.rs
        - crates/bitacora-runtime/tests/external.rs
        - crates/bitacora-app/src/views/sync_flow_tests.rs
        - crates/bitacora-app/src/editing.rs
    verified: {rev: "sha256:5489b1907819d137", commit: dd4122868facd4ac8435dec6d85c8589a2fab3e5, at: 2026-10-07T00:17:02Z, by: claude}
---

## Purpose
User files are never corrupted or silently overwritten, even when other tools (Logseq, editors, git) change them concurrently.

## Scope
Single-writer queue, atomic writes, hash checks, backups, file watching, echo suppression, external-edit merge. Source: [[block-editor]] §5–6, [[01-file-graph-layout]]. Implemented by BIT-EP-0008.

## Requirements

### BIT-SP-0005.R1 — All graph mutations go through the single-writer command queue in bitacora-core

Every mutation of graph files from the UI, the MCP server, the sync engine and the watcher SHALL be submitted to the `bitacora-core` command queue as an `Op` transaction and applied in submission order by a single writer. No other crate SHALL write `.md` files in the graph directly. Each submitted command SHALL carry its origin (`Ui`, `Mcp`, `Sync`, `External`) for auditing and undo labelling.

#### Scenario: Concurrent UI and MCP edits are serialized
- GIVEN the user is typing in block A and an MCP client appends a block to the same page
- WHEN both commands are submitted within 10 ms
- THEN they are applied one after another, both changes are in the file, and each is a separate undo entry

#### Scenario: No direct writes
- GIVEN the workspace source
- WHEN a test greps for `std::fs::write`/`File::create` on graph paths outside `bitacora-core::writer`
- THEN none is found (enforced by a clippy `disallowed-methods` rule or a test)

### BIT-SP-0005.R2 — Dirty pages are written after a debounce with a maximum delay, without blocking the UI

The write queue SHALL schedule a dirty page 400 ms after its last change but no later than 2 s after its first unwritten change, deduplicating per page, and SHALL perform I/O off the UI thread. All pages touched by one transaction SHALL be flushed in the same batch. On application quit or graph close, pending writes SHALL be flushed synchronously before exit.

#### Scenario: Debounced write
- GIVEN page P is clean
- WHEN the user commits edits at t=0 ms and t=300 ms
- THEN exactly one write of P happens at about t=700 ms

#### Scenario: Max delay under continuous edits
- GIVEN commits every 200 ms for 5 s
- THEN P is written at least every 2 s

#### Scenario: Flush on quit
- GIVEN P is dirty with a change made 50 ms ago
- WHEN the user quits the app
- THEN P is written before the process exits

### BIT-SP-0005.R3 — Serialized output is self-checked by re-parsing before it is written

Before writing, the writer SHALL re-parse the serialized bytes and compare the block tree (depths and texts) with the in-memory model. On mismatch it SHALL fall back to a full canonical render of the page, re-check it, log a bug report with the page path and the diff, and write the canonical render only if it re-parses identically; otherwise it SHALL refuse the write and keep the page dirty with an error notice. Only blocks whose text or depth changed SHALL differ from the previous disk bytes in the normal path.

#### Scenario: Normal write is minimal
- GIVEN `- a\n    - b\n- c\n` (4-space indent) and the user edits `c` to `cc`
- WHEN the page is written
- THEN the file is `- a\n    - b\n- cc\n` and the self-check passes

#### Scenario: Fallback on mismatch
- GIVEN a serializer bug makes the span-preserving output re-parse with a different depth for one block
- WHEN the page is written
- THEN the canonical render is written instead and a bug report is logged

### BIT-SP-0005.R4 — Pre-write hash check never overwrites unseen external changes

Before every write the writer SHALL `stat` the target; if `(len, mtime)` differ from the page's `DiskSnapshot` it SHALL read the file and compare its `blake3` hash with `disk.hash`. If equal, the write proceeds. If different, the writer SHALL NOT write and SHALL hand the page to the external-change path (merge or conflict). A write SHALL never replace bytes that Bitacora has not read.

#### Scenario: Unchanged file
- GIVEN P's disk snapshot hash H and the file still hashes to H
- WHEN P is written
- THEN the write proceeds

#### Scenario: Touched but identical content
- GIVEN `touch` changed only the mtime of P's file
- WHEN P is written
- THEN the hash matches and the write proceeds

#### Scenario: External edit detected
- GIVEN Logseq appended `- from logseq` to P's file after Bitacora read it
- WHEN Bitacora's debounced write of P fires
- THEN the file is not overwritten and the external-change path runs

### BIT-SP-0005.R5 — Writes are atomic and crash-safe (temp file, fsync, rename)

The writer SHALL write to `.<name>.bitacora-tmp` in the target's directory, `fsync` it, `rename` it over the target, and `fsync` the directory where supported. It SHALL preserve the target's permissions. If `rename` fails with an error indicating an unsupported filesystem (some network/FUSE mounts), it SHALL fall back to an in-place write and log a warning. Killing the process at any point SHALL leave either the old or the new complete content in the target. Stale `*.bitacora-tmp` files SHALL be removed at graph open and SHALL be ignored by the watcher and the indexer.

#### Scenario: Kill mid-write
- GIVEN a 1 MB page being written
- WHEN the process is killed (SIGKILL) at a random point during the write, repeated 200 times
- THEN after each run the target hashes to either the old or the new content

#### Scenario: Permissions preserved
- GIVEN a page file with mode `0640`
- WHEN it is written
- THEN its mode is still `0640`

#### Scenario: Stale temp file cleanup
- GIVEN `pages/.foo.md.bitacora-tmp` left by a crash
- WHEN the graph is opened
- THEN the temp file is deleted and `pages/foo.md` is untouched

### BIT-SP-0005.R6 — Encoding and line endings are preserved on edit and Logseq-compatible on create

Files SHALL be written as UTF-8. Files created by Bitacora SHALL use LF, no BOM, and tab indentation. When editing an existing file, newly emitted lines SHALL use the file's detected line ending and indentation unit, and an existing leading BOM and final-newline state SHALL be preserved.

#### Scenario: CRLF file stays CRLF
- GIVEN `- a\r\n- b\r\n`
- WHEN a block `c` is appended
- THEN the file is `- a\r\n- b\r\n- c\r\n`

#### Scenario: New page
- WHEN the user creates page `Ideas` and types `x` and child `y`
- THEN `pages/Ideas.md` is `- x\n\t- y` encoded as UTF-8 without BOM

#### Scenario: BOM kept
- GIVEN a file starting with `EF BB BF` then `- a`
- WHEN `a` is edited to `b`
- THEN the file starts with `EF BB BF` then `- b`

### BIT-SP-0005.R7 — After a successful write the disk snapshot and block origins are rebased

After a successful write the page's `DiskSnapshot` SHALL be set to the written bytes, their `blake3` hash, mtime and length, every emitted block SHALL get a fresh `Origin` pointing into the new bytes (offsets recorded by the serializer), and the page SHALL become clean. Edits committed while the write was in flight SHALL keep the page dirty and be written in the next cycle.

#### Scenario: Rebase makes blocks clean
- GIVEN page `- a\n- b` where `b` was edited to `bb`
- WHEN the write completes
- THEN `bb` has an origin span covering `- bb` in the new bytes and is clean

#### Scenario: Edit during write
- GIVEN a write of P is in progress
- WHEN the user commits another edit to P
- THEN after the write completes P is still dirty and a new write is scheduled

### BIT-SP-0005.R8 — A dirty page whose file was deleted externally is recreated with a notice

If the pre-write check finds the target missing while the page has unwritten changes, the writer SHALL recreate the file (creating parent directories) with the in-memory content and SHALL show a notice "<page> was deleted on disk and has been restored". A clean page whose file is deleted externally SHALL be removed from the in-memory model and index without touching the disk.

#### Scenario: Dirty page restored
- GIVEN P is dirty and `pages/p.md` was removed by `git checkout`
- WHEN the write fires
- THEN `pages/p.md` is recreated with the in-memory content and a notice is shown

#### Scenario: Clean page removed
- GIVEN P is open and clean
- WHEN `pages/p.md` is deleted externally
- THEN P disappears from the page list and no file is written

### BIT-SP-0005.R9 — Backups are written to logseq/bak in Logseq's layout before destructive overwrites

The writer SHOULD back up the previous disk bytes to `logseq/bak/<relative dir>/<file stem>/<ISO-8601 UTC timestamp with ':' replaced by '_'>.Desktop.<ext>` when a write removes text compared with the previous disk content, when a "Keep mine" conflict resolution overwrites an external version, and when an external version replaces unsaved in-memory content. It SHALL keep only the 6 newest backups per directory. Backups SHALL be written atomically and SHALL never be indexed or watched.

#### Scenario: Deletion creates a backup
- GIVEN `pages/foo.md` = `- a\n- b`
- WHEN the user deletes block `b` and the page is written at 2025-11-14T09:30:12.345Z
- THEN `logseq/bak/pages/foo/2025-11-14T09_30_12.345Z.Desktop.md` contains `- a\n- b`

#### Scenario: Append does not back up
- WHEN the user only appends block `c`
- THEN no backup file is created

#### Scenario: Retention
- GIVEN 6 backups exist for `pages/foo.md`
- WHEN a 7th is written
- THEN the oldest one is removed

### BIT-SP-0005.R10 — Write failures keep in-memory state authoritative, retry, and notify the user

If a write fails (permission denied, disk full, I/O error), the page SHALL stay dirty, the in-memory content SHALL remain authoritative, the writer SHALL retry with exponential backoff (1 s, 2 s, 4 s … capped at 60 s), save the new content to `logseq/bak/…` (when possible), and show a persistent error notice naming the file and the error. On quit with failed pages, the app SHALL warn before exiting. Multi-file transactions SHOULD report the list of files that were not written.

#### Scenario: Read-only file
- GIVEN `pages/foo.md` is made read-only
- WHEN an edit to `foo` is written
- THEN a notice "Could not save pages/foo.md: permission denied" is shown, the edit remains visible, and the write is retried

#### Scenario: Recovery
- GIVEN the failure above
- WHEN the file is made writable again
- THEN the next retry succeeds and the notice disappears

#### Scenario: Multi-file report
- GIVEN a transaction touching `a.md`, `b.md`, `c.md` where `b.md` fails
- THEN the error lists `pages/b.md` as not written while `a.md` and `c.md` are written

### BIT-SP-0005.R11 — The graph directory is watched with debounced events and Logseq ignore rules

`bitacora-watch` SHALL watch the graph directory recursively (`notify` + debouncer), coalesce events per path for 100 ms, and then read and hash the file and submit an `External` command to the core queue. It SHALL ignore dot-paths, `.git`, `logseq/bak/`, `logseq/.recycle/`, `logseq/version-files/`, `logseq/graphs-txid.edn`, `logseq/pages-metadata.edn`, `node_modules/`, `.DS_Store`, `*.bitacora-tmp` and config `:hidden` prefixes. Renames SHALL be reported as delete + create when the backend cannot pair them. Watcher errors (e.g. inotify limit reached) SHALL be surfaced with a notice and fall back to a periodic rescan of mtimes.

#### Scenario: External change is picked up
- GIVEN `pages/foo.md` is open and clean
- WHEN a text editor saves `- a\n- b` over it
- THEN within 300 ms the page view shows blocks `a` and `b`

#### Scenario: Ignored paths
- WHEN a file is written under `logseq/bak/pages/foo/`
- THEN no command is submitted

#### Scenario: Burst coalescing
- GIVEN an editor that writes a file in 3 chunks within 50 ms
- THEN exactly one External command is submitted for that path

### BIT-SP-0005.R12 — Watcher echoes of our own writes and no-op changes are suppressed

After each write the writer SHALL record `(path, hash)` in a short-lived echo filter (TTL 5 s); a watcher event whose file hash matches an entry SHALL be dropped and SHALL NOT trigger a reparse, reload or index update. Events whose content hash equals the page's current `DiskSnapshot` hash SHALL also be dropped. Comparison SHALL use exact hashes; whitespace-only differences at the ends (Logseq's trim semantics) SHALL be treated as no-ops for reload decisions but not for overwrite decisions.

#### Scenario: Own write ignored
- GIVEN Bitacora writes `pages/foo.md` with hash H
- WHEN the watcher reports a modify event and the file hashes to H
- THEN no reload happens and the undo stack is unchanged

#### Scenario: External write right after ours
- GIVEN Bitacora wrote hash H and 100 ms later Logseq wrote hash H2
- WHEN the event arrives
- THEN H2 is not in the echo filter and the external-change path runs

#### Scenario: No reparse loop
- GIVEN 100 consecutive edits to the same page
- THEN the number of reparses triggered by the watcher is 0

### BIT-SP-0005.R13 — Clean pages are reloaded on external change with BlockId remapping

When a clean page changes on disk, the core SHALL re-parse it and remap existing `BlockId`s to the new blocks by aligning base and new trees (uuid from `id::` first, then `(parent path, text)` LCS over the DFS sequence), so that scroll position, selection, collapsed UI state and undo history entries keep addressing the same logical blocks. Unmatched old blocks are removed and unmatched new blocks get fresh ids. The reload SHALL be recorded as a non-undoable "External change" transaction.

#### Scenario: Selection survives reload
- GIVEN `- a\n- b\n- c` with `b` selected
- WHEN an editor changes the file to `- a\n- b\n- c2\n- d`
- THEN `b` is still selected and `c2`, `d` are shown

#### Scenario: Undo after unrelated external change
- GIVEN the user edited `a` to `a1` (written), then the file was changed externally to add `- z`
- WHEN `Mod+Z`
- THEN `a1` reverts to `a` and `z` is kept

### BIT-SP-0005.R14 — In-progress edits are protected when the edited block changes on disk

If the block currently in edit mode (or with an uncommitted buffer) changes on disk, the editor SHALL keep the edit buffer intact, mark the block as "conflicted" with a visual indicator, and on commit offer: keep my text, take the disk text, or keep both (disk version inserted as next sibling). Changes on disk to other blocks of the same page SHALL be applied without disturbing the buffer or the caret.

#### Scenario: Other block changes while editing
- GIVEN the user is typing in `a` of `- a\n- b`
- WHEN Logseq changes `b` to `b2` on disk
- THEN `b2` is shown, the buffer and caret of `a` are unchanged

#### Scenario: Same block changes while editing
- GIVEN the user is typing `a-mine` in block `a`
- WHEN the disk changes `a` to `a-theirs`
- THEN the buffer still shows `a-mine`, the block is marked conflicted, and on `Esc` the choice dialog appears
- AND choosing "keep both" yields `- a-mine\n- a-theirs`

### BIT-SP-0005.R15 — Non-overlapping external edits to a dirty page are merged at block granularity

When a dirty page meets changed disk content (pre-write check or watcher), the core SHOULD run a block-level 3-way merge with base = `parse(disk.bytes)`, theirs = `parse(new bytes)`, ours = the in-memory page, aligning blocks by uuid first, then by `(parent path, text)` LCS. The merge SHALL be the shared implementation in the `bitacora-merge` crate (ADR-016). The base is the last bytes Bitacora read or wrote for that file, kept in memory only (ADR-017); there is no on-disk snapshot. If the sets of changed blocks are disjoint and structural changes do not conflict, theirs' changes SHALL be applied as ops onto ours in a non-undoable "External change" transaction and the page written normally; otherwise the page SHALL go to the conflict path. When no base is available (e.g. after a restart), a file with no pending local edits SHALL simply be reloaded, and a page with pending local edits SHALL go to the conflict path with a 2-way per-block diff (disk vs ours). The merge SHALL never insert conflict markers into files.

#### Scenario: Disjoint edits merge
- GIVEN base `- a\n- b\n- c`, ours edits `a` → `A` (unwritten), theirs edits `c` → `C`
- WHEN the write fires
- THEN the file becomes `- A\n- b\n- C` with no prompt

#### Scenario: Both added blocks at end
- GIVEN ours appends `- x` and theirs appends `- y`
- THEN the result contains both `x` and `y` (theirs first, then ours) and no data is lost

#### Scenario: Same block edited on both sides
- GIVEN ours edits `b` → `b1` and theirs edits `b` → `b2`
- THEN no write happens and the conflict notice is shown

#### Scenario: No base after restart
- GIVEN Bitacora restarted and holds no base for `pages/foo.md`
- WHEN the file changes on disk and the page has no pending local edits
- THEN the page is reloaded without a prompt
- AND IF local edits are pending THEN no write happens and the conflict notice shows a 2-way per-block diff

### BIT-SP-0005.R16 — Unresolvable external changes show a non-modal conflict notice with keep-mine / take-disk / show-diff

When an external change cannot be merged, the app SHALL keep the in-memory version, stop writing that page, and show a non-modal banner on the page "Page changed on disk" with actions [Keep mine (overwrite)], [Take disk version] and [Show diff]. Keep mine SHALL back up the disk version to `logseq/bak` and then write ours. Take disk SHALL back up ours to `logseq/bak`, reload the disk version, and add an undoable transaction restoring ours. Show diff SHALL display a block-level diff of disk vs ours; when no merge base is available (after a restart, ADR-017) this 2-way per-block diff is the only comparison offered. Other pages SHALL keep saving normally while the banner is open. The conflicted state SHALL also be exposed to MCP writes, which SHALL be refused on that page with an explicit error.

#### Scenario: Keep mine
- GIVEN a conflict on `pages/foo.md`
- WHEN the user clicks [Keep mine]
- THEN the disk version is backed up under `logseq/bak/pages/foo/` and the file is overwritten with ours

#### Scenario: Take disk
- WHEN the user clicks [Take disk version]
- THEN the page shows the disk content, ours is backed up, and `Mod+Z` brings ours back

#### Scenario: MCP write refused
- GIVEN the page is conflicted
- WHEN an MCP client calls a write tool on it
- THEN it receives an error "page has an unresolved on-disk conflict"
