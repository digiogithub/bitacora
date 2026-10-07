---
id: BIT-T-0106
type: task
title: Write the block-editor spike report and record the ADR-002 decision
status: done
priority: critical
parent: BIT-US-0072
milestone: BIT-M-0001
author: mcp
labels: [spike, docs, adr]
estimate: 2
created: 2026-10-06T14:29:45Z
updated: 2026-10-07T08:21:12Z
started: 2026-10-06T17:33:08Z
closed: 2026-10-07T08:21:12Z
---

## Description
Write `docs/design/block-editor-spike.md` with sections: Summary + verdict (go / no-go / go-with-fallback), Setup (versions: gpui-kit 0.7.1 / gpui-pre 0.3.8, OS/hardware), Findings (IME per platform, wrapping, caret/hit testing, selection, clipboard, unfocused rendering + click-to-caret, performance numbers from the harness, binary size, cold start), Textarea comparison, Upstream issues, Risks and follow-ups for BIT-EP-0007, Open questions. Link it from [[block-editor]] §7 and [[architecture]] §2. If the verdict deviates from ADR-002, add a new ADR row in [[architecture]] §5 superseding it; otherwise mark ADR-002 as "validated by spike" in its rationale column. Update the "Open questions" of [[gpui-and-gpui-kit]] that the spike resolved (binary size, cold start).

## Acceptance Criteria
- Report merged and linked; ADR log updated according to the verdict.
- Follow-up items for the editor epic listed with enough detail to become stories.

## Notes
- Epic acceptance criterion. AGENTS.md §1 (ADR log wins; add rows, never silently diverge) and §8.
