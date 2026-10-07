---
created_at: 2026-10-07T09:07:30.689736983Z
updated_at: 2026-10-07T09:07:30.689736983Z
tags:
    - plan
    - v2
    - pando
    - design-system
    - semantic-search
    - agents
    - graph-view
    - frameless
---
# Bitacora v2 implementation plan

Status: backlog generated 2026-10-07 (milestones BIT-M-0006..0009). Continues [[bitacora-full-development-plan]] (v1.0 closed, see [[2026-10-07-backlog-close-in-review]]).

## Scope (owner request, 2026-10-07)

1. Complete new screen design from the design system at `/www/Bitacora/bitacora-design-system` (tokens, fonts, mockups, `rust/theme.rs`).
2. Semantic indexing and search through Pando (`/www/MCP/Pando/pando`), as already done in git-in-track (`/www/git-in-track`).
3. Pando agentic capabilities: journal evaluation, AI recommendations, in-app chat (AG-UI protocol).
4. A settings panel for Pando (like git-in-track's, but covering more uses).
5. Graph view like Logseq's (global + local graph) drawn with GPUI.
6. Frameless window: app-drawn top bar with minimize / maximize / close.

## Milestones

| Milestone | Content |
|---|---|
| M5 — v2 Design system & frameless shell | Tokens/fonts/theme pipeline, component kit, client-side decorations + AppTitleBar, screen redesign (top bar, sidebar, calendar, outline, right panel, tasks view, popovers) |
| M6 — v2 Pando platform & semantic search | `pando-rs` Rust SDK (Pando repo), Pando server gaps, `bitacora-pando` crate, settings panel + consent, semantic indexing and hybrid search |
| M7 — v2 AI agents | Chat panel (AG-UI streaming, tool cards, approvals), journal review, recommendations, AI UI surfaces, privacy/audit |
| M8 — v2 Graph view & Release 2.0 | Graph view (global/local), v2 release hardening, docs, manual per-OS checklist |

## Key findings from analysis

### Design system
- Source of truth `tokens/tokens.json` (W3C format); `tools/gen_tokens.py` generates `tokens.css` and `rust/palette.rs`; `tools/check_contrast.py` enforces WCAG 4.5:1.
- Fonts (SIL OFL): Atkinson Hyperlegible Next (UI/body), Atkinson Hyperlegible Mono (markers, dates, kbd), Literata (journal/page titles). Not embedded in the app today.
- Layout: 52px top bar (sidebar toggle, back/forward, tabs, search ⌘K, theme/PDF/panel buttons), 252px left sidebar (nav, calendar, favorites/recents, graph + MCP footer), reading column max 760px (860 tasks), 360px right panel (Context / Agent tabs).
- Amber = AI only (unaccepted AI content, primary AI actions). Borders not shadows.
- Current app theme (`crates/bitacora-app/src/theme.rs`, `assets/themes/bitacora.json` "Paper") is a different palette; facade rule: only `src/ui/mod.rs` names `gpui_kit` (test `only_ui_names_gpui_kit`).
- `rust/theme.rs` targets upstream gpui; we run `gpui-pre 0.3.8` via `gpui_kit::gpui` (gpui-kit 0.7.1) — port, do not copy blindly.

### Frameless (verified in `gpui-pre-0.3.8` source)
- `WindowOptions.window_decorations: Some(WindowDecorations::Client)` (`platform.rs:2500`), `TitlebarOptions { appears_transparent, traffic_light_position }` (`platform.rs:2645`), `app_owns_titlebar_drag` (`platform.rs:2472`).
- `Styled::window_control_area(WindowControlArea::{Drag,Close,Max,Min})` (`div.rs:1257`) gives native hit-test on Windows (snap layouts).
- `window.start_window_move` / `start_window_resize(edge)` / `zoom_window` / `minimize_window` / `titlebar_double_click` / `window_controls()` / `window_decorations()`.
- gpui-component 0.7.1 ships `TitleBar` (`title_bar.rs`) and `window_border()` (`Root::decorate` already wraps windows). Plan: custom 52px `AppTitleBar` following the kit's pattern.
- Risks: GNOME Wayland has no SSD (must draw controls); X11 without compositor falls back to Server decorations (must not draw duplicate controls); macOS keeps native traffic lights (inset ~78px).

### Pando API (verified in source)
- REST, `X-Pando-Token` header: `POST/DELETE /api/v1/remembrances/kb/documents`, `POST /api/v1/remembrances/kb/reindex`, `POST /api/v1/remembrances/kb/search` (`internal/api/routes.go:155-169`, `handlers_remembrances_search.go:75`). Embeddings server-side. One global KB, `path_prefix` the only namespace.
- AG-UI adapter `internal/agui/` (off by default; `pando agui-serve` or `pando serve --agui-port`): `GET /healthz`, `GET /info`, `POST /{agent}` (SSE, `RunAgentInput`), threads API, `POST /runs/{id}/cancel`. Bearer auth + Origin allow-list (native clients send no Origin). Frontend (client-executed) tools interrupt the run; HITL via synthetic `pando_permission_request` tool call, fail closed. `Profiles` give named restricted agents with tool globs (e.g. `journal-reviewer`, `recommender`, `chat`). Threads persisted in `agui_threads`.
- Pando agents can consume external MCP servers over `streamable-http` with headers, so they can call Bitacora's MCP server. Unverified whether the AG-UI agent loads global `MCPServers` — first Pando-side task.
- SDKs in `sdk/`: TypeScript, Python, .NET, Java (all with AG-UI clients). **No Go SDK and no Rust SDK** exist in `sdk/` (owner believed a Go SDK existed; `pkg/` only has `extension` and `mesnada`). Pando is MIT.
- git-in-track integration: `internal/pando/` (MCP client + TOON decoder + cache_read paging; REST only for reindex), config `search.pando.{mode,mcpUrl,mcpToken,restUrl,restToken,projectId,allowRemote}` (modes auto/managed/external/off, ADR-039 there), tokens from env never serialised, non-loopback refused unless `allowRemote`, Pando hits re-resolved against the local index, offline degrades to lexical.
- Pando gaps for Bitacora: no list-by-prefix, no content-hash skip on upsert, no batch upsert/delete, no delete-by-prefix, upsert writes mirror files (bloat with block-level docs), no per-token prefix scoping, no similar-to-id API, no versioned REST contract.

### Graph view
- Logseq 0.10.15 behaviour (`src/main/frontend/handler/graph.cljs:84-175`, `components/page.cljs:577-770`, `extensions/graph/pixi.cljs:59-100`): nodes = pages, edges = refs ∪ tags ∪ namespaces; filters journals / orphans (default on) / builtin / excluded; size `8*max(1,cbrt(degree))`; forces link-dist 70, charge -600, charge-range 600, collide 26, velocityDecay 0.5; settings stored in config.edn `:graph/settings` and `:graph/forcesettings`; export PNG; local graph = 1-hop.
- GPUI: `canvas()` + `paint_quad` (circles) + batched `PathBuilder::stroke` edges + `request_animation_frame`; no offscreen render (`render_to_image` stub), so export via our own SVG writer (+ optional `tiny-skia` PNG).

## Decisions (become ADR rows when implemented)

- **D1 (ADR-027) SDK placement — hybrid.** Generic async Rust client `pando-rs` lives in the Pando repo at `sdk/rust/` (REST KB API + AG-UI client: typed events incl. Pando extensions, SSE, `/info`, threads, frontend-tool interrupt/resume, HITL helper), next to the other SDKs and versioned with the server. Bitacora gets a thin adapter crate `bitacora-pando` holding product logic (profiles, frontend tools, consent, approval mapping, semantic sync). Until `pando-rs` is on crates.io, Bitacora pins it as a git dependency by rev. Evaluate `ag-ui-core` (0.1, single maintainer) only as a type source.
- **D2 (ADR-028) Crate placement.** `bitacora-pando` depends on `bitacora-config`, `bitacora-core` (types), `bitacora-index` and `pando-rs`; `bitacora-runtime` depends on it. Async (tokio) lives in `bitacora-pando`/runtime; `bitacora-core` stays sync. New edge: `index ← pando ← runtime`.
- **D3 (ADR-029) Pando is opt-in, managed by default, on the shared KB.** Off by default, per-graph consent, loopback-only unless `allowRemote`, token in OS keychain, settings machine-local (never in the graph or git, cf. ADR-019). Default mode **managed** (as git-in-track ADR-039): Bitacora supervises `pando serve` from a per-graph cache instance dir with a generated `.pando.toml` (AG-UI, Bitacora profiles/personas, `MCPServers.bitacora`) merged over the user's global Pando config; modes `external`/`off` remain. Semantic documents live in Pando's **shared KB** (agent memory), so the consent dialog says any Pando agent can see indexed blocks.
- **D3b Pando stays generic.** Bitacora uses only Pando's existing generic APIs; nothing Bitacora-specific is implemented in Pando. Only the generic `pando-rs` SDK is added to the Pando repo. The former "Pando server support" epic (BIT-EP-0019) is cancelled.
- **D4 (ADR-030) Semantic documents are block-level, keyed by block uuid** (`bitacora/<graph_id>/<block-uuid>`), text = page title + breadcrumb + block content; driven by `IndexWriter` events with a machine-local sync ledger + outbox (outside the rebuildable index; source of truth for what exists remotely, so no remote listing, batch, hash-skip or delete-by-prefix endpoints are needed); per-document REST upsert/delete with bounded concurrency; hits re-resolved against the local index; hybrid ranking by RRF with FTS5.
- **D5 (ADR-031) Agents read the graph through Bitacora's MCP server** with a dedicated Read-only `pando` token, plus user-attached AG-UI context. Writes only through the frontend tool `propose_edit` (diff card, applied via the core Op queue, audited, undoable) or HITL-approved MCP writes; unanswered approvals are denied.
- **D6 (ADR-032) Design tokens pipeline.** Vendor `tokens.json` + generator + contrast check; generate palette into `crates/bitacora-app/src/ui/theme/`; embed fonts; bundled "Bitacora Dark/Light" kit themes generated from tokens; the 1.x "Paper" theme is removed. Custom colour schemes from a config file come after 2.0 (BIT-US-0164).
- **D7 (ADR-033) Client-side decorations** with a custom 52px `AppTitleBar`; fall back gracefully when the platform forces Server decorations.
- **D8 (ADR-034) Graph layout in-house** in a new GPUI-free crate `bitacora-graph` (Barnes-Hut force simulation, Logseq-equivalent parameters), data from a new `bitacora-index` read API; settings in config.edn `:graph/settings` / `:graph/forcesettings` via the core Op path.

## Owner decisions (2026-10-07)

1. Managed Pando mode like git-in-track (BIT-US-0141, now high priority; BIT-T-0437, 0488, 0489).
2. Shared KB in the agent memory (consent text updated in BIT-US-0138; managed instance must not override the KB store — spike BIT-T-0489).
3. Semantic search ships with client-side solutions on Pando's current API. Verified: REST upsert writes no mirror file (`internal/api/handlers_remembrances_kb.go`), and KB directory sync only deletes documents with `source_path` metadata (`internal/rag/kb/sync.go`), so REST-only documents are safe. Pando-side items cancelled (BIT-EP-0019, US-0132..0134, T-0415..0422).
4. Journal reviews only in a machine-local cache.
5. Paper theme dropped (BIT-T-0384 cancelled); colour-scheme files later (BIT-US-0164).
6. Reopen last graph at startup + graph menu (BIT-US-0165, T-0490, T-0491).

## Open questions

1. Which OSes get manual frameless verification for 2.0?
2. Graph node ceiling (5k vs 20k with LOD); PNG export via `tiny-skia` or SVG only first.

## Execution status (2026-10-07)

All 49 v2 stories (BIT-M-0006..0009) were implemented by worktree subagents and merged into `main`; BIT-US-0164 (colour-scheme files) stays post-2.0. Final gate on Linux: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (1982 passed, 0 failed), `cargo xtask check-deps`, `cargo xtask tokens --check`.

Deviations: `pando-rs` lives in-workspace at `crates/pando-rs` (ADR-027) because the Pando repo had owner WIP; PNG export via `tiny-skia` (no labels; SVG has labels); recommended graph ceiling 5k nodes (see [[performance-v2]]).

Items left `in_review` need owner validation (visual review, macOS/Windows, real-GPU perf, live Pando with a capable model, crates.io publish, release tag): see [[owner-manual-validation-checklist]] section V and [[release-checklist-2.0]]. Open owner decisions: chat auto-approval of read-only Pando tools, filtering MCP `read_asset`/`config_text` for the `pando` token, the two purge buttons in Pando settings, breakpoints 1170/760px ([[ai-privacy-review]] §5).
