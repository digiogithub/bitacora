---
created_at: 2026-10-07T09:20:55.261571963Z
updated_at: 2026-10-07T09:20:55.261571963Z
tags:
    - changes
    - backlog
    - v2
    - plan
---
# 2026-10-07 — v2 backlog generated

Continues [[bitacora-v2-plan]] (decisions D1-D8, open questions). No code changed; backlog and KB only.

## What changed
- Plan: `docs/plans/bitacora-v2-plan.md` (analysis findings from 4 subagents: design system + frameless, Pando semantic indexing, Pando AG-UI agents + SDK decision, graph view).
- gintrack (`docs/.pmngr/**`), all status `backlog`:
  - Milestones: BIT-M-0006 (M5 design system & frameless shell), BIT-M-0007 (M6 Pando platform & semantic search), BIT-M-0008 (M7 AI agents), BIT-M-0009 (M8 graph view & Release 2.0).
  - Specs + requirements: BIT-SP-0008 design system & window chrome (R1-R6), BIT-SP-0009 Pando connection/consent/security (R1-R7), BIT-SP-0010 semantic indexing & hybrid search (R1-R6), BIT-SP-0011 AI agents (R1-R6), BIT-SP-0012 graph view (R1-R6).
  - Epics: BIT-EP-0015 design system foundation, 0016 frameless window, 0017 screen redesign (M5); 0018 pando-rs SDK (Pando repo), 0019 Pando server support (Pando repo), 0020 Pando integration core + settings + consent, 0021 semantic search (M6); 0022 chat panel, 0023 journal review / recommendations / AI writing (M7); 0024 graph view, 0025 release 2.0 (M8).
  - Stories BIT-US-0113..0163 (51) and tasks BIT-T-0377..0487 (111). Stories cite the requirement they implement in Notes.

## Key decisions recorded
- SDK: hybrid — generic `pando-rs` in Pando repo `sdk/rust/` + thin `bitacora-pando` adapter crate (no Go or Rust SDK exists in Pando `sdk/` today; TS/Python/.NET/Java do).
- Semantic: block-level docs `bitacora/<graph_id>/<block-uuid>`, IndexWriter events + SQLite outbox, RRF hybrid, local re-resolution, opt-in per graph.
- Agents read via Bitacora MCP with Read-only `pando` token; writes via `propose_edit` approval → core Op queue.
- Frameless via `WindowDecorations::Client` + custom 52px AppTitleBar; graph layout in-house GPUI-free `bitacora-graph`.

## Verification
All gintrack writes accepted; ids allocated by the tool. Requirements not verified (nothing implemented).
