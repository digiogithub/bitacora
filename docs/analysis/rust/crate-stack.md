# Bitacora Rust crate stack and workspace layout

> Checked on **2026-10-06** (versions from the crates.io API on that date). Scope: items 4–5 of the Rust stack research. GPUI and GPUI Kit (UI framework, text editing) are covered in [[gpui-and-gpui-kit]].
> Related: [[architecture]], [[block-editor]], [[sqlite-index-schema]], [[git-sync-merge]], [[mcp-server]].

## Summary

- **UI:** `gpui-kit =0.7.1`, which pins `gpui-pre =0.3.8`. It is the only GPUI dependency.
- **Async:** GPUI's executors run the UI. A separate **tokio 1.53** runtime (owned by the app, bridged with a vendored `gpui_tokio`-style helper) runs **axum 0.8.9** and **rmcp 3.5.1** (MCP over streamable HTTP) plus network I/O. `bitacora-core` stays synchronous and executor-agnostic.
- **Storage and index:** **rusqlite 0.40.2** with `bundled` (FTS5 included) is preferred over sqlx 0.9. The index is a derived cache of the Markdown files ([[sqlite-index-schema]]).
- **Parsing:** a custom lossless line-based outline parser in `bitacora-markdown`. Inline syntax is parsed with **pulldown-cmark 0.13.4** (offset iterator), or with a hand-written inline lexer for Logseq extensions. **comrak 0.56** is the fallback when a full AST or GFM rendering is needed.
- **Sync:** **git2 0.21.0** (vendored libgit2) for v1. **gix 0.88** is the future path (pure Rust; high-level push is still maturing). For 3-way merge: **diffy 0.5.2** (`merge`) for text and **imara-diff 0.2** or **similar 3.2** for diff display, plus a block-level merge of our own ([[git-sync-merge]]).
- **Everything else:** `notify 8.2` + `notify-debouncer-full 0.7`, `uuid 1.27` (v4 for Logseq ids, v7 internally), `jiff 0.2.37` (or `chrono 0.4.45`), `edn-rs 0.19` (read) plus a custom lossless writer for `config.edn`, `nucleo-matcher 0.3.1`/`nucleo 0.5`, `directories 6.0`, `tracing 0.1.44`, `serde 1.0.229`, `keyring 4.2`, `velopack 1.2` (auto-update), `rust-i18n 4.2.4` (i18n, same as GPUI Kit).

---

## 4. Crate stack

### 4.1 Version table

| Area | Crate | Version (2026-10-06) | Last release | Choice | Rationale |
|---|---|---|---|---|---|
| UI | `gpui-kit` | 0.7.1 | 2026-10-05 | ✅ | See [[gpui-and-gpui-kit]]. Pins `gpui-pre =0.3.8`. |
| Async | `tokio` | 1.53.2 | 2026-10-03 | ✅ | Needed by axum and rmcp. Runs on its own runtime, not GPUI's. |
| HTTP | `axum` | 0.8.9 | 2026-04-14 | ✅ | rmcp's streamable-HTTP server integrates with axum 0.8 (`rmcp` depends on `axum ^0.8`). |
| MCP | `rmcp` | 3.5.1 | 2026-10-05 | ✅ | Official MCP Rust SDK. Features: `server`, `macros`, `schemars`, `transport-streamable-http-server`, `transport-streamable-http-server-session`, `auth`, `transport-io` (stdio for `bitacora-cli mcp`). |
| JSON schema | `schemars` | 1.x | — | ✅ | Required by rmcp tool macros. |
| File watching | `notify` | 8.2.0 | 2026-08-30 | ✅ | FSEvents, inotify, ReadDirectoryChangesW. Also used by GPUI Kit. |
| | `notify-debouncer-full` | 0.7.0 | 2026-05-02 | ✅ | Coalesces editor or git bursts, tracks renames via file IDs, so you don't have to debounce by hand. |
| SQLite | `rusqlite` | 0.40.2 | 2026-08-08 | ✅ | Features `bundled` (static SQLite with **FTS5** and JSON1; same version on every OS), `functions`, `hooks`, `backup`, `serde_json`. Synchronous calls fit a dedicated index thread. |
| | `sqlx` | 0.9.0 | 2026-05-21 | ❌ | Async-only and needs a runtime. Compile-time checked queries need a DB or offline data. Weaker FTS5 and custom-function ergonomics. No benefit for an embedded, single-writer cache. |
| | `rusqlite_migration` | 2.6.0 | 2026-05-28 | ✅ | Schema migrations via `user_version`. Since the index is rebuildable, a simple approach is enough. |
| | `r2d2_sqlite` / `deadpool-sqlite` / `tokio-rusqlite` | 0.35 / 0.14 / 0.8 | 2026 | optional | Only if the MCP server needs concurrent readers. Prefer one writer thread plus a few read-only connections in WAL mode. |
| Markdown | `pulldown-cmark` | 0.13.4 | 2026-05-20 | ✅ (inline) | Fast pull parser with `into_offset_iter()` source ranges. Has GFM tables, tasks and strikethrough options, and wikilinks support in 0.13 (*verify the exact semantics*). Not lossless, so we never re-serialize through it. |
| | `comrak` | 0.56.0 | 2026-10-06 | fallback | Full CommonMark/GFM AST with `sourcepos`, wikilinks extension and a CommonMark formatter. Heavier. Useful for export and HTML rendering. |
| | `markdown` (markdown-rs) | 1.0.0 | 2025-04-23 | ❌ | Spec-exact mdast, but slower and less flexible for custom syntax. |
| | `pulldown-cmark-to-cmark` | 23.0.0 | 2026-09-29 | optional | Markdown emission for export. Never for round-tripping user files. |
| | `tree-sitter` + `tree-sitter-md` | 0.27.0 / 0.5.3 | 2026 | optional | Incremental highlighting in the raw-source Editor (GPUI Kit feature `tree-sitter-markdown`, if offered). |
| Rope | `ropey` | 1.6.1 (stable) / 2.0.0-beta.1 | 2025-08-02 | conditional | GPUI Kit already uses `ropey =2.0.0-beta.1` internally. A **block editor doesn't need a rope** (a `String` per block). Use a rope only if we build our own page-level editor. |
| | `crop` | 0.4.3 | 2025-04-25 | alternative | B-tree rope with UTF-16 metrics. Fine, but less widely used. |
| Diff/merge | `similar` | 3.2.0 | 2026-08-17 | ✅ (display) | Myers and Patience diffs, inline word diff, unified diff output. Good for the diff viewer. No 3-way merge. |
| | `imara-diff` | 0.2.0 | 2025-06-14 | ✅ (perf) | Fastest histogram diff (used by gitoxide and Helix). Use it for big files and for line-level diffs that feed the merge. |
| | `diffy` | 0.5.2 | 2026-08-31 | ✅ (merge) | `diffy::merge(base, ours, theirs)` gives a 3-way text merge with conflict markers. It is the fallback once block-level merge fails ([[git-sync-merge]]). |
| Git | `git2` | 0.21.0 | 2026-05-18 | ✅ v1 | Mature libgit2 bindings: clone, fetch, push, merge-base, merge_trees, index, credentials callbacks (HTTPS token, SSH agent or key). Features `vendored-libgit2`, `https`, `ssh` (`vendored-openssl` on Linux, or rustls through a custom transport). |
| | `gix` | 0.88.0 | 2026-09-25 | watch | Pure Rust, fast status and diff, blob/tree 3-way merge, native SSH, credential helpers. Push exists in **plumbing** (send-pack), and the high-level API is still maturing. Revisit for v2 or for read-only paths (status, log). |
| | system `git` CLI | — | — | fallback | Optional "use system git" setting (as Logseq uses dugite). Gives the best compatibility with user hooks and credential helpers. |
| IDs | `uuid` | 1.27.0 | 2026-10-02 | ✅ | `v4` for Logseq-compatible `id::` properties. `v7` (time-ordered) for internal row keys if needed. Features `serde`, `v4`, `v7`. |
| Time | `jiff` | 0.2.37 | 2026-09-12 | ✅ (preferred) | Correct time-zone handling (bundled tzdb on Windows), spans, `strftime`. Pre-1.0 but stable in practice (BurntSushi). |
| | `chrono` | 0.4.45 | 2026-06-04 | alternative | The ecosystem default (rusqlite and serde integration everywhere). Pick one and stick with it. Logseq's journal formats (`:journal/page-title-format "MMM do, yyyy"`, date-fns tokens) need **our own token translator** either way. |
| EDN | `edn-rs` | 0.19.0 | 2025-12-26 | ✅ (read) | Parses `config.edn` into a value tree (maps, keywords, sets, vectors, strings). **Not lossless**: comments and formatting are lost. |
| | `edn-format` | 3.3.0 | 2023-10-03 | alternative | Spec-oriented parser and emitter. Stale. |
| | custom lossless EDN CST | — | — | ✅ (write) | Logseq's `config.edn` is heavily commented. Edits from Bitacora (for example toggling a setting) must preserve comments, so build a small CST/token-span editor (about 500 LOC). |
| Fuzzy | `nucleo-matcher` | 0.3.1 | 2024-02-20 | ✅ | Helix's fzf-quality matcher with Unicode normalization. Faster than `fuzzy-matcher`. Stable even though it hasn't been released recently. |
| | `nucleo` | 0.5.0 | 2024-04-02 | ✅ | High-level, multi-threaded, incremental matcher for the command palette and quick switcher over tens of thousands of titles. |
| | `frizbee` | 0.13.0 | 2026-08-13 | watch | SIMD Smith-Waterman (blink.cmp). Fast and typo-tolerant. |
| | `fuzzy-matcher` | 0.3.7 | 2020-10-04 | ❌ | Unmaintained. |
| Paths | `directories` | 6.0.0 | 2025-01-12 | ✅ | `ProjectDirs::from("es", "Digio", "Bitacora")` for config, data and cache directories. (`dirs 7.0.0` is a lower-level alternative.) |
| Logging | `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | 2025-12 / 2026-03 | ✅ | Structured logs plus spans. Rolling file appender (`tracing-appender`) in the cache directory. Also an `env-filter`. |
| Serialization | `serde` / `serde_json` | 1.0.229 / 1.0.151 | 2026-07 | ✅ | App settings (JSON or TOML), MCP payloads, Dock layout persistence. |
| Credentials | `keyring` | 4.2.0 | 2026-08-29 | ✅ | v4 store split. The default `v1` feature gives macOS Keychain, Windows Credential Manager and Secret Service via zbus. Use it for git HTTPS tokens and MCP auth tokens. Linux without Secret Service needs a fallback (encrypted file or ask every time). |
| Auto-update | `velopack` | 1.2.161 | 2026-09-29 | ✅ (evaluate) | Installer plus delta updates on all three OSes, with a GitHub Releases source. Handles `.app` bundles and Windows installs correctly. |
| | `self_update` | 1.3.0 | 2026-09-02 | ❌ for GUI | Replaces a single binary from GitHub Releases. Fine for `bitacora-cli`, but wrong for signed `.app`/MSI installs. |
| | `cargo-packager-updater` | 0.2.3 | 2025-07-21 | alternative | Pairs with cargo-packager. Slow-moving. |
| i18n | `rust-i18n` | 4.2.4 | 2026-09-29 | ✅ | `t!("key")` macro with YAML, JSON or TOML locale files compiled in. Maintained by Longbridge and used by GPUI Kit itself, so app and components share one locale switch. Bitacora ships English, Spanish and French (`crates/bitacora-app/assets/locales/<ns>.<lang>.yml`); the Settings language list is `i18n::LANGUAGES` and the default "Follow the system" uses `sys-locale` (Linux order: LANGUAGE, LC_ALL, LC_MESSAGES, LANG; any `fr*` tag maps to `fr`). |
| | `fluent-bundle` | 0.16.0 | 2025-05-22 | alternative | Better plurals and grammar (Project Fluent), more boilerplate. |
| Errors | `thiserror` / `anyhow` | 2.0.21 / 1.0.104 | 2026 | ✅ | `thiserror` in libraries, `anyhow` in app and CLI binaries. |
| FS helpers | `fs-err`, `tempfile`, `ignore`, `walkdir`, `globset` | 3.3.2, 3.27.0, 0.4.33, 2.5.0, 0.4.20 | 2026 | ✅ | Clear I/O errors. Atomic writes (temp file plus rename). Graph scanning that honours `.gitignore` and Logseq's `:hidden` list. |
| Channels | `async-channel` / `flume` / `crossbeam-channel` | 2.5.0 / 0.12.0 / 0.5.17 | — | ✅ (one of) | Core to UI event streams that work with both GPUI and tokio executors. |
| Misc | `regex` 1.13.1, `parking_lot` 0.12.5, `open` 5.4.4 (open URLs and files), `rfd` 0.17.2 (native file dialogs; check portal support in Flatpak), `image` 0.25.10, `rust-embed` 8.12.0 | | | ✅ | |
| Testing | `insta` 1.49.0, `proptest` 1.11.0, `criterion` 0.8.2 | | | ✅ | Snapshot tests for parser round-trips, property tests (parse then serialize equals identity), benchmarks. |
| Search (alt) | `tantivy` | 0.26.2 | 2026-09-08 | ❌ v1 | FTS5 is enough. Tantivy adds an index format and memory cost. Reconsider only for ranking or fuzzy full-text at large scale. |

### 4.2 Async runtime interplay

```
┌──────────── main thread (GPUI foreground executor) ────────────┐
│  Views/entities (BlockEditor, Sidebar, Dock...)                 │
│  cx.spawn(...) ← polls async-channel of CoreEvents → cx.notify │
└───────────────▲───────────────────────────────┬────────────────┘
                │ events                         │ commands (sync API)
┌───────────────┴──────────────┐   ┌────────────▼────────────────────┐
│ bitacora-core service        │   │ GPUI background executor         │
│ (graph state behind RwLock/  │◄──┤ cx.background_spawn: parse,      │
│  actor thread; sync API)     │   │ index, diff, git ops (blocking)  │
└──────▲───────────────▲───────┘   └──────────────────────────────────┘
       │               │
┌──────┴─────┐  ┌──────┴──────────────────────────────────────────┐
│ index      │  │ tokio runtime (2–4 workers, own thread)         │
│ thread     │  │ axum + rmcp streamable HTTP (127.0.0.1)         │
│ (rusqlite, │  │ → calls core service via spawn_blocking         │
│ 1 writer)  │  │ notify-debouncer callback → core.reload(path)    │
└────────────┘  └─────────────────────────────────────────────────┘
```

- **Never** call `tokio::spawn` from a GPUI task, and never `block_on` on the main thread. Bridge with `Tokio::spawn(cx, fut)`, the vendored copy of Zed's `gpui_tokio` (Apache-2.0; about 80 LOC; not published as `gpui-pre-*`).
- The MCP server must be able to run **headless** (`bitacora-cli mcp --graph ~/notes`) without GPUI, so it depends only on `bitacora-core`, `bitacora-index` and `bitacora-sync`.
- Write conflicts between the UI, MCP and the file watcher are serialized through a single **core command queue** (actor pattern), so every mutation goes through the same op log, the same undo history and the same file writes ([[architecture]]).
- File-watcher echo suppression: the core records the hash of each file it writes, and watcher events whose content hash matches are ignored.

### 4.3 Markdown strategy (summary; details in [[block-editor]])

Logseq Markdown is not CommonMark-first. It is a **line-based outline** (`- ` bullets with tab or 2-space indentation), `key:: value` properties, `id::` UUIDs, `((uuid))` block refs, `[[page]]`/`#tag`/`#[[tag]]`, `TODO`/`DOING`/`NOW`/`LATER`/`DONE` markers, `SCHEDULED:`/`DEADLINE:` lines, `:LOGBOOK:` drawers, `{{macros}}`, and page-level properties in the first block. Therefore:

1. Write a **custom lossless block parser** (each block keeps its raw text span; serializing unmodified blocks is byte-identical). Test it with `proptest` round-trips on real graphs.
2. Use an **inline tokenizer** for the Logseq-specific tokens (refs, tags, markers, macros, properties), layered over **pulldown-cmark** for standard inline Markdown (emphasis, code, links, images), with offsets for UI styling and indexing.
3. Use comrak only for export or HTML features.

## 5. Workspace layout

### 5.1 Crates

```
bitacora/
├─ Cargo.toml                 # [workspace] resolver = "3", edition 2024, workspace.dependencies (exact pins for gpui-kit)
├─ rust-toolchain.toml        # pinned stable (e.g. 1.9x) + clippy, rustfmt
├─ crates/
│  ├─ bitacora-core/          # domain model (Graph, Page, Block, Property, Ref), ops + op log, undo, journals, config model; no UI, no tokio
│  ├─ bitacora-markdown/      # lossless outline parser/serializer, inline tokenizer (pulldown-cmark), Logseq dialect quirks, round-trip tests
│  ├─ bitacora-config/        # config.edn read (edn-rs) + lossless CST edits; app settings (serde)
│  ├─ bitacora-index/         # rusqlite schema, FTS5, backlinks, incremental reindex from core events  → [[sqlite-index-schema]]
│  ├─ bitacora-merge/         # block-aware 3-way merge (ADR-016)
│  ├─ bitacora-sync/          # (see ADR-007: git CLI + gix) commit cadence, fetch/merge/push, block-level 3-way merge (diffy/imara-diff), credentials (keyring) → [[git-sync-merge]]
│  ├─ bitacora-watch/         # notify-debouncer-full wrapper, echo suppression (can live inside core if small)
│  ├─ bitacora-mcp/           # rmcp tools/resources over core+index; axum streamable-HTTP + stdio transports → [[mcp-server]]
│  ├─ bitacora-app/           # GPUI binary: gpui-kit, views, BlockEditor, keymaps, themes, i18n, tokio bridge, packaging metadata
│  └─ bitacora-cli/           # headless binary: `bitacora mcp`, `reindex`, `sync`, `export`, `doctor` (clap)
├─ assets/                    # icons, fonts, default keymap.json, locales/*.yml, themes/*.json
├─ fixtures/graphs/           # sample Logseq graphs for tests (incl. edge cases)
└─ xtask/                     # cargo xtask: bundle, sign, release notes
```

Dependency direction: `markdown` ← `core` ← {`index`, `sync`, `mcp`} ← {`app`, `cli`}. `config` is used by `core`. **Only `bitacora-app` depends on `gpui-kit`**, which keeps 80% of the code testable and buildable without GPU or system UI libraries.

Profiles:

```toml
[profile.dev]
debug = "line-tables-only"      # faster links; GPUI debug info is huge
[profile.dev.package."*"]
opt-level = 1                   # GPUI is sluggish at opt-level 0
[profile.release]
lto = "thin"
codegen-units = 1
strip = "debuginfo"
panic = "unwind"                # keep for crash reporting
```

### 5.2 CI matrix (GitHub Actions)

| Job | Runner | Steps |
|---|---|---|
| `checks` | ubuntu-latest | `cargo fmt --check`, `typos`, `cargo machete`, `cargo deny check` (licenses: flag GPL from accidental Zed crates; advisories) |
| `lint` | macos-latest (as GPUI Kit does; fastest GPUI build) | `cargo clippy --workspace --locked -- -D warnings` |
| `test-core` | ubuntu-latest | `cargo test --locked -p bitacora-core -p bitacora-markdown -p bitacora-index -p bitacora-sync -p bitacora-mcp -p bitacora-cli` (no GUI deps) |
| `test-app` | matrix: `macos-latest` (aarch64), `macos-13` (x86_64, if still needed), `windows-latest`, `ubuntu-24.04` | `cargo test --locked -p bitacora-app` (GPUI `#[gpui::test]` runs headless with the test platform) |
| `bundle` (tags) | same matrix | `cargo packager --release` → dmg (signed and notarized), msi/NSIS (signed), deb + AppImage. Upload to GitHub Releases. Velopack pack for updates. |
| `flatpak` (tags, later) | ubuntu-latest | `flatpak-builder` with generated cargo sources |

Common setup: `actions-rust-lang/setup-rust-toolchain@v1` (with Swatinem cache; save only on `main`), `rui314/setup-mold@v1` on Linux, `CARGO_PROFILE_DEV_DEBUG=0` in CI to keep caches under 10 GB (GPUI Kit's practice).

Linux system packages (Ubuntu 24.04), derived from GPUI Kit's `script/install-linux.sh` and Zed's `script/linux`:

```bash
sudo apt-get update
sudo apt-get install -y \
  gcc g++ clang cmake pkg-config mold \
  libfontconfig-dev libfreetype-dev \
  libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev \
  libvulkan1 mesa-vulkan-drivers \
  libssl-dev libzstd-dev libasound2-dev \
  libdbus-1-dev            # keyring secret-service (zbus is pure Rust; dbus-dev only if using dbus backend)
# Not needed unless we enable the webview: libwebkit2gtk-4.1-dev
# Not needed with vendored libgit2 + bundled sqlite: libgit2-dev, libsqlite3-dev
```

macOS: Xcode (Metal) is preinstalled on runners. Use GPUI's `runtime_shaders` feature (already in GPUI Kit's pin). Windows: MSVC is preinstalled. Signing uses secrets (Apple ID or app-specific password or API key; Azure Trusted Signing).

## Risks

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | GPUI/GPUI Kit churn (see [[gpui-and-gpui-kit]]) | High | Only `bitacora-app` depends on it. Exact pins. Monthly upgrade cadence. |
| R2 | Lossless Logseq Markdown round-trip (users' files are the source of truth) | High | Custom parser plus property tests on real graphs. Never re-serialize untouched blocks. |
| R3 | Git merge correctness and auth UX (SSH keys, HTTPS tokens, 2FA) across OSes with git2 | High | Block-level merge first, `diffy` text merge as fallback, conflict UI. Optional "system git" mode. Integration tests against local bare repos. |
| R4 | Tokio and GPUI executor mix-ups ("no reactor running", main-thread blocking) | Medium | One bridge module. Lint rule or code review. Keep core sync. |
| R5 | Concurrent writes from UI, MCP and watcher | Medium–High | Single command queue in core, echo suppression, atomic writes. |
| R6 | `config.edn` edits destroying user comments | Medium | Lossless CST editor; read-only fallback on parse errors. |
| R7 | Auto-update with signed bundles | Medium | Velopack spike early. Otherwise an "update available" notice plus download link for v1. |
| R8 | Crates with old releases (`nucleo` 2024, `directories` 2025-01, `edn-format` 2023) | Low | Small, stable APIs. Vendoring is feasible. |
| R9 | Keyring unavailability on minimal Linux (no Secret Service) | Low–Medium | Fallback to an encrypted file store or a per-session prompt. |

## Recommendations

1. Lock these versions in `[workspace.dependencies]`:
   - `gpui-kit = "=0.7.1"`, `tokio = "1.53"`, `axum = "0.8.9"`, `rmcp = "3.5"` with `server, macros, schemars, transport-streamable-http-server, transport-io`.
   - `rusqlite = { version = "0.40", features = ["bundled", "functions", "hooks", "backup"] }`, `rusqlite_migration = "2.6"`.
   - `notify = "8.2"`, `notify-debouncer-full = "0.7"`, `pulldown-cmark = "0.13"`, `diffy = "0.5"`, `imara-diff = "0.2"`, `similar = "3.2"`.
   - `git2 = { version = "0.21", features = ["vendored-libgit2", "https", "ssh"] }`.
   - `uuid = { version = "1.27", features = ["v4", "v7", "serde"] }`, `jiff = "0.2"`, `edn-rs = "0.19"`, `nucleo = "0.5"`, `nucleo-matcher = "0.3"`.
   - `directories = "6"`, `tracing = "0.1"`, `tracing-subscriber = "0.3"`, `serde = "1"`, `keyring = "4.2"`, `rust-i18n = "4.2"`, `thiserror = "2"`, `anyhow = "1"`.
2. Make `bitacora-core` synchronous and executor-agnostic, with a single command queue. Run the MCP server on a dedicated tokio runtime, and also ship it in `bitacora-cli` for headless use.
3. Use SQLite (FTS5) as a rebuildable cache, never as the source of truth ([[sqlite-index-schema]]).
4. ~~Use git2 now and track gix.~~ Superseded by ADR-007 in [[architecture]]: hybrid git CLI (network, commits) + gix (reads, merged trees); git2 only as a fallback backend. See [[git-sync-merge]].
5. Package with cargo-packager. Spike Velopack for updates. Use `self_update` only for the CLI.
6. Set up CI from day one with the 3-OS matrix, `cargo deny` license checks (to block GPL Zed crates), and a GPUI-free `test-core` job for fast feedback.

## Open questions

- `jiff` or `chrono`? Which does `rusqlite`/`serde` integration and the Logseq date-format translation favour? Decide in [[architecture]].
- Does pulldown-cmark 0.13's wikilink support conflict with Logseq `[[page]]` semantics (aliases, namespaces `a/b`, `[[page|label]]`, nested `[[a [[b]]]]`)? We may need to disable it and tokenize refs ourselves.
- rustls or OpenSSL for git2 HTTPS on Linux (vendored OpenSSL increases build time; rustls needs a custom smart transport)?
- MCP auth for the local HTTP server: bearer token in keyring, or Origin/localhost checks only? See [[mcp-server]].
- Is Velopack compatible with notarized `.app` plus Gatekeeper and with MSIX/Store distribution if we ever want it?
- Should `bitacora-watch` and `bitacora-config` be separate crates or modules of `bitacora-core` (fewer crates means faster builds)?
- Is i18n with `rust-i18n` enough for Logseq-compatible locale strings (dates in journal titles use the locale)?

## Sources

- crates.io API (`https://crates.io/api/v1/crates/<name>`), queried 2026-10-06, for every version in §4.1
- rmcp features: <https://crates.io/crates/rmcp>, <https://github.com/modelcontextprotocol/rust-sdk>
- gitoxide status: <https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md>
- gpui_tokio: <https://github.com/zed-industries/zed/blob/main/crates/gpui_tokio/src/gpui_tokio.rs>
- GPUI Kit manifest and CI (dependency choices, Linux packages, cache strategy): <https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml>, <https://github.com/longbridge/gpui-kit/blob/main/.github/workflows/ci.yml>, <https://github.com/longbridge/gpui-kit/blob/main/script/install-linux.sh>
- Zed Linux deps: <https://github.com/zed-industries/zed/blob/main/script/linux>
- Logseq git behaviour context: `docs/analysis/logseq/05-git-and-apis.md`
- cargo-packager: <https://github.com/crabnebula-dev/cargo-packager>; velopack: <https://github.com/velopack/velopack>; keyring: <https://github.com/open-source-cooperative/keyring-rs>; rust-i18n: <https://github.com/longbridge/rust-i18n>
