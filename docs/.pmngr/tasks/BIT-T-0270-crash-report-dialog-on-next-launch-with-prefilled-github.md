---
id: BIT-T-0270
type: task
title: Crash report dialog on next launch with prefilled GitHub issue
status: done
priority: medium
parent: BIT-US-0111
milestone: BIT-M-0005
author: mcp
labels: [observability, ui, bitacora-app]
estimate: 2
created: 2026-10-06T14:32:30Z
updated: 2026-10-06T20:18:19Z
closed: 2026-10-06T20:18:19Z
---

## Description
On startup, if new crash reports or an abnormal-termination marker exist, show a GPUI Kit `Dialog`: summary (version, time, first line of message), buttons View (read-only GPUI Kit `Editor` with the JSON), Copy, Open issue (opens `https://github.com/<org>/bitacora/issues/new?template=crash.yml&title=...&body=...` with a truncated body via the `open` crate — the user reviews before submitting), Dismiss (marks report as seen). Add `.github/ISSUE_TEMPLATE/crash.yml`. Setting `diagnostics.prompt_on_crash` (default true).

## Acceptance Criteria
- `#[gpui::test]`: with a fixture report in a temp data dir the dialog appears once; after Dismiss it does not appear again.
- Issue URL stays under 8 KB (body truncated with a note to paste the full report).

## Notes
- [[gpui-and-gpui-kit]] §2.2 (Dialog, Editor). No automatic upload (privacy).
