---
tags:
    - change
    - ci
    - dependencies
    - tests
---
# 2026-11-12 — Green CI, advisories, rename golden graph, visual checks

Owner asked to finish every open point after [[2026-11-11-release-2.4.0]]. Plan: [[2026-11-12-ci-green-and-open-items-plan]]. Released in [[2026-11-12-release-2.4.1]].

## What changed
- **typos** (`typos.toml`): `Apparence` allowed; it is the French string asserted by `french_translations_resolve_through_rust_i18n` in `crates/bitacora-app/src/lib.rs`.
- **Advisories** (`Cargo.toml`, `Cargo.lock`, `deny.toml`): rmcp `~3.9`, rusqlite `0.42` (libsqlite3-sys 0.40), zip `9.5`, gix `0.89`, plus lock updates (tokio 1.54.1, rustls 0.23.46, rustls-webpki 0.103.16, webpki-roots 1.0.12, libz-sys 1.1.33, ctutils 0.4.8, async-executor 1.14.1 and the Dependabot minor/patch group). `deny.toml` ignores, each with its reason: RUSTSEC-2026-0370/0371/0372 (zvariant, zbus_names, zvariant_derive unmaintained, no fixed release, transitive via zbus) and RUSTSEC-2026-0367/0368/0369/0429 (zip 8.x, now only inside velopack 1.2.161; the fix exists only in a velopack 1.3 pre-release, not adopted for the auto-updater).
- **gix 0.89** (`crates/bitacora-sync/src/backend/gix_net.rs` push fast-forward check, `gix_read.rs` `merge_base`): `Repository::merge_base` now returns `Option<Id>`.
- **Flaky workspace tests** (`crates/bitacora-app/src/views/workspace.rs`, test module: `setup`, `SETTLE_TURNS`, `settle`, `wait_until`): the `bitacora-session` thread could wake the foreground executor before the test called `allow_parking()`, which the GPUI test scheduler reports as non-determinism ("Detected activity on thread bitacora-session"); the polling loops (2–6 s) were also too short on loaded machines. Parking is now allowed in `setup` and the loops share one 30 s bound.
- **BIT-T-0131** rename golden graph: `fixtures/graphs/rename` (ours), `fixtures/graphs/rename.expected` (written by Logseq 0.10.15, procedure in its `PROVENANCE.md`), `crates/bitacora-core/tests/rename_golden.rs`, `fixtures/graphs/MANIFEST.sha256`. Divergences are listed in the test with reasons: three choices (favourites renamed in place, `tags::` values follow the rename, `[[old]]` is case-insensitive) and two gaps filed as BIT-T-0517 (page whose title nests the reference is not moved) and BIT-T-0518 (`{{query [[Old]]}}` not rewritten).
- **BIT-T-0514** visual check and the URL-paste check of BIT-T-0516, on the 2.4.0+ build running headless (Xvfb, llvmpipe, isolated profile, input through xdotool):
  - all 16 `<` commands insert the expected markers (`#+BEGIN_X`/`#+END_X`, `EXPORT latex`/`EXPORT ascii`, a fence for Src) and render in dark and light: quote rule, code copy button, callout icon and title per kind, example/export labels, verse, center, hidden comment. No bug found. The Query sample shows the query error box because `sample query` is not a query.
  - pasting a URL over the selected word gives `[docs](https://example.org/docs)` on disk and a link in read mode.
  - Screenshots: `docs/changes/img/2026-11-12-special-blocks-dark.png`, `-light.png`, `2026-11-12-angle-menu.png`, `2026-11-12-paste-url.png`.

## Verification
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok. `typos`: clean. `cargo fmt --all --check`: clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.
- Tests per package (`cargo test -p <crate> --locked`), all passing: markdown, config, merge, core (incl. `rename_golden`), watch, graph, pando-rs, index, sync, mcp, pando, runtime, testkit, cli (18), app 701/701 three times in a row.
- `cargo xtask fixtures verify`: OK (450 files).

## Not done
- Dependabot PR #10 (actions/upload-artifact v7, download-artifact v8) left open: it changes the release workflow and was not exercised.
