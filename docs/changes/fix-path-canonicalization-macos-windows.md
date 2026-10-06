---
created_at: 2026-10-06T20:58:35.23216769Z
updated_at: 2026-10-06T20:58:35.23216769Z
---
# Fix: path canonicalization on macOS and Windows

Symptom: CI test-core on macOS/Windows failed `layout_matches_adr_005_and_rejects_paths_inside_graph` because `IndexLocation::in_data_dir` compared a canonical graph path to a non-canonical, not-yet-existing data dir (`/var` vs `/private/var`, 8.3 short names).

Changes
- `crates/bitacora-index/src/location.rs`: `absolute()` now walks components, canonicalizing while the prefix exists and applying the remainder (incl. `..`) lexically. Used by `graph_id` and `in_data_dir`. New unit tests (symlinked parents on Unix, `..` handling).
- `crates/bitacora-runtime/src/crash.rs`: `Redactor::new` also redacts the canonical spelling of graph roots and home; Unix symlink test.
- `.github/workflows/ci.yml`: test-core and test-core-os use `--no-fail-fast`.

Audit: core `GraphPath::from_abs` (lexical, fed by walkdir from same root), watch (canonicalizes root), MCP `read_asset` (both sides canonicalized), cli `resolve` (canonicalizes graph) are consistent. No new dependency (no dunce; both sides use std canonicalize so verbatim `\\?\` prefixes match).

Verification: cargo test -p bitacora-index -p bitacora-runtime --locked pass; clippy clean. macOS/Windows legs unverified locally.
