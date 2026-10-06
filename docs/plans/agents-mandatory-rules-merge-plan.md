---
created_at: 2026-10-06T16:32:14.113402269Z
updated_at: 2026-10-06T16:32:23.945064112Z
tags:
    - plan
    - documentation
    - agent-guidance
---
# Plan: Merge mandatory AI-agent rules into AGENTS.md

## Goal
Integrate the canonical mandatory operating requirements into the existing Bitacora `AGENTS.md` without losing its architecture, safety, build, backlog, test, commit, and documentation guidance.

## Phase 1 — Context and clause audit
- Search the knowledge base for prior AGENTS.md/convention/instruction work and relevant project context; inspect the project-root instruction files and README.
- Map each canonical requirement to the existing guidance: context recovery, external research, planning, verified increments, KB documentation, memory-tool selection, and general conduct.
- Preserve existing Bitacora-specific rules and classify gaps for targeted insertion.
- Status: complete. Searched broadly without tag filters, followed KB links, read the root guide and README, and found no separate root/nested instruction file to reconcile.

## Phase 2 — Merge the guidance
- Edit only the root `AGENTS.md` unless verification shows another instruction file needs reconciliation.
- Add a clearly labelled MANDATORY operating-rules section integrated into the existing numbered guide, using tools actually available to this project and preserving existing wording where adequate.
- Status: complete. Added mandatory context, research, plan, incremental verification, KB documentation, memory-tool, and general conduct guidance; relabelled the existing project invariants mandatory and retained the repository-specific sections.

## Phase 3 — Verify and document
- Re-read the complete final Markdown; confirm every canonical requirement is covered and explicitly labelled, and existing project sections remain intact.
- Check the diff/status and report verification limits accurately.
- Record the change, path(s), reason, and verification in the KB with `kb_add_document`.
- Status: complete. Full document reread and diff review completed; `git diff --check` passed; recorded the change in [[agents-mandatory-operating-rules]].