---
id: BIT-T-0330
type: task
title: Typed settings model with change notifications
status: done
priority: high
parent: BIT-US-0107
milestone: BIT-M-0005
author: mcp
labels: [bitacora-config, settings]
estimate: 3
created: 2026-10-06T14:34:02Z
updated: 2026-10-06T21:53:47Z
closed: 2026-10-06T21:53:47Z
---

## Description
`crates/bitacora-config/src/settings.rs`: `AppSettings` (platform config dir, TOML, versioned) with sections `editor`, `search` (`substring`, `max_block_len`, `paranoid_scan`, `remove_accents`), `sync` (remote, interval, auto_commit), `mcp` (enabled, port, writes_enabled, token stored in OS keyring via `keyring` crate when available), `appearance` (theme, font_size, locale), `keymap` overrides; plus a `GraphConfigEdit` API for comment-preserving `config.edn` keys. `SettingsStore` emits `SettingsChanged { keys }`; each key declares `ApplyMode::{Live, ReindexRequired, RestartRequired}`.

## Acceptance Criteria
- Round-trip tests; unknown keys preserved; editing `config.edn` keeps comments and formatting (golden test).
- Changing `search.substring` reports `ReindexRequired`.

## Notes
BIT-SP-0003.R2, BIT-SP-0003.R15. ADR-010.
