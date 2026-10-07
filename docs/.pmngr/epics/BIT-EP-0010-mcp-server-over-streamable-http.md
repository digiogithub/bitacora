---
id: BIT-EP-0010
type: epic
title: MCP server over Streamable HTTP
status: done
priority: high
milestone: BIT-M-0002
author: mcp
labels: [mcp, api]
created: 2026-10-06T14:21:13Z
updated: 2026-10-07T08:21:44Z
started: 2026-10-07T00:15:15Z
closed: 2026-10-07T08:21:44Z
---

## Description
`bitacora-mcp`: rmcp + axum Streamable HTTP server on a dedicated tokio runtime, always running while the app (or `bitacora-cli serve`) runs. `127.0.0.1` bind, mandatory bearer token stored outside the graph, Origin check, scopes (read / write / delete), read-only by default, audit log, tools named after the Logseq plugin API, resources `bitacora://page/<name>`, prompts. Writes go through the core command queue (undoable, committed as agent edits).

## Acceptance Criteria
- MCP Inspector / Claude Code can connect over HTTP with the token and call read tools (M1) and write tools (M2).
- Requests without token or with foreign Origin are rejected.
- Every agent write appears in the audit log and in undo history.

## Notes
ADR-010. See [[mcp-server]], [[05-git-and-apis]].
