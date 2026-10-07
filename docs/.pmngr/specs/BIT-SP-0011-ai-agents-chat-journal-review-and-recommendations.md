---
id: BIT-SP-0011
type: spec
title: "AI agents: chat, journal review and recommendations"
status: backlog
author: mcp
labels: [ai, pando, agui, v2]
created: 2026-10-07T09:08:04Z
updated: 2026-10-07T11:03:02Z
requirements:
  R1:
    status: backlog
    trace:
      code:
        - crates/bitacora-mcp/src/exclusion.rs
        - crates/bitacora-mcp/src/tokens.rs#TokenStore::ensure_token
        - crates/bitacora-runtime/src/live.rs#provision_pando_mcp
      tests:
        - crates/bitacora-mcp/tests/pando_token.rs
        - crates/bitacora-runtime/tests/pando.rs
        - crates/bitacora-mcp/src/tokens.rs#ensure_token_is_idempotent_follows_scopes_and_reminted_when_the_secret_is_lost
  R2:
    status: backlog
  R3:
    status: backlog
  R4:
    status: backlog
  R5:
    status: backlog
  R6:
    status: backlog
---

## Purpose
Define the behaviour of Pando agents inside Bitacora over the AG-UI protocol: chat, journal review, recommendations and AI writing surfaces.

## Scope
Agent graph access, approvals and writes, chat panel rendering, journal review, recommendations, AI text surfaces. Plan: [[bitacora-v2-plan]] (D5).

## Requirements

### BIT-SP-0011.R1 — Agents access the graph only via MCP or user-attached context

Pando agents SHALL access graph content only through Bitacora's MCP server, using a dedicated `pando` token that is Read-scoped by default, or through context the user explicitly attached to a run (open page, selection). The app SHALL NOT bulk-send graph content as AG-UI context.

#### Scenario: Default token scope
- GIVEN Pando integration is enabled and the `pando` MCP token is provisioned
- WHEN an agent calls a write tool on the MCP server with that token
- THEN the call is rejected with insufficient scope and audited

#### Scenario: Attached context
- GIVEN the user selects 3 blocks and chooses "Ask agent about selection"
- WHEN the run starts
- THEN `RunAgentInput.context` contains exactly those 3 blocks and nothing else from the graph

### BIT-SP-0011.R2 — Agent writes are approved, queued as Ops, audited and undoable

Every change an agent proposes to the graph SHALL be shown to the user as an approval card with a diff preview and SHALL only be applied after explicit approval, as `Op` transactions through the `bitacora-core` command queue, recorded in the agent audit log and undoable. An approval left unanswered (timeout, panel closed, app quit) SHALL be treated as denied. Page content SHALL be treated as data: no text inside the graph can widen an agent's permissions.

#### Scenario: propose_edit approved
- GIVEN the agent calls frontend tool `propose_edit` with one block change
- WHEN the user clicks Approve
- THEN the change is applied as one undoable transaction and appears in the agent activity log

#### Scenario: Unanswered approval
- GIVEN a `pando_permission_request` is shown
- WHEN its timeout expires
- THEN the tool call is resolved as denied and the graph is unchanged

### BIT-SP-0011.R3 — Chat panel renders streaming text, tool calls and approvals

The chat panel SHALL render AG-UI events incrementally: `TEXT_MESSAGE_*` as streaming text with `[[Page]]` references turned into links, `TOOL_CALL_*` as collapsible cards with name, arguments and result, and permission requests / `propose_edit` as inline approval cards. It SHALL support multiple threads (list, resume, delete) persisted by Pando, and cancelling a running run.

#### Scenario: Streaming
- GIVEN a run is producing `TEXT_MESSAGE_CONTENT` deltas
- THEN each delta appears in the message within one frame without re-rendering earlier messages

#### Scenario: Cancel
- WHEN the user presses Stop during a run
- THEN `POST /runs/{id}/cancel` is sent and the message is marked as cancelled

### BIT-SP-0011.R4 — Journal review summarises a day without writing to the graph

The journal review SHALL, on demand (and optionally on a user-configured schedule), produce for a chosen day or range a structured review: summary, themes, mood indicators, pending tasks (cross-checked with the index task query) and suggested next actions. The review SHALL be stored in a Bitacora-owned cache outside the graph; it SHALL only be written into a page through an approved `propose_edit`.

#### Scenario: Review today
- GIVEN today's journal has 12 blocks including 3 TODO
- WHEN the user clicks "Review today"
- THEN a review card shows a summary, themes and exactly those 3 pending tasks, and no graph file changes

#### Scenario: Save to journal
- WHEN the user clicks "Insert into journal" on a review
- THEN an approval card with the diff is shown before any change

### BIT-SP-0011.R5 — Recommendations are suggestions applied only by user action

The recommender SHALL suggest, for the current page or block: related pages, missing `[[links]]`, tags and next actions. Suggestions SHALL be shown as chips/cards in amber AI style and SHALL change the graph only when the user accepts one, as a normal undoable Op. Dismissed suggestions SHALL not be re-offered for the same content.

#### Scenario: Accept link suggestion
- GIVEN the recommender suggests linking "kubernetes" to `[[Kubernetes]]` in block B
- WHEN the user accepts
- THEN block B is updated in one undoable transaction and the suggestion disappears

### BIT-SP-0011.R6 — AI writing surfaces insert nothing without explicit action

Inline AI ghost text and the "Compose with AI" box (⌘J) SHALL render AI output in amber as unaccepted content and SHALL insert it into a block only on an explicit accept action (Tab/Enter on the accept control or button). Escape or moving focus SHALL discard it. Accepted text SHALL be a normal undoable edit.

#### Scenario: Discard
- GIVEN ghost text is shown after the caret
- WHEN the user presses Escape
- THEN the ghost text disappears and the block content is unchanged
