---
id: BIT-T-0220
type: task
title: Asset file naming function with Logseq test vectors
status: backlog
parent: BIT-US-0096
milestone: BIT-M-0003
author: mcp
labels: [bitacora-core, assets]
estimate: 2
created: 2026-10-06T14:31:16Z
updated: 2026-10-06T14:31:16Z
---

## Description
`crates/bitacora-core/src/assets/naming.rs`: `fn asset_file_name(original: &str, epoch_ms: u64, index: usize) -> String` per `handler/editor.cljs:1405-1454`: split stem/ext (`extname`; if not a known format use the full multi-dot extension), stem: ` `, `%`, `/` → `_`; append `_<ms>_<index>`; collapse runs of `_`. Plus `fn asset_kind(ext) -> Image|Audio|Video|Pdf|Other` matching Logseq's lists.

## Acceptance Criteria
- Vectors: `Screen Shot 2024.png` → `Screen_Shot_2024_1731580000000_0.png`; `image.png` → `image_1731580000000_0.png`; `report 50%.docx` (index 1) → `report_50_1731580000000_1.docx`; `a__b.tar.gz` → expected per Logseq multi-dot rule (verify against Logseq 0.10.15 and record).
- Property test: output never contains space, `%`, `/` or `__`.

## Notes
BIT-SP-0002.R15. [[01-file-graph-layout]] §5.
