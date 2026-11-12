---
created_at: 2026-11-12T15:55:51.83917109Z
updated_at: 2026-11-12T15:55:51.83917109Z
tags:
    - plan
    - ci
    - release
---
# Plan: green CI, advisories, last open backlog items, release (2026-11-12)

Owner request after [[2026-11-11-release-2.4.0]]: finish every open point, push, tag and release, then `cargo clean`.

Phases (each verified on its own):
1. typos: `Apparence` (French test string in `crates/bitacora-app/src/lib.rs`) — allow the word in `_typos.toml`. Verify: `typos`.
2. Advisories (`cargo deny check advisories`): lock updates (ctutils, libz-sys, rustls, rustls-webpki, tokio, webpki-roots, webpki-root-certs), pin bumps (rmcp, rusqlite/libsqlite3-sys, zip 9, gix 0.89 and the Dependabot minor/patch group), documented ignores for unmaintained transitive crates without a fix (zbus_names, zvariant). Verify: `cargo deny check`, workspace tests, clippy.
3. Flaky `views::workspace::tests` (test scheduler sees activity from the `bitacora-session` thread; polling timeouts). Verify: repeated runs under load.
4. BIT-T-0131 rename golden fixture with Logseq 0.10.15 as black-box oracle (ADR-015).
5. BIT-T-0514 visual check of special blocks and the URL-paste check, under Xvfb if the app renders there.
6. Release: version bump, changelog, tag, publish; `cargo clean`.

Progress is recorded in the change records linked from here.