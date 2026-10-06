---
created_at: 2026-10-06T17:59:36.766039156Z
updated_at: 2026-10-06T17:59:36.766039156Z
tags:
    - change
    - ui
    - spike
    - bitacora-app
---
# Block editor spike, 1,000-block page, report (BIT-US-0040, BIT-US-0060, BIT-US-0072)

Continues [[bitacora-full-development-plan]]; see [[block-editor]], [[gpui-and-gpui-kit]], [[block-editor-spike-report]], [[ime-test-checklist]].

## What changed (commits 99fbdc3, docs commit)
- crates/bitacora-app/src/spike/block_editor/{text_ops,buffer,doc,inline,layout,element,editor,frame_marker,bench,textarea_probe,tests}.rs; cli flags --spike-editor/--spike-blocks/--spike-page/--spike-bench; app.rs hook; ui/mod.rs facade modules text_edit and input; workspace dep unicode-segmentation =1.13.3.
- docs: analysis/rust/block-editor-spike-report.md, design/ime-test-checklist.md, links in architecture.md (ADR-002 rationale, section 2), block-editor.md 7.2, gpui-and-gpui-kit.md open question.

## Why
ADR-002 go/no-go before the editor epic. Preliminary go, fallback Textarea per block.

## Verification
cargo fmt --check, clippy --workspace --all-targets --locked -D warnings, cargo test -p bitacora-app (82 pass), xtask check-deps, cargo deny, machete, typos clean. Benchmark under Xvfb+lavapipe (release): scroll CPU p95 ~3 ms, edit-to-paint p95 ~2 ms, cold start ~135 ms, 1,000/2,681/5,000 blocks.

## Open / manual
IME on macOS, Windows, Linux (ibus/fcitx5) and real-GPU benchmark; BIT-US-0072 in_review; BIT-T-0104 pending.
