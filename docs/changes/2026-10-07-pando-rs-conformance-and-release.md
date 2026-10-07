---
created_at: 2026-10-07T13:00:00Z
updated_at: 2026-10-07T13:00:00Z
tags:
    - change
    - pando
    - sdk
    - release
---
# pando-rs conformance tests, docs and release readiness

Implements BIT-US-0131 (BIT-T-0413, BIT-T-0414). Continues [[2026-10-07-pando-rs-agui-client]] and [[2026-10-07-pando-rs-kb-client]]; plan: [[bitacora-v2-plan]].

## Conformance
- `crates/pando-rs/tests/fixtures/agui/*.sse`: the three recorded AG-UI streams copied byte for byte from Pando `sdk/typescript/tests/fixtures/agui/` (they are hand-authored to the Go wire format, not live captures; see the fixtures README).
- `tests/conformance.rs`: serves the fixtures verbatim from an axum server and asserts the same things as the TS replay tests: interrupt exposes the pending `get_weather` call, `resume` posts assistant tool call then `tool` result after the last user message, `STATE_DELTA` replace/replace/add reduce to the expected state, no fixture event falls into `Event::Unknown`.
- `tests/live.rs` (all `#[ignore]`): `live_agui_info_and_health_decode`, `live_agui_thread_api_on_a_fresh_project` (spawn `pando agui-serve --no-tls --no-token` on a free port in a temp dir, or use `PANDO_LIVE_AGUI_URL`; no LLM call) and `live_rest_health_and_kb_search` (needs `PANDO_LIVE_REST_URL`, since `pando serve` is TLS-only with a self-signed cert).

## Release readiness
- `Cargo.toml`: homepage, documentation, keywords, categories. `publish` stays inherited `false` on purpose.
- Added `LICENSE` (MIT), `CHANGELOG.md` (0.1.0), README sections: testing and conformance, semver policy, releasing.
- Workspace CI already runs fmt/clippy/test/deny for `-p pando-rs`; no CI change.
- Not published and not pushed. The crate is meant to move to Pando `sdk/rust/`.

## Verification
- `cargo clippy -p pando-rs --all-targets --locked -- -D warnings`: clean.
- `cargo test -p pando-rs --locked`: 8 unit + 10 agui + 4 conformance + 14 kb passed, 3 live ignored.
- `cargo test -p pando-rs --test live -- --ignored live_agui`: 2 passed against the real `pando` v1.2.11.
- `cargo package -p pando-rs --allow-dirty`: packages and builds from the tarball. `cargo publish --dry-run` correctly refuses (`publish = false`).
