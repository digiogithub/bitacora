---
id: BIT-T-0209
type: task
title: Append-only JSONL audit sink with rotation
status: backlog
priority: high
parent: BIT-US-0022
milestone: BIT-M-0003
author: mcp
labels: [bitacora-mcp, audit]
estimate: 2
created: 2026-10-06T14:31:00Z
updated: 2026-10-06T14:31:00Z
---

## Description
`crates/bitacora-mcp/src/audit.rs`: `AuditSink` trait + `JsonlAuditSink` writing `<data_dir>/bitacora/audit/mcp-audit.jsonl` via a background task (bounded channel, `O_APPEND`, fsync every N lines / 1 s). Record: `{ts, token, client{name,version}, tool, args_sha256, summary, blocks[], tx_id?, result:"ok"|code}`; `auth_failed` records without presented token. Rotation at 10 MB keeping 5 files. Wrap every tool handler (rmcp `tool_handler` wrapper or tower layer reading `AuthContext`) to emit one record.

## Acceptance Criteria
- Tests: write call produces a record with tx_id and uuids; read call stores no content; rotation keeps newest entries; sink failure never fails the tool call (logged).

## Notes
Story BIT-US-0022. Implements BIT-SP-0007.R8.
