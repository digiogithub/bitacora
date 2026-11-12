# rename.expected (golden output of Logseq 0.10.15)

What Logseq 0.10.15 (desktop, Linux x64 AppImage) wrote after renaming the page `Old` to
`New Idea` in a copy of `../rename`. Used by `crates/bitacora-core/tests/rename_golden.rs`.
Logseq is the black-box oracle here (ADR-015): the input graph is ours, and only the files it
produced are stored.

## How it was produced (2026-11-12)

1. Copy `fixtures/graphs/rename` to a scratch folder.
2. Start Logseq 0.10.15 with an empty profile (own `HOME` and `--user-data-dir`), headless under
   `xvfb-run`, with `--remote-debugging-port` and `--inspect`.
3. In the main process, replace `dialog.showOpenDialog` so that "Add graph" returns the scratch
   folder; click "Add graph" and the folder button in the window; wait for the graph to parse.
4. In the window, run `logseq.api.rename_page('Old', 'New Idea')` and wait for the file writes.
5. Copy `pages/`, `journals/` and `logseq/config.edn` back here.

Left out on purpose: `logseq/bak/`, `logseq/.recycle/`, and the files Logseq creates when it
opens any graph (`pages/contents.md`, `logseq/custom.css`), which the rename did not produce.

## What Logseq did, as observed

- Files are re-serialized as a whole: every rewritten file lost its final newline.
- `#Old` and `#old` become `#[[New Idea]]`; `[[old]]` (lower case) is left alone.
- The property key `old::` becomes `new-idea::`.
- `tags:: Old, x` is not rewritten, neither in a block nor in the page properties.
- `:favorites` gains `"new idea"` and keeps `"Old"`; `:default-home` is renamed.
- References in inline code and fenced code are untouched.
