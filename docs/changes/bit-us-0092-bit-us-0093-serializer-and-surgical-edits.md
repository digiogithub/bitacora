---
created_at: 2026-10-06T17:09:10.785161347Z
updated_at: 2026-10-06T17:09:10.785161347Z
tags:
    - change
    - markdown
    - serializer
---
# BIT-US-0092 + BIT-US-0093: byte-preserving serializer and surgical block edits

Part of [[bitacora-full-development-plan]]. Designs: [[02-markdown-block-syntax]] (§2.2, §2.7, §3.3, §5.3, §7), [[block-editor]], ADR-003, ADR-006, ADR-015.

## What changed (crate `bitacora-markdown`, commits ed41b87, bd47082)
- `src/doc.rs`: `Document`, `Node::{Original, Pre, Edited}`; dirty tracking, `set_block_content` (no-op when semantically unchanged), `insert_block`, `remove_block`, `set_depth`, `set_pre_block_content`; detects EOL, indent unit, BOM.
- `src/serialize.rs`: `serialize(&Document, &WriteOptions)`; originals verbatim, edited via canonical writer; inserts a line break at boundaries; edited last block keeps the file's EOF-newline convention.
- `src/canonical.rs`: `Eol`, `IndentUnit` (+detect), `write_block`, `write_pre_block`, `convert_drawers` (`:PROPERTIES:` -> `key:: value` only on edited blocks). Logseq quirks (bullet-less heading first block, auto pre-block) deliberately not reproduced.
- `src/image_meta.rs`: EDN map after `![..](..)`, `set_size` in pr-str style. Placed at crate root (not `inline/`) to avoid clashing with the parallel inline-scanner work.
- `src/edit/{properties,identity,state}.rs`: `set_property`, `set_property_values`, `remove_property`, `get_property`, `set_front_matter_property`; `ensure_block_id`, `block_id`, `find_duplicate_ids`, `strip_self_ref` (uuid crate added as dependency); `set_collapsed` (`CollapseMode`), `set_scheduled`/`set_deadline` (`Timestamp`), `clock_in`/`clock_out` (`ClockTime`), `set_marker`.
- Tests: `tests/serializer.rs` (round-trip over fixtures/graphs/** and fixtures/markdown/**, single-block edit keeps prefix/suffix bytes, §7 example rebuilt), `tests/edit_golden.rs` + insta snapshots.

## Choices
- New clock entries go first inside an existing `:LOGBOOK:`; a new logbook goes after title, planning lines and the property group.
- Only the first property group is edited; drawer converted when a property is set/removed in a drawer block.
- SCHEDULED is placed right after the title, DEADLINE after SCHEDULED.

## Verification
fmt, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test -p bitacora-markdown --locked` (98 unit + integration tests incl. 4 serializer + 3 golden), `cargo xtask check-deps`, `cargo deny check` all clean. Logseq black-box verification of edit vectors pending (no Logseq tooling here).
