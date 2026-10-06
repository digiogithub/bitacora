# Block editor spike report (ADR-002)

> Status: **preliminary go** for ADR-002 (custom block text element on `EntityInputHandler`). Linux results are in;
> macOS, Windows and real-IME validation are **pending** (BIT-US-0072 stays `in_review`).
> Written 2026-10-06 for BIT-US-0040 (custom element), BIT-US-0060 (1,000-block performance) and BIT-US-0072 (report).
> Inputs: [[block-editor]] §7, [[gpui-and-gpui-kit]] §1.2, §3, risks R3, R4, R8. Checklist: [[ime-test-checklist]].
> Code: `crates/bitacora-app/src/spike/block_editor/` (throwaway; BIT-EP-0007 re-implements on top of `bitacora-core`).

## 1. Summary and verdict

| Question | Answer |
|---|---|
| Can a custom `BlockTextElement` on `EntityInputHandler` deliver soft wrap, caret, selection, IME marked text, clipboard and in-block undo? | **Yes.** About 2,500 lines including tests, one afternoon of work, no blocking GPUI limitation found. |
| Can it do the outliner behaviours (Up/Down across blocks keeping x, Enter split, Backspace merge, Tab/Shift-Tab, fold, click-to-caret on rendered text)? | **Yes**, covered by 19 keystroke-level `#[gpui::test]`s plus 20 pure unit tests. |
| Is a 1,000+ block virtualized page fast enough? | **Yes** on Linux (release build, X11, software Vulkan): about 2.4 ms CPU per scrolled frame (21 rows built per frame), 1.6 ms edit-to-paint in a mid-page block, 135 ms to the first frame, flat memory from 1,000 to 5,000 blocks. |
| Does real IME work on macOS, Windows and Linux (ibus, fcitx5)? | **Not verified.** The `EntityInputHandler` contract is implemented and unit-tested; platform behaviour needs the manual run of [[ime-test-checklist]]. |
| Verdict | **Go (preliminary) with a documented fallback.** Keep ADR-002. Do not start the editor epic's IME-dependent stories until the three-OS checklist has run. If a platform shows a blocker rooted in GPUI, the fallback is a GPUI Kit `Textarea` per block (section 6), which the probe shows is feasible for edge-key interception. |

No ADR row is added: the decision stands. The ADR-002 rationale now points to this report.

## 2. Setup

| Item | Value |
|---|---|
| UI stack | `gpui-kit =0.7.1` (default features: `component`, `assets`; **tree-sitter off**), `gpui-pre 0.3.8` |
| Toolchain | Rust stable 1.98.1 (workspace MSRV 1.90) |
| Host | Intel Core Ultra 9 285 (24 threads), 96 GB RAM, Linux 7.1.5 |
| Display for the benchmark | Xvfb 1280x900 (X11) with Mesa **lavapipe** (llvmpipe, software Vulkan). `WAYLAND_DISPLAY` unset so GPUI uses X11. |
| Why no real GPU | In this session the Wayland compositor delivered no frames to any window (even the unmodified shell's `--smoke-test` times out), so the real Intel/NVIDIA numbers were not obtained. Software rendering moves GPU time off the CPU timers below; CPU-side costs are what the benchmark measures. |
| Build | `cargo build --release -p bitacora-app` (`lto = "thin"`, `codegen-units = 1`, `strip = "debuginfo"`) |
| Run | `bitacora --spike-editor [--spike-blocks N] [--spike-page FILE.md]`, `bitacora --spike-bench ...` prints one `SPIKE_BENCH {json}` line |

## 3. What was built

| Piece | File | Notes |
|---|---|---|
| Text helpers | `text_ops.rs` | UTF-8/UTF-16 offset conversion (surrogate pairs, mid-char rounding), grapheme and word boundaries (`unicode-segmentation`). |
| Focused-block buffer | `buffer.rs` | Selection, reversed flag, marked range, snapshot undo with typing coalescing (one step per word, one per IME composition). |
| Outline model | `doc.rs` | Flat `Vec` with depths. split (first child when the block has visible children), merge into the previous visible block (children re-parented), indent, outdent (following siblings become children, Logseq's default), fold. Depth invariant checked. |
| Inline styling and click map | `inline.rs` | Tokenizer for `[[ref]]`, `**bold**`, `` `code` ``, `TODO`/`DONE`. Source-aligned runs for the editor; display string with hidden `**` and an offset map for unfocused blocks; bounded render cache. |
| Geometry | `layout.rs` | Visual row table over GPUI `WrappedLine`s (soft wrap plus hard newlines): caret position, hit testing, per-row selection rectangles, IME bounds. |
| Custom element | `element.rs` | `BlockTextElement` (measured layout, `shape_text`, selection quads, caret, `window.handle_input`). |
| Editor entity | `editor.rs` | Actions and key bindings, cross-block model, `EntityInputHandler`, unfocused rows (`StyledText` plus click-to-caret), bullets, guides, fold arrows, `gpui::list`. |
| Harness | `bench.rs`, `frame_marker.rs` | Frame-driven measurement; prints JSON. |
| Probe | `textarea_probe.rs` | Tests pinning how GPUI Kit's `Textarea` handles edge keys (section 6). |

Keyboard model (default bindings; `Mod` is Cmd on macOS and Ctrl elsewhere):

| Key | Behaviour |
|---|---|
| Up / Down | Move one visual row keeping `goal_x`; on the first/last row move to the adjacent visible block, caret on its last/first row at the same x (indent-corrected). Shift variants select to the block edge. |
| Left at 0 / Right at end | Move to the end/start of the neighbour. |
| Enter | Split at the caret. On an empty nested block it outdents. While an IME composition is marked it only commits. |
| Shift+Enter | Soft newline inside the block. |
| Backspace at 0 / Delete at end | Merge with the previous / next visible block. |
| Tab / Shift+Tab | Indent / outdent (Tab is ignored while marked text exists). |
| Mod+Up / Mod+Down | Collapse / expand the focused block's subtree. |
| Mod+A/C/X/V, Mod+Z, Mod+Shift+Z | Select all, clipboard, in-block undo/redo. |
| Home/End, word keys | Visual-row edges; word motion (Alt on macOS, Ctrl elsewhere). |

## 4. Findings

### 4.1 Wrapping, caret and hit testing

- GPUI's `WrappedLine::position_for_index` returns the *end of the previous row* for an index exactly at a soft-wrap
  boundary. For typing, the caret must be at the start of the next row, so `BlockLayout` keeps its own row table
  (`row_for_index` = last row starting at or before the offset) built from `wrap_boundaries()` and the unwrapped glyph
  positions. All caret, selection and vertical-move logic runs on that table.
- `shape_text` splits at `\n` and consumes one byte of the run list per newline: the `TextRun`s must cover the
  newline bytes. Our runs come from a tokenizer that covers the whole string, so this holds by construction.
- Wrapped layouts are cached by GPUI per frame (current plus previous frame, keyed by text, size, runs and wrap
  width). Shaping the focused block again in a key handler (for `goal_x` movement) is therefore cheap, and handlers
  and paint stay consistent as long as they use the same runs and width. The editor records the painted width and the
  base font from the element.
- A click or `closest_index_for_x` at the right end of a soft-wrapped row must not return the row end (it is the next
  row's start); `index_on_row` backs off one char.
- `gpui::test` uses a deterministic fake text system (equal advance for ASCII), so geometry tests (goal-x
  navigation, wrap row counts, selection rectangles, IME bounds) run headless and are stable.

### 4.2 IME and the input handler

- `EntityInputHandler` is fully implemented: `text_for_range`, `selected_text_range`, `marked_text_range`,
  `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `bounds_for_range` (caret/selection
  rectangle on the right visual row, in window coordinates), `character_index_for_point`,
  `set_selected_text_range`, `text_length_utf16`.
- GPUI's `examples/input.rs` has three pitfalls that the spike fixes and the real editor must keep fixed:
  1. the new selection after `replace_and_mark_text_in_range` is given **relative to the new text**, but the example
     converts it with the *whole buffer*; and it adds `range.end` to the end of the selection instead of
     `range.start`;
  2. UTF-16 offsets in the middle of a surrogate pair are not rounded;
  3. `character_index_for_point` asserts that the cached layout equals the live text, which fails during a fast
     composition.
- Composition is one undo step: the snapshot is taken when the first marked text arrives and not while it changes.
- Enter during composition commits instead of splitting, Tab is ignored while marked. Whether GPUI forwards these keys
  to the IME before key bindings fire is platform-specific and is cases C17/C18 of the checklist.
- Everything the platform sends is clamped to char boundaries and the text length, so a misbehaving IME cannot panic
  the editor (the helpers use no `unwrap`).

### 4.3 Cross-block model and virtualization

- One `SpikeEditor` entity owns the page, the focused block's buffer and one `FocusHandle`; the key context
  `SpikeBlockEditor` carries the actions. Only the focused row paints through `BlockTextElement`; every other row is a
  `StyledText` with `with_runs`, so only one live editor exists, as designed.
- Unfocused click-to-caret works through `StyledText::layout().index_for_position` and an offset map: hidden `**`
  markers make display and source offsets differ; boundaries map to the end of the earlier visible segment (a click
  right after bold text lands before the closing `**`). `[[`/`]]` stay visible but dimmed, like Logseq.
- `ListState::splice` is used per structural change: fold/unfold and indent/outdent splice only the changed range
  (prefix/suffix diff of the visible rows); split/merge shift block indexes, so the rows from the first changed row
  down are re-measured lazily. With stable block ids in `bitacora-core` this shrinks to the inserted/removed rows.
- **`ListState::scroll_to_reveal_item` does not reach far-away rows that were never measured** (their heights are
  not known, so the computed offset is wrong). On a 2,681-block page the benchmark never saw its mid-page row. Use
  `ListState::scroll_to(ListOffset { item_ix, offset_in_item: 0 })` to jump (`SpikeEditor::scroll_to_row`); reveal is
  fine for adjacent navigation.
- `h_flex()` in GPUI Kit centers its children vertically; a row with a bullet next to wrapped text needs
  `.items_stretch()` (or `items_start`) or the bullet floats in the middle.
- `Element::into_any` and `IntoElement::into_any_element` are different methods (the first is only on `Element`);
  `str::floor_char_boundary` needs Rust 1.91 (workspace MSRV 1.90, clippy `incompatible_msrv`), so `text_ops` has its
  own.

### 4.4 Clipboard and undo

- Copy, cut and paste of plain text work through `cx.write_to_clipboard` / `read_from_clipboard`; pasted newlines
  stay inside the block in the spike (the real editor turns blank-line separated paragraphs into blocks,
  [[block-editor]] §7.5).
- Undo is a per-block snapshot stack (200 steps, typing coalesced by word). It is lost on block change by design: the
  real undo is the page-level transaction log in `bitacora-core` ([[block-editor]] §4).

## 5. Performance

Method: `bitacora --spike-bench` after the window opened. Five warm-up frames, one unrecorded scroll pass that
measures every row once, then 240 scroll frames of 130 px each with the render cache on, 240 with it off, 100 single
character edits in a block in the middle of the page (`replace_text_in_range`, the path IME and typing use), and a
collapse of the largest subtree. CPU per frame is the time from the view's `render` to the end of paint (a zero-size
marker element painted last); edit latency is edit call to the end of the edited block's paint. GPU submission,
presentation and the display's vsync are **not** included.

Targets stated up front: 60 fps scroll (CPU p95 per frame below 16.7 ms), keystroke-to-paint p95 below 16 ms, cold
start below 1 s.

| Page | Blocks | Cold start to 2nd frame | RSS after first frames | RSS after full scroll | Scroll CPU per frame (mean / p95 / max) | Edit to paint (mean / p95 / max) | Fold of 442 rows |
|---|---|---|---|---|---|---|---|
| Generated, run 1 | 1,000 | 135 ms | 201 MiB | 215 MiB | 2.5 / 3.2 / 3.4 ms | 1.6 / 2.0 / 2.3 ms | 0.11 ms |
| Generated, run 2 | 1,000 | 130 ms | 201 MiB | 215 MiB | 2.5 / 3.2 / 3.5 ms | 1.7 / 2.0 / 2.3 ms | 0.10 ms |
| Generated, run 3 | 1,000 | 132 ms | 201 MiB | 215 MiB | 2.4 / 3.1 / 3.7 ms | 1.6 / 2.1 / 2.2 ms | 0.07 ms |
| `logseq-docs/pages/Changelog.md` | 2,681 | 135 ms | 200 MiB | 213 MiB | 2.3 / 3.0 / 4.0 ms | 1.5 / 1.8 / 1.8 ms | 0.10 ms (72 rows) |
| Generated | 5,000 | 138 ms | 202 MiB | 216 MiB | 2.3 / 3.1 / 4.1 ms | 1.6 / 1.8 / 1.9 ms | 0.11 ms |

- **Pass/fail:** scroll CPU p95 3.0-3.2 ms against 16.7 ms: **pass** (about 5x headroom); edit-to-paint p95 about
  2 ms against 16 ms: **pass**; cold start about 135 ms against 1 s: **pass** (this is the spike window only, not the
  full workspace shell; the shell's own `--smoke-test` logged 207-575 ms in the previous report).
- Rows built per scrolled frame: 21-26 (virtualization works; visible rows plus overdraw), 0.2 ms per frame in the
  row builder.
- Memory does not grow with the page: 201 MiB at 1,000 blocks and 202 MiB at 5,000 blocks. Almost all of it is the
  GPUI/Vulkan baseline (software rasterizer here); scrolling the page adds about 14 MiB (glyph atlas and layout
  caches).
- The frame *interval* is exactly 48 ms in every run: that is the Xvfb display pacing (three 16 ms ticks), not
  the cost of a frame, so FPS cannot be read from it. It is reported in the raw JSON only. Real FPS on a GPU and
  vsync needs the same run on real hardware (follow-up).
- The render cache of the spike (tokenize once per distinct text) made no measurable difference (2.3-2.5 ms with and
  without it; row building is 0.2 ms per frame). GPUI's own per-frame line-layout cache and the lazily measured list
  do the heavy lifting. The cache is kept only because it is cheap; the real implementation should cache the parsed
  inline AST per (block id, content hash), which it needs anyway for the unfocused view.
- Release binary: **43.6 MB** (`lto = "thin"`, `strip = "debuginfo"`, default GPUI Kit features, no tree-sitter).
  With GPUI Kit's bare `tree-sitter` feature (no language grammar features): **44.1 MB** (+0.5 MB, about 1%); each
  `tree-sitter-<language>` feature adds its grammar on top, so enable none (the editor does not need code
  highlighting).
- Debug build (opt-level 1 dependencies): edit to paint about 2.1 ms mean, so the interactive loop is also fine
  unoptimized.

macOS and Windows runs of the same command are pending (the harness is platform neutral; RSS is read from `/proc` on
Linux only and needs a per-OS reader).

## 6. GPUI Kit Textarea per block (Option A) compared with the custom element (Option C)

Evidence: reading `gpui-component 0.7.1` / `gpui-base 0.7.1` (`input/base/state.rs`, `movement.rs`) and the tests of
`textarea_probe.rs`, which run a `Textarea` inside a parent with action listeners. No full Textarea-per-block
prototype was built (BIT-T-0105 asks for one; this is a desk plus probe evaluation, see the limits below).

| Capability | A: `Textarea` per block | C: custom element (spike) |
|---|---|---|
| IME / marked text | Provided, hardened by the kit (v0.7.1 release notes mention IME fixes); same `EntityInputHandler` machinery underneath. | Implemented by us on the same API; we own the bugs, and the example's pitfalls (4.2). |
| Soft wrap, caret, selection | Provided (auto-grow via `auto_grow(min_rows, max_rows)`). | Implemented (`layout.rs`, about 200 lines). |
| Inline styling (dim `[[`, bold, code, TODO) | Limited: plain text plus decorations and **atomic inline tokens** (`Textarea::token`, `on_token_click`) for refs. Styling arbitrary spans of the raw Markdown is not the intended use. | Free-form `TextRun`s from our tokenizer. |
| Edge keys for navigation | Up/Down: consumed by the Textarea (never bubbles) but visible to an ancestor's **`capture_action`** (probe test). Backspace at offset 0: **propagates** to the ancestor (the kit does this on purpose). Enter: propagates and emits `InputEvent::PressEnter` only with `submit_on_enter`; otherwise inserts a newline. Tab: consumed (inline indent), only catchable by capture. To implement "Up on the first visual row" the parent must ask the Textarea for the cursor's visual row (`DisplayMap`), a deeper dependency on kit internals. | Direct: our actions, our layout. |
| Popup anchoring at the caret | The kit has completion/hover popovers for the editor mode; for a custom popover the caret rectangle must be read from the state. | `bounds_for_range` we already wrote; same data. |
| Cross-block selection, drag of blocks | Not supported across entities (each Textarea has its own selection). | Same limitation: one live editor; block selection is a separate model ([[block-editor]] §7.5). |
| Undo | Per-Textarea history; useless across blocks. | Per-block snapshot in the spike; real undo is the page-level log in either case. |
| Memory / entities | One `TextareaState` entity (rope, display map, undo) per **visible** block. | One live editor total; unfocused rows are `StyledText`. |
| Performance on a 1,000-block page | Not measured. The state per block is heavier than a `StyledText`; virtualization limits it to the visible rows. | Measured (section 5). |
| API churn exposure | The kit's input API is large and moves (0.6 rename, 0.7 split into `gpui-base`/`gpui-component`). | Only GPUI's text-system and input-handler API, which the `ui` facade isolates. |
| Effort | Lowest for plain editing; the outliner key model needs capture hooks and row queries. | Highest, but about 2,500 lines including tests already exist as a base. |

Limits of this evaluation: no Textarea page was built and no IME was run on it, so "IME is better in A" is an
assumption from the kit's release notes, not a measurement. The fallback stays viable: the probe shows the edge keys
can be intercepted without patching the kit.

## 7. Risks and follow-ups

| # | Item | Why it matters |
|---|---|---|
| F1 | Run [[ime-test-checklist]] on macOS 14+, Windows 11, Linux X11/Wayland (GNOME, KDE) with ibus and fcitx5; record results; file upstream issues reproduced with `examples/input.rs`. | The only open acceptance criterion of ADR-002 (R4: Linux is the most fragile). |
| F2 | Re-run `bitacora --spike-bench` on real GPUs (macOS, Windows, Linux Wayland with a live compositor) and read RSS per OS. | The Linux numbers use software rendering and exclude GPU time and vsync. |
| F3 | Editor epic: keep the pitfalls fixed (4.2), keep marked text out of the undo log, commit or cancel composition before block navigation (case C7), and flush the edit buffer on blur. | IME correctness. |
| F4 | Replace per-edit `ListState::splice(row..row + 1)` invalidation with a measured-height refresh only when the row count of the focused block changes. | Avoids re-measuring a row per keystroke; today it is cheap (edit to paint under 2 ms). |
| F5 | Page view keyed by stable `BlockId` so structural edits splice only the affected rows. | The spike re-measures the tail after split/merge. |
| F6 | Accessibility: expose the focused block as an AccessKit text input; check what the `gpui-pre` snapshot offers. | R7, not covered by the spike. |
| F7 | Selection across blocks, drag and drop, autocomplete popups at `bounds_for_range`, autopair and image/multi-line paste. | Out of spike scope ([[block-editor]] §7.4-7.5). |
| F8 | Replace the spike inline tokenizer with the `bitacora-markdown` inline AST, with source-range spans per run. | The offset map for click-to-caret needs spans from the parser. |
| F9 | Decide the scroll-to-block strategy (`scroll_to` for jumps, reveal for adjacent moves) in the page view. | Section 4.3. |

## 8. Open questions

- Do Wayland compositors deliver `text-input-v3` preedit and cursor rectangles reliably to a GPUI window (GNOME,
  KDE), and does XIM through XWayland behave the same? Checklist rows L3-L5.
- Does GPUI route Enter/Tab/arrows to the IME before our key bindings on every platform (cases C17, C18)? The spike
  defends itself either way, but the observed order decides whether the defence is enough.
- How much does real GPU presentation add to edit-to-paint (macOS Metal, Windows DirectX, Linux Vulkan)?

## 9. Reproduce

```bash
export LIBRARY_PATH=<dir with libxkbcommon-x11.so link>   # only on hosts lacking the -dev package
cargo test -p bitacora-app spike                           # unit and keystroke tests
cargo build --release -p bitacora-app
bitacora --spike-editor                                    # interactive, 1,000 generated blocks
bitacora --spike-editor --spike-page fixtures/graphs/logseq-docs/pages/Changelog.md
# Headless benchmark (X11 + software Vulkan):
env -u WAYLAND_DISPLAY LIBGL_ALWAYS_SOFTWARE=1 VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json \
  xvfb-run -a -s "-screen 0 1280x900x24" bitacora --spike-bench
```
