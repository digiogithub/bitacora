---
id: BIT-SP-0012
type: spec
title: Graph view
status: backlog
author: mcp
labels: [ui, graph, v2]
created: 2026-10-07T09:08:04Z
updated: 2026-10-07T09:09:54Z
requirements:
  R1:
    status: backlog
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
Define the global and per-page graph visualization, behaviourally compatible with Logseq 0.10.x graph view (`src/main/frontend/handler/graph.cljs`, `components/page.cljs:577-770`; behaviour only, ADR-015).

## Scope
Graph data, layout simulation, rendering and interaction, settings persistence, export. Plan: [[bitacora-v2-plan]] (D8).

## Requirements

### BIT-SP-0012.R1 — Graph built from the index: links, tags, namespaces, embeds

The system SHALL build the graph from the SQLite index, never by reading graph files directly. Nodes SHALL be pages (including referenced pages without a file); edges SHALL be the union of page-to-page block references (links, tags, property values, embeds), `tags::` declarations and namespace parent relations, excluding self-links and pages whose name looks like a UUID or an asset path.

#### Scenario: Placeholder page
- GIVEN page "A" links to `[[B]]` and B has no file
- WHEN the global graph is built
- THEN nodes contain A and B and an edge A–B

#### Scenario: Namespace relation
- GIVEN pages "proj" and "proj/alpha"
- WHEN the graph is built
- THEN an edge proj–proj/alpha exists

### BIT-SP-0012.R2 — Node filters and sizing match Logseq

The graph view SHALL offer toggles for journals, orphan pages, built-in pages and excluded pages (`exclude-from-graph-view:: true`), with orphan pages shown by default. Node radius SHALL be proportional to `8 * max(1, cbrt(degree))`. The current page SHALL be highlighted; tag pages SHALL use a distinct colour from the theme tokens.

#### Scenario: Journals off
- GIVEN journals toggle is off
- WHEN the graph renders
- THEN no journal page appears as a node

#### Scenario: Degree sizing
- GIVEN a page with degree 27
- THEN its size is 24 (8 * cbrt(27))

### BIT-SP-0012.R3 — Force layout runs off the UI thread with Logseq defaults

The layout SHALL be a force simulation with link spring, Barnes-Hut many-body repulsion, collision and weak centering forces, with defaults link distance 70, charge strength -600, charge range 600, collide radius 26 and velocity decay 0.5. The simulation SHALL run off the UI thread, SHALL NOT block rendering, and SHALL stop requesting frames once it has settled. The layout code SHALL NOT depend on gpui.

#### Scenario: Settled graph is idle
- GIVEN the simulation alpha has fallen below its minimum and no interaction happens
- THEN no animation frames are requested

#### Scenario: Large graph
- GIVEN a graph of 5000 nodes
- THEN the simulation sustains at least 60 ticks/s on the reference machine and the UI stays responsive

### BIT-SP-0012.R4 — Pan, zoom, click-to-open and hover highlighting

The graph view SHALL support drag-to-pan on empty space, scroll/pinch zoom anchored at the cursor, dragging nodes (reheating the simulation), click on a node to open its page, and hover to highlight the node, its neighbours and their edges. Labels SHALL appear above a zoom threshold and for hovered nodes.

#### Scenario: Click opens page
- WHEN the user clicks node "Project X" without dragging
- THEN page "Project X" opens in the main view

#### Scenario: Hover
- WHEN the pointer hovers node A
- THEN A, its neighbours and the connecting edges are highlighted and other nodes are dimmed

### BIT-SP-0012.R5 — Graph settings panel persisted in config.edn like Logseq

The graph view SHALL provide a settings panel with sections Nodes (filters, counts, pause simulation), Search (label filter), Forces (link distance, charge strength, charge range, reset) and Export. Boolean filters SHALL be stored in `logseq/config.edn` under `:graph/settings` and force values under `:graph/forcesettings`, using the same keys as Logseq, written through the core Op path with comment-preserving config editing.

#### Scenario: Persist force setting
- WHEN the user sets link distance to 120
- THEN `config.edn` contains `:graph/forcesettings {:link-dist 120 …}` and all other bytes of the file are unchanged

### BIT-SP-0012.R6 — Local graph, incremental refresh and export

The system SHALL provide a local graph for the current page in the right panel (page, pages it references, pages referencing it, its tags, and links among them; journals optional). Both views SHALL update incrementally when the index changes, preserving positions of surviving nodes. The global graph SHALL export as SVG and PNG, written atomically through a save dialog.

#### Scenario: Incremental update
- GIVEN the global graph is settled
- WHEN the user adds a link from A to new page Z
- THEN Z appears near A and other nodes keep their positions within a small tolerance

#### Scenario: SVG export
- WHEN the user exports as SVG
- THEN the file contains one circle per visible node and one line per visible edge
