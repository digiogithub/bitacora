---
id: BIT-T-0314
type: task
title: "Templates: lookup, variable expansion and default journal template"
status: backlog
priority: medium
parent: BIT-US-0105
milestone: BIT-M-0005
author: mcp
labels: [bitacora-core, bitacora-index, editor]
estimate: 3
created: 2026-10-06T14:33:07Z
updated: 2026-10-06T14:33:07Z
---

## Description
`crates/bitacora-core/src/templates.rs`: list templates from the index (`block_property_values` key `template`); `instantiate(template_uuid, ctx) -> Vec<NewBlock>` copying the subtree (excluding the parent when `template-including-parent:: false`), stripping `template::`/`template-including-parent::`, assigning new block identity, expanding `<% today %>`, `<% yesterday %>`, `<% tomorrow %>`, `<% time %>`, `<% current page %>` and natural-language `<% next monday %>` where feasible. Apply `:default-templates {:journals "name"}` to the virtual today's journal in memory only (file created on first edit).

## Acceptance Criteria
- Unit tests for each variable and the including-parent flag; inserting a template is one undo step.
- Viewing today's journal with a default template creates no file.

## Notes
[[04-editor-outliner-operations]] §9 and Requirements 15; [[block-editor]] §9 Later.
