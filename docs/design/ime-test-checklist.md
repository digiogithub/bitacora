# IME and text-input manual test checklist

> Status: ready to execute (BIT-T-0103) · Inputs: [[gpui-and-gpui-kit]] §1.2 and §3.2 item 7, [[block-editor]] §7.2,
> [[block-editor-spike-report]]. This is the artifact AGENTS.md §8 refers to ("manual IME checklist per OS before
> releases"). Run it against the spike (`bitacora --spike-editor`) now and against the real block editor before each
> release.

## How to run

1. Build or download the app and start the spike: `bitacora --spike-editor` (generated 1,000-block page) or
   `bitacora --spike-editor --spike-page fixtures/graphs/logseq-docs/pages/Changelog.md`.
2. Click a block (it becomes the live editor) or press the arrow keys; the caret appears in the focused block.
3. For every case below, run the steps with the IME listed in the matrix and write the result in the results table
   at the end: `pass`, `fail` (with notes and a screenshot) or `n/a`.
4. Classify every failure as **spike bug (fixed)**, **GPUI upstream (issue link)** or **platform limitation**.
   For upstream failures, reproduce with GPUI's own `examples/input.rs` before filing.

Useful reference while testing: the header line of the spike window shows the focused block, and the log
(`RUST_LOG=bitacora_app=debug`) prints the window GPU.

## Platform and IME matrix

Every combination must be executed at least once (the required cases are marked in the last column).

| # | OS / session | Input method | Languages to type | Required |
|---|---|---|---|---|
| M1 | macOS 14+ | Built-in Japanese (Kana) and Romaji | `nihongo` -> `にほんご` -> `日本語` | all cases |
| M2 | macOS 14+ | Built-in Pinyin (Simplified) | `nihao` -> `你好` | all cases |
| M3 | macOS 14+ | ABC with dead keys (`Option+e`, then `e`) | `é`, `ñ`, `ü` | C9 |
| M4 | macOS 14+ | Emoji and symbols palette (`Ctrl+Cmd+Space`) | `😀` | C10 |
| M5 | macOS 14+ | Dictation (optional) | any sentence | C11 |
| W1 | Windows 11 | Microsoft IME, Japanese | `nihongo` -> `日本語` | all cases |
| W2 | Windows 11 | Microsoft Pinyin | `nihao` -> `你好` | all cases |
| W3 | Windows 11 | US-International keyboard (dead keys) | `é`, `ñ` | C9 |
| W4 | Windows 11 | Emoji panel (`Win+.`) | `😀` | C10 |
| L1 | Linux X11, GNOME | ibus-mozc (Japanese) | `nihongo` -> `日本語` | all cases |
| L2 | Linux X11, GNOME | ibus-libpinyin or ibus-pinyin | `nihao` -> `你好` | all cases |
| L3 | Linux Wayland, GNOME | ibus (text-input-v3 through mutter) | Japanese and Pinyin | all cases |
| L4 | Linux Wayland, KDE Plasma | fcitx5-mozc and fcitx5-pinyin | Japanese and Pinyin | all cases |
| L5 | Linux X11, KDE Plasma | fcitx5 (XIM or GTK/Qt immodule) | Japanese and Pinyin | all cases |
| L6 | Linux, any | Compose key / dead keys (`xkb` `us(intl)`) | `é`, `ñ` | C9 |
| L7 | Linux, any | Emoji picker of the DE (GNOME `Ctrl+.`) | `😀` | C10 |

Record the exact versions of the OS, the IME, the compositor or window system and the display scale.

## Cases

Each case starts in a block with the text `alpha beta gamma` and the caret after `alpha` unless stated otherwise.
"Candidate window" means the IME's candidate or preedit popup.

| ID | Case | Steps | Expected result |
|---|---|---|---|
| C1 | Composition start and commit | Type a reading (for example `nihongo`), press Space to convert, press Enter to commit. | The preedit text is underlined while composing. After commit the text is inserted at the caret, the underline disappears, and no block split happens on the committing Enter. |
| C2 | Composition cancel | Type a reading, press Esc. | The preedit text disappears (or is committed as plain text, depending on the IME) and the block text is consistent. No stray underline remains. |
| C3 | Candidate window position | While composing, look at the candidate window. | It appears next to the caret (below the composing line), not at the window corner or at the block's start. |
| C4 | Candidate window on a wrapped row | Use a block long enough to wrap (resize the window narrower). Put the caret on the second visual row and compose. | The candidate window is anchored to the second row (`bounds_for_range` returns the right row). |
| C5 | Candidate window near the window edge | Resize or move the window so the caret is near the right and bottom edges, and compose. | The IME repositions the candidate window so it stays on screen. The caret rectangle is still correct. |
| C6 | Backspace during composition | Type `nihon`, press Backspace twice. | The preedit shrinks one reading character at a time. The text before the preedit is untouched. When the preedit becomes empty the composition ends. |
| C7 | Composition across block navigation | Start composing, then press Up, Down or click another block. | The composition is committed or cancelled cleanly (the IME decides), the previous block has no leftover marked text, and the new block starts without a composition. |
| C8 | Selection replaced by composition | Select the word `beta` with Shift+Arrow and start composing. | The selection is replaced by the preedit; after commit only the committed text remains. |
| C9 | Dead keys | Type the dead-key sequence for `é` (macOS `Option+e` then `e`; Windows/Linux the intl layout `'` then `e`). Also `ñ` and `ü`. | The accented character is inserted once, with no stray accent and no duplicate. Backspace removes the whole character. |
| C10 | Emoji picker | Open the platform emoji picker and insert `😀`, then type `x` and press Left twice. | The emoji is inserted as one unit. The caret moves over it in one step (grapheme cluster, UTF-16 surrogate pair handled). |
| C11 | Dictation (macOS, optional) | Start dictation and speak a sentence. | Text appears incrementally as marked text and is committed at the end. |
| C12 | CJK and emoji selection by mouse | Put `日本語 😀 abc` in a block. Drag-select across it; double-click a word; triple-click. | The selection rectangles cover whole characters; double-click selects a word, triple-click selects the block text. No selection edge falls inside a surrogate pair. |
| C13 | Caret positions around CJK text | Use Left/Right and Home/End over `日本語 abc`. | The caret moves one character per key press, never lands inside a character, and its x position matches the glyph edges. |
| C14 | Paste during composition | Start composing, then paste (`Ctrl/Cmd+V`) with text in the clipboard. | No crash and no corrupted text. Either the composition is committed first and the paste applied after, or the paste is ignored; document which. |
| C15 | HiDPI and fractional scaling | Run at 100%, 150% and 200% (Windows/Linux) or on a Retina and a non-Retina display (macOS). Check caret, selection and candidate window. | Caret and selection rectangles align with the glyphs at every scale; the candidate window is still at the caret. |
| C16 | Undo around composition | Compose and commit `日本語`, then press `Ctrl/Cmd+Z`. | The whole composition result is removed in one undo step (not one reading character at a time). |
| C17 | Enter during composition | Compose and press Enter. | The IME consumes Enter (commit). The block is not split. A second Enter splits the block. |
| C18 | Tab and arrows during composition | Compose and press Tab, then arrows. | Tab does not indent the block while marked text exists; the arrows move inside the candidate list, not the caret (IME-dependent). |
| C19 | Focus loss during composition | Start composing, then switch to another app and back. | No stuck underline or phantom preedit after returning. |
| C20 | Typing speed | Type fast (a long ASCII paragraph) and mash Backspace/Enter. | No dropped characters, no panic, the caret never lags visibly behind the text. |
| C21 | AI ghost text and IME (BIT-US-0153) | Turn on Settings, Editor, AI ghost text. Type a sentence, pause until the amber suggestion shows, then start composing (`日本語`). | The suggestion disappears at the first preedit character and no request is made while marked text exists. Tab during composition does not accept anything. |
| C22 | Accepting a suggestion next to composed text | After committing composed text, pause for a suggestion and press Tab. | The suggestion is inserted after the committed text as one undo step; `Ctrl/Cmd+Z` removes only the suggestion. |
| C23 | Compose box field and IME | Open the compose box with `Ctrl/Cmd+J` and write the instruction with an IME. | Composition works in the field, Enter commits the preedit instead of sending, a second Enter sends. Esc closes the box and the block keeps its caret. |

## Results table template

Copy one row per (case, platform/IME). Keep screenshots next to the report or in the PR.

| Case | Platform and version | IME and version | Result | Classification | Notes or screenshot |
|---|---|---|---|---|---|
| C1 | | | | | |

## Linux specifics to record

- Session type (`echo $XDG_SESSION_TYPE`), compositor and version.
- Active IME framework and environment (`GTK_IM_MODULE`, `QT_IM_MODULE`, `XMODIFIERS`, `SDL_IM_MODULE`).
- Whether the app runs natively on Wayland or through XWayland (`WAYLAND_DISPLAY` set or unset when launching).
- GPU and Vulkan driver, printed at startup in the log line `gpu device=... software=...`.

## Open questions

- Which of the cases above can be automated with a platform text-services test (macOS accessibility, Windows UI
  Automation, Linux `ibus-daemon` in CI)? Today only the `EntityInputHandler` contract is automated
  (`spike::block_editor::tests`).
- Does GPUI forward Enter/Tab to the IME before running our key bindings on every platform (cases C17 and C18)? The
  spike guards both (Enter commits a marked range, Tab is ignored while marked), but the real behaviour is
  platform-dependent and must be observed.
