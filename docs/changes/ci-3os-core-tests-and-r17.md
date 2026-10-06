---
created_at: 2026-10-06T20:27:22.806872322Z
updated_at: 2026-10-06T20:27:22.806872322Z
tags:
    - change
    - ci
    - watch
    - backlog
---
# CI headless tests on 3 OSes and BIT-SP-0002.R17 linkage

Continues [[bitacora-full-development-plan]].

## CI
- `.github/workflows/ci.yml`: new job `test-core-os` (macos-latest, windows-latest) running `cargo test --workspace --exclude bitacora-app --locked`; own cache keys `test-core-<os>`, `cache-save-if` main only, Windows CRLF checkout like `test-app`. actionlint clean.
- Audit of Unix assumptions in non-app crates: permissions/symlink/exec-bit tests are already `cfg(unix)`-guarded (mcp tokens, core scan/write_pipeline, sync backends, cli self_update). No code change made. Unverified on real Windows/macOS: tests spawning `git` (testkit), `hostname`, `node` (mldoc corpus, skips when absent), crash_safety child process.

## R17
- Logseq backup client label is `Desktop`: `src/electron/electron/backup_file.cljs:47` (`".Desktop" ext`). R17 scenario updated from `.Bitacora.` to `.Desktop.` (matches `crates/bitacora-core/src/editor/backup.rs`, AGENTS rule 2).
- `Implements: BIT-SP-0002.R17` added to Notes of BIT-US-0065/0066/0067/0069 (update_item has no links param). R17 trace.code/tests set.
- Watcher: journals (`journals/*`) whose trimmed content is `-` are no longer emitted (`is_blank_journal` in `crates/bitacora-watch/src/process.rs`, test `blank_journal_is_ignored_but_real_content_is_reported`). NOT implemented: ignoring a journal equal to the default template (needs graph config in the watcher); R17 not verified for that reason.

## Verification
`cargo test -p bitacora-watch --locked` (6 + 12 passed), clippy -D warnings on bitacora-watch clean.
