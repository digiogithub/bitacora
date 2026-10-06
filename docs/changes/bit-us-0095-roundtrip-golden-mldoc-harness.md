---
created_at: 2026-10-06T17:36:15.540738382Z
updated_at: 2026-10-06T17:36:15.540738382Z
tags:
    - change
    - markdown
    - testing
---
# BIT-US-0095 round-trip, golden and mldoc differential harness

Plan: [[bitacora-full-development-plan]]. Continues [[bit-us-0083-0084-0059-inline-scanner-tasks-page-props]] and [[bit-us-0092-bit-us-0093-serializer-and-surgical-edits]]. Branch commit e752b9e (worktree branch, merged by orchestrator).

## What changed
- `crates/bitacora-markdown/tests/roundtrip_suite.rs`: every fixture round-trips through outline spans, Document and analyze; the 19 "round-trip fixtures to write ourselves" of 02-markdown-block-syntax section 11 as in-memory cases; proptest over mutated corpus files (cut, drop/dup line, CRLF); hand-written golden canonical serialization of edited blocks (CRLF, tabs+BOM, insert/depth, no-EOL, remove).
- `crates/bitacora-markdown/tests/mldoc_corpus.rs` + `tools/mldoc-diff/corpus.js`: differential run over ALL fixtures (graphs + markdown), per block start/level/marker/priority/property keys/pages/block refs. Result: zero divergences on the real corpus. Allowlist `fixtures/markdown/corpus-allowlist.txt` (stale entries fail). Skippable: auto-skips when node or `tools/mldoc-diff/node_modules/mldoc` is missing; `BITACORA_SKIP_MLDOC=1` skips; `BITACORA_REQUIRE_MLDOC=1` makes unavailability a failure (use in CI with node + `npm install`); `BITACORA_MLDOC_REPORT=1` prints divergences. `#+BEGIN_QUERY` bodies are skipped by the oracle walk (Logseq skips them).
- `fixtures/markdown/divergences/*.md`: the known divergences (inline HTML w/o close, lone `- TODO` at EOF without newline are real entries; stray `:END:`, `+1w 10:30`, `#[[a]]b` documented as not visible in compared fields).
- `benches/split.rs`: criterion benches on a ~700 KiB page (split ~360us, Document::parse ~500us, clean serialize ~46us, one-edit serialize ~121us).
- Cleanup: `edit::state::Timestamp` now formats through `tasks::Timestamp::format` (weekday helpers moved to `tasks/timestamp.rs`, `year` is now `u32`), `set_marker` uses `tasks::Marker`. Output identical (golden/snapshots unchanged).

## Front matter lists and ref kinds (orchestrator question)
- YAML `tags: [a, b]` yields pages `[a` and `b]` **in Logseq itself**: `parse-property` -> `extract-refs-by-commas` runs `sep-by-comma` on the mldoc Plain text `[a, b]` (text.cljs:130-165; mldoc gives `Plain "[a, b]"`). So `interpret` is compatible and NOT changed; vector added in `properties/value.rs` tests and in `fixtures/markdown/page-props/cases.txt`. Block-style YAML lists (`alias:\n  - x`) make mldoc drop the whole front matter (no Directives), same as ours. If the product wants friendlier behaviour it is a deliberate divergence needing an ADR.
- `[[x]]` and `#x` in one block: Logseq stores `:block/refs` as a set of pages with no kind (block.cljs get-page-reference includes Tag at line ~78), so both collapse to one ref to page `x`. `:block/tags` comes only from the Heading `tags` field (always empty for Markdown inline tags) and from the `tags::` property. The index should record kind-agnostic refs; "tag" is syntax only.

## Spec verification procedure (gintrack)
1. `cargo nextest run -p bitacora-markdown --profile ci --locked` (config `.config/nextest.toml`, report `target/nextest/ci/junit.xml`; `cargo install cargo-nextest --locked`).
2. `python3 tools/junit-paths.py target/nextest/ci/junit.xml .` adds a `file` attribute per testcase. Required: nextest's classname is the crate name, which gintrack cannot map (all 196 tests were `unmapped` without it). Unit tests map to `crates/<crate>/src/<module>.rs`, integration tests to `crates/<crate>/tests/<bin>.rs`.
3. `gintrack spec ingest target/nextest/ci/junit.xml --repo <repo root>`; the symbol is the bare test name, so trace refs `path#test_name` and whole-file refs both work. Trace entries must point at test files, not fixture data files (those stay `missing`).
4. `verify_requirement` (or `gintrack spec verify BIT-SP-0001.Rn --commit`). Ingest records HEAD of the repo it runs in and requires the traced files to exist there, so run it on the merged tree.
Status: ingest verified in the worktree (196/196 mapped). Stamps NOT written: the new test files are not in main yet and results would be attributed to main HEAD. Orchestrator should run steps 1-4 on main after merging; expect SP-0001 R1,R2,R4-R19 to pass (R3/R8/R9/R10/R12/R19 traces fixed to point at test files and new suites).

## Verification
fmt, clippy -D warnings clean (bitacora-markdown), `cargo test -p bitacora-markdown --locked` all green (165 unit + integration incl. 9 roundtrip_suite, mldoc_corpus ~2.4s), nextest 196/196, check-deps OK.
