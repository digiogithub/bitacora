---
created_at: 2026-10-06T16:44:47.325584554Z
updated_at: 2026-10-06T16:44:47.325584554Z
tags:
    - change
    - workspace
    - quality
---
# BIT-US-0001 / BIT-US-0002: workspace skeleton and quality guards

Part of [[bitacora-full-development-plan]]; see [[crate-stack]], [[architecture]], [[gpui-and-gpui-kit]].

## What changed
- Root `Cargo.toml` (resolver 3, edition 2024, MIT, rust-version 1.90, dev/release profiles), `rust-toolchain.toml` (stable 1.98.1 + rustfmt/clippy), `.cargo/config.toml` (xtask alias, commented mold hint).
- `[workspace.dependencies]` with all third-party pins verified to resolve on crates.io (checked with a scratch crate depending on every pin). Documented versions all existed; deviations: `gix` features use `blob-diff` (no `blob-merge` feature exists in 0.88), `notify-debouncer-full = "0.7"` (0.8 is only rc), `jiff = "0.2"` resolves 0.2.38. `git2` not added (ADR-007/020).
- Crates: bitacora-{markdown,config,core,watch,index,merge,sync,mcp,app,cli,testkit} stubs (thiserror Error + smoke test each), cli has clap subcommands serve/reindex/sync/doctor (exit non-zero), app is the only gpui-kit user (bin `bitacora`).
- `xtask check-deps` (`xtask/src/deps.rs`, `check`): gpui only in app, no direct `gpui`, gpui-kit `=` pin, allowed-edge table, no tokio in core closure; 7 unit tests on synthetic graphs.
- Guards: `rustfmt.toml`, `clippy.toml` (unwrap/expect allowed in tests), workspace lints (unwrap/expect/dbg deny), `deny.toml` (MIT-compatible allowlist incl. 0BSD, bzip2-1.0.6, CDLA-Permissive-2.0; bans crates.io `gpui`; 5 unmaintained RUSTSEC ignores, all transitive via gpui), `typos.toml` (excludes fixtures/graphs, Cargo.lock, docs/.pmngr), cargo-machete clean.

## Verification
fmt check, clippy -D warnings, 18 tests, `cargo deny check` (all ok), `cargo xtask check-deps` OK, typos OK, cargo machete OK. Proven: libpijul (GPL-2.0+) fails licenses; `gpui = "0.2"` fails bans; `unwrap()` in non-test code fails clippy.

## Host note
Linux linking needs `libxkbcommon-x11-dev` (dev symlink missing on this host; worked around with LIBRARY_PATH).
