---
created_at: 2026-10-06T17:00:25.652872555Z
updated_at: 2026-10-06T17:00:25.652872555Z
tags:
    - change
    - markdown
    - parser
---
# BIT-US-0026 + BIT-US-0058: outline splitter, block tree, property scanner

Part of [[bitacora-full-development-plan]]. Designs: [[02-markdown-block-syntax]], [[block-editor]] (byte-preserving model, ADR-003).

## What changed (crate `bitacora-markdown`, commits 4dd0f4f, b2f00cc)
- `src/span.rs` `Span`; `src/lines.rs` `Lines`, `LineKind`, `RegionKind/Part`, `ParserOptions`, `UnclosedRegion` (T-0060).
- `src/outline.rs` `split`, `split_with`, `Outline`, `RawBlock`, `BlockKind`, `content_of`, `pre_block_content` (T-0061).
- `src/tree.rs` `build_tree`, `build_tree_from_levels`, `NodeLinks` (T-0062).
- `src/properties/{scan,value,drawer}.rs`: `scan_properties`, `PropertyGroup`/`PropLine`, `normalize_key`, `is_valid_key`, `interpret`, `PropValue`, `PropertyConfig`, `scan_refs`, `read_drawer` (T-0091..0093).
- Fixtures `fixtures/markdown/outline/*` (16 files + expected JSON from mldoc 1.5.7), `tools/mldoc-diff/` (outline.js, gen-outline-expected.js), tests `tests/outline_fixtures.rs`, `tests/outline_roundtrip.rs` (proptest). Dev-deps: proptest, serde_json.
- Docs: 02-markdown-block-syntax section 12 open questions 1 (unclosed regions) and 2 (BOM) answered; section 2.1/3.1 observed rules added.

## Observed mldoc 1.5.7 facts (verified by running mldoc)
- Unclosed fence / #+BEGIN: plain text, later bullets still split blocks.
- Fence closes on the next line starting with ``` or ~~~ (any kind/length, any indent). Bullet line can open a fence. #+END_X prefix match case-insensitive; no nesting.
- ATX heading lines are blocks at any indent; Logseq forces level 1.
- BOM is part of the first line (not stripped): `﻿- a` is not a block.
- Front matter hides list items; unclosed = hr.
- Property: text after the bullet can be a property; `a::\tb`, `a:b:: c`, `a:::` are text; blank/text lines split groups; `#+x:` joins only after a property.

## Verification
cargo fmt, `cargo clippy --workspace --all-targets --locked -- -D warnings` clean, `cargo test -p bitacora-markdown --locked`: 60 unit + 2 fixture + 3 proptest tests pass; `cargo xtask check-deps` OK; `cargo deny check` OK.

## Spec traces
BIT-SP-0001.R1, R2, R4, R5, R6, R14, R19 (code+tests). R3 only partly (pre-block span; no hoisting of `#+key:` / front-matter property parsing yet). `verify_requirement` not run (needs ingested test results).
