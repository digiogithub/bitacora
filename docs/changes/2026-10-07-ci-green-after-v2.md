---
title: CI green on 3 OS after v2
tags: [changes, ci]
---

# CI fixes after the v2 merges

Part of [[bitacora-v2-plan]], story BIT-US-0161.

- `typos.toml`: exclude the Spanish vendored `design/**`, allow `LOD`, `Hel` and quoted Spanish design words.
- `crates/bitacora-pando/tests/managed.rs` `pid_alive`: macOS has no `/proc`; use `ps -o stat= -p` there.
- `crates/bitacora-app/src/fonts.rs` `embedded_families_resolve`: ignored off Linux (macOS platform must be created on the main thread; DirectWrite `all_font_names` omits in-memory fonts).
- Flatpak (release workflow): runtime/SDK 24.08 shipped rustc 1.89 but the workspace needs 1.90; moved to 25.08 with llvm20 (manifest, workflow image, release-process doc). Not verifiable locally.

Verified on Linux: `typos`, fmt, clippy `-D warnings` for bitacora-pando and bitacora-app, managed and fonts tests.
