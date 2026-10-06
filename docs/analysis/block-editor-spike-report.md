---
created_at: 2026-10-06T17:59:28.725466874Z
updated_at: 2026-10-06T17:59:28.725466874Z
tags:
    - analysis
    - spike
    - block-editor
    - adr
---
# Block editor spike report (ADR-002)

Full text (committed): docs/analysis/rust/block-editor-spike-report.md. Checklist: docs/design/ime-test-checklist.md. Related: [[block-editor]], [[gpui-and-gpui-kit]], [[architecture]], [[ime-test-checklist]], [[bitacora-full-development-plan]].

## Verdict
Preliminary GO for ADR-002 (custom block text element on EntityInputHandler), fallback = GPUI Kit Textarea per block. macOS/Windows/Linux IME validation is pending (BIT-US-0072 stays in_review). No new ADR row; ADR-002 rationale links the report.

## Built (crates/bitacora-app/src/spike/block_editor/)
text_ops (UTF-8/16, graphemes), buffer (selection, marked range, snapshot undo), doc (outline ops), inline (tokenizer, click-to-caret offset map, cache), layout (visual row table over WrappedLine), element (BlockTextElement), editor (actions, cross-block model, EntityInputHandler, gpui::list rows), bench + frame_marker (harness), textarea_probe. Run: `bitacora --spike-editor [--spike-blocks N] [--spike-page F]`, `--spike-bench`. 82 app tests pass (19 keystroke-level gpui tests + unit tests + 3 Textarea probe tests).

## Findings
- WrappedLine::position_for_index is ambiguous at soft-wrap boundaries: own row table needed.
- shape_text needs runs covering newline bytes; GPUI caches wrapped layouts per frame.
- GPUI examples/input.rs pitfalls: marked selection must be relative to the new text; selection end must add range.start; surrogate rounding; character_index_for_point assert.
- ListState::scroll_to_reveal_item fails for far unmeasured rows: use scroll_to(ListOffset).
- h_flex centers children vertically (use items_stretch); Element::into_any vs IntoElement::into_any_element; str::floor_char_boundary needs Rust 1.91 (MSRV 1.90).
- IME: Enter commits composition instead of splitting; Tab ignored while marked; composition = one undo step. Real platform behaviour unverified.

## Performance (Linux, release, Xvfb + lavapipe software Vulkan; GPU time and vsync excluded)
1,000 blocks x3: cold start 130-135 ms; RSS 201 -> 215 MiB; scroll CPU/frame mean 2.4-2.5, p95 3.1-3.2 ms (21 rows built/frame); edit-to-paint mean 1.6, p95 2.0 ms; fold 442 rows 0.07-0.11 ms. Changelog.md (2,681 blocks): scroll p95 3.0 ms, edit p95 1.8 ms. 5,000 blocks: scroll p95 3.1 ms, edit p95 1.8 ms, RSS 202 -> 216 MiB (memory flat in page size). Targets (60 fps, <16 ms keystroke, <1 s start) pass with ~5x headroom. Frame interval is 48 ms = Xvfb pacing, not cost. Inline render cache: no measurable gain. Binary 43.6 MB (44.1 MB with bare gpui-kit tree-sitter feature).

## Textarea per block (Option A) probe
Up/Down consumed by Textarea but visible to ancestor capture_action; Backspace at offset 0 propagates; Enter only with submit_on_enter; Tab consumed. Feasible fallback; no full prototype or IME run (desk + probe evaluation).

## Follow-ups
Run IME checklist on 3 OS (F1); rerun bench on real GPUs, per-OS RSS (F2); keep input.rs fixes (F3); stable-id splicing (F5); AccessKit (F6); cross-block selection/drag/autocomplete/paste (F7); parser-provided inline spans (F8); scroll-to-block strategy (F9).
