---
created_at: 2026-10-06T16:59:21.01409082Z
updated_at: 2026-10-06T16:59:21.01409082Z
tags:
    - change
    - core
    - compat
---
# Core: naming codecs, journals, graph discovery

Commit 236db92 on worktree branch. Continues [[bitacora-full-development-plan]]; spec [[01-file-graph-layout]] §1.1, §3, §4.

## What changed (crates/bitacora-core/src)
- `naming.rs`: `triple_lowbar::{encode,decode}`, `legacy::{encode,decode,dot_encode}`, `page_name_sanity`, `page_key`, `path_file_body`, `title_roundtrip_mismatch`, `needs_title_property` (legacy only at creation), `derive_title(rel_path, props_title, cfg)` (title:: value passed in; markdown extraction is another story).
- `date.rs`: `Date`, `DateFormat` (Joda subset: y/yy/yyyy, M..MMMM, d/dd/do, E..EEEE, quoted literals), strict parser.
- `journal.rs`: `capitalize_all`, `journal_title_formatters`, `detect_journal`, `journal_page`, `journal_file_path`, `JournalPage::FILE_RENAME_ALLOWED=false`.
- `graph_path.rs`: `GraphPath` (relative, NFC, `/`).
- `scan.rs`: `is_ignored_path`, `has_allowed_extension`, `scan_graph` (read-only, skips symlinks/dot entries, `:hidden`), `parse_order` (filter-files ordering, UTF-16 sort), `decode_text`/`read_text` (BOM stripped for parsing only).
- New workspace dep `unicode-normalization` (cargo deny ok); core deps walkdir; dev proptest/tempfile.

## Notes
- Faithful quirk: root-level `node_modules/` is not ignored (rule needs `/node_modules/` inside the trimmed path).
- Parser choices not oracle-checked: yy -> 2000+yy, 1-2 digit d/M, 4 digit yyyy.

## Verification
`cargo test -p bitacora-core --locked`: 31 passed; clippy workspace -D warnings clean; cargo deny ok; xtask check-deps ok. Black-box Logseq oracle checks pending (bb unavailable).
