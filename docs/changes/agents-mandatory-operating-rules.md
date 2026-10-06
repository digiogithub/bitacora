---
created_at: 2026-10-06T16:32:18.947593738Z
updated_at: 2026-10-06T16:32:18.947593738Z
tags:
    - change
    - documentation
    - agent-guidance
---
# Merge mandatory agent operating rules into AGENTS.md

## What changed
Updated the root `AGENTS.md` to integrate clearly labelled mandatory rules for context recovery, knowledge-link/code-index use, external research, planning, incremental implementation and verification, KB change records, memory-tool selection, and general agent conduct. Reclassified the existing project safety and architecture constraints as mandatory without removing them. Preserved the Bitacora crate layout, commands, backlog workflow, test expectations, commit guidance, and KB conventions.

## Files and symbols touched
- `AGENTS.md` — instruction sections and headings only; no code symbols changed.

## Why
Make the project's AI-agent rules unambiguously mandatory and align the operating guidance with the canonical checklist while retaining Bitacora-specific engineering constraints.

## Verification
- Re-read the complete final `AGENTS.md` and checked that all checklist areas are explicitly labelled mandatory and existing project guidance remains.
- Reviewed `git diff -- AGENTS.md`.
- `git diff --check` passed.
- No source code changed, so tests/build were not applicable.

Continues [[agents-mandatory-rules-merge-plan]].