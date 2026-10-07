---
id: BIT-SP-0009
type: spec
title: "Pando integration: connection, consent and security"
status: backlog
author: mcp
labels: [pando, security, privacy, v2]
created: 2026-10-07T09:08:04Z
updated: 2026-10-07T10:30:07Z
requirements:
  R1:
    status: backlog
  R2:
    status: backlog
    trace:
      code:
        - crates/bitacora-pando/src/credentials.rs#PandoCredentials
        - crates/bitacora-config/src/pando.rs#PandoSettings
      tests:
        - crates/bitacora-pando/src/credentials.rs#trace_logging_never_leaks_the_secret
        - crates/bitacora-config/src/pando.rs#serialized_settings_contain_no_token_field
  R3:
    status: backlog
    trace:
      code:
        - crates/bitacora-config/src/pando.rs#validate_pando_url
        - crates/bitacora-pando/src/service.rs#PandoService
      tests:
        - crates/bitacora-config/src/pando.rs#remote_needs_allow_remote_then_https
        - crates/bitacora-pando/src/service.rs#invalid_or_remote_urls_are_unavailable_without_connecting
  R4:
    status: backlog
  R5:
    status: backlog
    trace:
      code:
        - crates/bitacora-pando/src/service.rs#PandoService
        - crates/bitacora-runtime/src/live.rs#Session
        - xtask/src/deps.rs#check
      tests:
        - xtask/src/deps.rs#pando_in_core_closure_fails
        - crates/bitacora-runtime/tests/pando.rs
  R6:
    status: backlog
  R7:
    status: backlog
---

## Purpose
Define how Bitacora connects to Pando (REST KB API and AG-UI), what it may send, and how credentials and consent are handled.

## Scope
`pando-rs` SDK usage, `bitacora-pando` adapter, settings panel, per-graph consent, tokens, transport restrictions. Plan: [[bitacora-v2-plan]] (D1-D3).

## Requirements

### BIT-SP-0009.R1 — No graph content leaves the machine without consent

The system SHALL NOT send any graph content (block text, page names, properties, selections, prompts built from them) to Pando unless the Pando integration is enabled globally AND the graph has a recorded consent for the feature in use (semantic indexing, agents). Pando integration SHALL be off by default. Revoking consent SHALL stop all transmission immediately and offer to purge the graph's documents from Pando.

#### Scenario: Fresh install
- GIVEN a new installation with a graph opened
- WHEN the user never opens the Pando settings
- THEN no HTTP request is made to any Pando endpoint

#### Scenario: Consent revoked
- GIVEN semantic indexing is enabled for graph G and syncing
- WHEN the user revokes consent for G
- THEN the outbox stops sending, no further request carries G content, and a "Purge G from Pando" action is offered

### BIT-SP-0009.R2 — Pando credentials in the OS keychain, settings machine-local

Pando tokens (REST and AG-UI) SHALL be stored in the OS keychain (or read from environment variables) and SHALL never be logged, serialised to disk in plain text, or shown in `Debug` output. Pando settings SHALL be machine-local and SHALL NOT be written inside the graph folder or committed by git sync (cf. ADR-019).

#### Scenario: Token in logs
- GIVEN a Pando token is configured
- WHEN the app runs with `RUST_LOG=trace` and performs a search
- THEN no log line contains the token value

#### Scenario: Graph folder untouched
- WHEN the user saves Pando settings
- THEN no file changes inside the graph folder

### BIT-SP-0009.R3 — Loopback-only Pando unless remote is explicitly allowed

The system SHALL connect only to Pando URLs whose host is a loopback address unless the user explicitly enables "Allow remote Pando", in which case the settings SHALL warn that content will leave the machine and SHALL require HTTPS for non-loopback hosts.

#### Scenario: Remote URL refused
- GIVEN "Allow remote Pando" is off
- WHEN the user enters `http://10.0.0.5:7000`
- THEN the URL is rejected with a message and no request is made

#### Scenario: Remote over plain HTTP
- GIVEN "Allow remote Pando" is on
- WHEN the user enters `http://pando.example.com`
- THEN the URL is rejected until it uses `https://`

### BIT-SP-0009.R4 — Pando unavailability degrades gracefully

When Pando is not configured, unreachable, times out or rejects the token, every Pando-backed feature SHALL degrade to an explicit "unavailable" state with a reason, without blocking the UI, and all non-AI features (editing, lexical search, sync, MCP) SHALL keep working. The connection status SHALL be visible in settings and in the UI status area.

#### Scenario: Pando stopped
- GIVEN semantic search and chat are enabled and Pando is stopped
- WHEN the user searches and opens the chat panel
- THEN lexical results appear normally with a "semantic unavailable" chip and the chat panel shows "Pando unreachable" with a retry action

### BIT-SP-0009.R5 — Pando access through pando-rs and the bitacora-pando crate

All communication with Pando SHALL go through the `bitacora-pando` crate, which uses the generic `pando-rs` SDK for the REST KB API and the AG-UI client. `bitacora-core` SHALL remain synchronous; Pando I/O SHALL run on the async runtime owned by `bitacora-pando`/`bitacora-runtime`, and only `bitacora-runtime` (and binaries through it) SHALL depend on `bitacora-pando`. New dependencies SHALL pass `cargo deny check`.

#### Scenario: Dependency direction
- WHEN `cargo tree -p bitacora-core` runs
- THEN neither `bitacora-pando`, `pando-rs` nor `tokio` appear

### BIT-SP-0009.R6 — Pando settings panel with per-feature and per-graph switches

Settings SHALL include a Pando section reachable from a top-bar/sidebar button and the settings window, with: enable switch, connection mode (connect to running Pando, or managed if implemented), REST and AG-UI URLs, token entry (keychain), "Test connection" (reports Pando version, agents and model from `/info`), agent profile selection, independent switches for semantic search, chat, journal review and recommendations, and per-graph consent with excluded pages/namespaces/tags and the MCP write grant for the `pando` token. A minimum supported Pando version SHALL be checked.

#### Scenario: Test connection
- GIVEN a valid URL and token
- WHEN the user clicks "Test connection"
- THEN the panel shows the Pando version, available agent profiles and active model

#### Scenario: Too old Pando
- GIVEN `/info` reports a version below the minimum
- THEN features stay disabled and the panel explains the required version

### BIT-SP-0009.R7 — User can inspect everything sent to Pando

The system SHALL keep a local, user-visible activity log of Pando interactions: semantic sync batches (counts and document ids, not content), agent runs (profile, attached context summary, MCP tools called with the `pando` token, approvals and their outcome). The log SHALL be machine-local, size-bounded and clearable.

#### Scenario: Inspect run
- GIVEN the user ran a journal review
- WHEN they open Pando activity
- THEN the entry lists the profile, the date range sent, each MCP tool call and its result status
