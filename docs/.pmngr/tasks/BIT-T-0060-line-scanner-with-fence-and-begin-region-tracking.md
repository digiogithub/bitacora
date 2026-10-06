---
id: BIT-T-0060
type: task
title: "Line scanner with fence and #+BEGIN region tracking"
status: in_progress
parent: BIT-US-0026
milestone: BIT-M-0002
author: mcp
labels: [bitacora-markdown, parser]
estimate: 3
created: 2026-10-06T14:28:29Z
updated: 2026-10-06T16:46:41Z
started: 2026-10-06T16:46:41Z
---

## Description
Implement `crates/bitacora-markdown/src/lines.rs`: an iterator over the input `&[u8]` yielding `Line { start, end, eol_len, indent: &[u8], kind }` where `eol_len` is 0/1/2 (`\n` or `\r\n`). Classify each line:
- `BulletStart { indent_len, after_dash }` when it matches `^[ \t]*-([ \t]|\r?$)` (`mldoc:lib/syntax/heading0.ml:71-75`);
- `AtxHeading { hashes }` when at column 0 it matches `^#{1,}[ \t]` (only meaningful for top level; `#foo` stays `Text`);
- `Text` otherwise.
Track open regions so lines inside them are always `Text`: fences opened by ```` ``` ```` or `~~~` (after optional indent, closing with the same char and ≥ the same run length) and `#+BEGIN_<NAME>` … `#+END_<NAME>` (case-insensitive, `mldoc block0.ml:127-221`). A bullet line at column 0 inside an open fence is not a block start ([[02-markdown-block-syntax]] §2.1).
Provisional rule for an unclosed fence/`#+BEGIN` (open question §12.1): treat region as open to EOF behind a `ParserOptions::unclosed_region` enum (`ToEof` | `FallbackToParagraph`), default chosen after the differential fixtures task resolves the question.

## Acceptance Criteria
- Unit tests: `-foo` → Text; `- ` and `-` at EOF/EOL → BulletStart; `\t- d` indent_len 1; `"    - c"` indent_len 4.
- Fence containing `- x`, `foo:: bar`, `[[x]]` yields only Text lines.
- `#+begin_quote` / `#+END_QUOTE` mixed case closes correctly.
- CRLF lines report `eol_len == 2` and never include `\r` in `indent`.

## Notes
Part of BIT-US-0026. Implements BIT-SP-0001.R1, R2, R19. ADR-003. No dependency on other bitacora crates.
