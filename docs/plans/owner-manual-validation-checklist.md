---
tags: [plan, validation, owner]
---
# Owner manual validation checklist

Sections A to E below are the 1.x list; the 2.0 per-OS verification is the **V2 section at the end of this file** (V1 to V9).

Definitive list of the items still `in_review` after the final backlog closure (2026-10-07). Everything else in BIT is `done`. Each item needs a human, a non-Linux host, a secret or a real GitHub trigger. Linked from [[bitacora-full-development-plan]]. When a line passes: comment on the item, move it to `done`, then close the parent story/epic/milestone (BIT-M-0001..0005) once all children are done.

Already closed by GitHub CI evidence (run 37544944693 green on ubuntu, macos, windows): BIT-US-0027, BIT-T-0011 (cold run 37511221482 test-app windows 749 s / macos 802 s vs warm 187 s / 203 s), BIT-US-0003.

## A. Workflows to trigger (agent token lacks workflow_dispatch / tag push rights)

Run from the repo with `gh` (owner token) or the Actions tab.

| Items | Steps |
|---|---|
| BIT-T-0017 (BIT-US-0012) coverage | `gh workflow run coverage.yml` then `gh run watch`; download the `lcov` artifact, confirm it is non-empty. Also push a deliberately changed insta snapshot on a throwaway branch and confirm CI fails (INSTA_UPDATE=no). |
| BIT-T-0028 (BIT-US-0013) GPUI Kit canary | `gh workflow run gpui-kit-canary.yml` (optionally `-f version=<X.Y.Z>`); confirm build+test on ubuntu/macos and that one issue `GPUI Kit canary: X.Y.Z` is created/updated and nothing is pushed. Check Settings > Dependabot shows `.github/dependabot.yml` as valid. |
| BIT-T-0178 (BIT-US-0090) bundle smoke tests | `gh workflow run bundle.yml`; confirm install/launch/uninstall smoke tests pass for dmg, NSIS exe, msi, deb and **AppImage (never built locally)**; uninstall leaves graph folders untouched. |
| BIT-T-0271 (BIT-US-0112) flatpak build | `gh workflow run flatpak.yml`; confirm `bitacora.flatpak` builds with offline cargo sources. Then for Flathub: PR against `flathub/flathub` with the manifest on the tagged git source, `cargo-sources.json` committed beside it, `appstream-util validate` on the metainfo. |
| BIT-T-0218 (BIT-US-0097), BIT-T-0256 (BIT-US-0110), BIT-T-0243 / T-0242 (BIT-US-0100) release dry run | `cargo xtask bump 0.1.1-beta.0`, commit, `git tag v0.1.1-beta.0`, push the tag. Verify: tag==workspace version check; draft pre-release with 3 bundle sets, 5 CLI archives, flatpak, `SHA256SUMS`, notes from conventional commits; `gh attestation verify <file> -R <owner>/<repo>`; Linux CLI `ldd` guard passes. On a clean box run `packaging/install/install.sh` (and `install.ps1` on Windows), then `bitacora-cli self-update --check` and `self-update --prerelease`. Implement/verify `vpk pack` + `vpk upload github` in the release job so the release carries `releases.<channel>.json`, full/delta `.nupkg`, `RELEASES` (docs/design/auto-update.md). Delete draft + tag afterwards. |
| BIT-T-0242 / BIT-US-0100 real auto-update | Install the Velopack package of v0.1.0; publish v0.1.1 with the feed; launch; confirm the notice, background download and "Restart now"; confirm the new version starts and the graph is intact. Notice-only path: run an AppImage/deb/dev build against a newer release and confirm the Download button opens the release page. |

## B. Secrets (GitHub repo secrets; all optional, signing steps skip when absent; docs/design/release-process.md section 4)

| Items | Steps |
|---|---|
| BIT-T-0176 (BIT-US-0090) macOS notarization | Set `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`. Run the release dry run (A). On a Mac: `spctl -a -vv Bitacora.app` accepted, `xcrun stapler validate Bitacora.dmg`; also for the notarized `bitacora-cli` binary. |
| BIT-T-0177 (BIT-US-0090) Windows Authenticode | Set `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TRUSTED_SIGNING_ENDPOINT`, `AZURE_TRUSTED_SIGNING_ACCOUNT`, `AZURE_TRUSTED_SIGNING_PROFILE`. Run the release dry run; on Windows `signtool verify /pa bitacora.exe Bitacora*.exe Bitacora*.msi`; check SmartScreen behaviour. |
| BIT-T-0115 (BIT-US-0016) token store keychain | No secret needed, but needs macOS and Windows: create a named MCP token in Settings > Agents, confirm it lands in Keychain / Credential Manager (Secret Service on Linux), falls back to the 0600 file when unavailable, and survives restart. |

## C. Manual desktop checks (need macOS, Windows, a Linux desktop with real GPU)

| Items | Steps |
|---|---|
| BIT-US-0014 | Download `build-app.yml` artifacts (`gh workflow run build-app.yml`); launch on macOS, Windows and Linux Wayland-with-output; confirm window opens, first frame renders (`bitacora --smoke-test` exits 0). |
| BIT-T-0050 (BIT-US-0025) | With the app running change OS appearance live (macOS System Settings > Appearance, GNOME/KDE dark toggle); theme follows; choice persists after restart. |
| BIT-US-0060 | `bitacora --spike-bench` on macOS, Windows and a real-GPU Linux machine; record scroll p95 / edit-to-paint p95 / cold start in docs/analysis/rust/block-editor-spike-report.md (Linux lavapipe: ~3 ms / ~2 ms / ~135 ms). |
| BIT-T-0337 (BIT-US-0109) | `bitacora --graph <dir> --perf-bench` on macOS, Windows and real-GPU Linux; target p99 frame time < 16.7 ms; add numbers to docs/design/performance-report-1.0.md. |
| BIT-T-0338 (BIT-US-0109) | Screen readers (VoiceOver, Narrator or NVDA, Orca): open a graph, read the journal feed, move with arrows (row reads text, level, expanded state), fold/unfold, open search palette, open Settings and flip a switch, trigger a dialog and dismiss with Esc (docs/design/accessibility-1.0.md section 4). |
| BIT-US-0072: BIT-T-0104 (backlog), BIT-T-0105, BIT-T-0106 and BIT-US-0031: BIT-T-0165 | Execute docs/design/ime-test-checklist.md: start `bitacora --spike-editor` (and the real editor), run every case for matrix M1-M5 (macOS Japanese, Pinyin, dead keys, emoji, dictation), W1-W4 (Microsoft IME, Pinyin, US-International, Win+.), L1-L5 (ibus/fcitx5 on X11 and Wayland, GNOME/KDE); fill the results table; classify failures (spike bug / GPUI upstream with issue link / platform limitation). Then decide ADR-002 go/no-go (BIT-T-0106), finish the Textarea-per-block fallback prototype if no-go (BIT-T-0105), and run the keymap checklist on 3 OS (BIT-T-0165). |
| BIT-T-0292 (BIT-US-0046) | Git auth matrix from docs/design/git-sync-merge.md section 3.1: HTTPS with macOS Keychain helper, Windows Git Credential Manager, Linux libsecret; SSH with agent and passphrase key; gix-fallback in-app credential prompt (askpass bridge). Record results in the matrix. |
| BIT-T-0272 (BIT-US-0112) | Install the flatpak on GNOME and KDE: file chooser portal opens a graph outside `xdg-documents`, ssh-agent push works, Secret Service stores a token, an MCP client on the host reaches `127.0.0.1:<port>` with the bearer token. |

## D. Logseq 0.10.15 desktop / Clojure oracle (Babashka and Clojure not installed on the build host)

Reference checkout: `../logseq` tag 0.10.15. Read-only oracle; never copy code (ADR-015).

| Items | Steps |
|---|---|
| BIT-US-0081 triple-lowbar codec | In Logseq 0.10.15 (`:file/name-format :triple-lowbar`) create pages with titles from the vectors in `crates/bitacora-core/src/naming.rs` tests (`/`, `.`, `%`, `?`, unicode, trailing dots); compare resulting file names with our encode/decode; fix or document differences. |
| BIT-US-0091 journals | Compare date formatter/parser leniency (numeric widths, `yy` pivot 2000+) for `:journal/page-title-format` and `:journal/file-name-format` variants used in `fixtures/graphs/journals` against Logseq (cljs-time behaviour). |
| BIT-US-0082 rename cascade (BIT-T-0131) | Rename pages in Logseq on a copy of the rename fixtures; capture rewritten files as goldens; diff with our cascade (known decision: our `[[old]]` match is case-insensitive, Logseq's is case-sensitive; decide and record). |
| BIT-T-0301 / BIT-US-0101, BIT-T-0308 / BIT-US-0103 queries | Run the corpus `fixtures/queries/advanced` (43 queries) and the simple DSL cases of `crates/bitacora-index/tests/query_simple.rs` in Logseq on the same graph; compare result sets; document deviations (docs/design/sqlite-index-schema.md 7.4/7.5). |
| BIT-T-0014 / BIT-US-0011 | Open `fixtures/graphs/edge-cases` and `edge-cases-legacy-names` in Logseq 0.10.15 as graphs; confirm no parse errors and nothing is rewritten (`git status` clean in a copy). |
| rewrite-edn goldens (config editor, BIT-US-0056/0071 done, oracle pending) | With Babashka: regenerate rewrite-edn outputs for config editor goldens and compare byte-for-byte. |

## E. Other

| Items | Steps |
|---|---|
| Spec verification stamps | Needs `cargo nextest run --workspace --profile ci --locked`, `python3 tools/junit-paths.py target/nextest/ci/junit.xml .`, `gintrack spec ingest target/nextest/ci/junit.xml --repo .`, `gintrack spec verify <refs> --commit --by <you>` (not run by the closure agent, see Progress in [[bitacora-full-development-plan]]). |
| Release key | Decide on a minisign/ed25519 key for detached signatures of `SHA256SUMS` (docs/design/release-process.md open questions). |
| Windows SmartScreen reputation, Linux CLI musl | Observe after the first public release; musl only if requested. |

---

# V2 (2.0) per-OS verification

Consolidated for BIT-US-0161 / BIT-T-0484 on 2026-10-07. Everything below needs a human, a real display or GPU, a non-Linux host or a live Pando. Agents verified the logic with automated tests on Linux (Xvfb, lavapipe); this section lists what that could not cover. Related: [[frameless-checklist]], [[ime-test-checklist]] (cases C21-C23), [[performance-v2]], [[release-checklist-2.0]], [[bitacora-v2-plan]].

How to use it: copy the matrices into the release issue, run each row on every OS you can, then for every passing item comment on the gintrack item and move it to `done`; close a story when all its children are done. The `in_review` list was taken with `list_items project BIT status in_review` on 2026-10-07 and is reproduced in section V9.

Hosts to cover: **Linux** GNOME Wayland, KDE Wayland (or COSMIC), X11 without compositor, a real-GPU machine; **macOS** (Apple silicon preferred); **Windows 11**. Run once with the Light and once with the Dark theme, and once with the system language set to something other than English.

## V1. Install and first run

| Check | Linux | macOS | Windows |
|---|---|---|---|
| Fresh profile: app starts, the picker (or the reopened last graph) shows, no console errors | | | |
| `bitacora --smoke-test` exits 0 | | | |
| Fonts: headings use the display face, UI text the sans face, code the mono face (embedded design-system fonts, no system fallback) | | | |
| Upgrade from a real 1.x profile (settings, layout, index): settings migrate, layout restores, no data loss (BIT-US-0162, BIT-T-0486, docs/user/upgrade-from-1x.md) | | | |

## V2. Startup and graph menu (BIT-US-0165)

1. Open a graph, quit, start without `--graph`: the last graph reopens without the picker; with a deleted last graph the picker shows and the stale recent is pruned.
2. Graph menu: Open graph..., Open recent (order, empty state), Close graph; keys Ctrl/Cmd+O and Ctrl/Cmd+Shift+W; palette commands `OpenGraph` / `CloseGraph`.
3. **macOS native menu bar**: the Bitacora, Graph, Edit and Window menus appear, items enable and disable correctly (Close graph only with a graph), Open recent updates after opening another graph, Cmd+Q and Cmd+, (Settings) work, and the standard Edit items (Cut, Copy, Paste, Select all) act on the focused block editor.
4. Linux and Windows have no native bar: confirm the sidebar graph switcher and the palette cover the same actions.

## V3. Frameless window and top bar (BIT-US-0119, BIT-US-0120, BIT-US-0121, BIT-US-0122, BIT-US-0128)

Run the 12 checks of [[frameless-checklist]] on every OS and fill its results table. Highlights per platform:

| Check | Linux | macOS | Windows |
|---|---|---|---|
| Drag, double-click maximize/restore, min/max/close, hover colours | | | |
| Resize from 8 edges, minimum size 640x400, cursors | | | |
| Tiling and maximized insets (shadow ring and border disappear on tiled sides) | | n/a | |
| **Windows snap**: hover the maximize button shows snap layouts, Win+arrows snap, dragging the caption to the top edge maximizes | n/a | n/a | |
| macOS traffic lights centred, 78 px inset, full screen removes it | n/a | | n/a |
| Compositor without minimize/maximize (tiling WM) draws close only; server decorations (X11, no compositor) draw no duplicate controls | | n/a | n/a |
| Top bar: back/forward, search field, buttons act like the old toolbar, tabs live in the title bar, interactive children do not start a window drag | | | |
| Narrow window: sidebars collapse at the breakpoints, focus order is sensible, reduce-motion is honoured | | | |

## V4. Visual review of every redesigned screen, Light and Dark

Compare each screen with the mockups in `/www/Bitacora/bitacora-design-system` (open the matching HTML mockup next to the app). Tick Light and Dark separately.

| Screen | Items | Light | Dark |
|---|---|---|---|
| Journals feed: display-type date headers, block bullets, guide lines, edit background, task markers | BIT-US-0124, BIT-T-0399, BIT-T-0400 | | |
| Page view and page header | BIT-US-0124 | | |
| Left sidebar: navigation, calendar (days with notes marked, today highlighted), favorites and recents, status footer | BIT-US-0123, BIT-T-0397, BIT-T-0398 | | |
| Top bar and tabs inside the title bar | BIT-US-0122, BIT-T-0395, BIT-T-0396 | | |
| Tasks view: grouping (overdue, this week, later, no date), filter pills, row actions, overdue badge in the sidebar | BIT-US-0126, BIT-T-0402, BIT-T-0403 | | |
| Command palette, PDF popover, confirmation dialogs | BIT-US-0127, BIT-T-0404 | | |
| Settings window and section navigation | BIT-T-0405 | | |
| Right panel: Context and Agent tabs, related blocks | BIT-US-0145, BIT-T-0448 | | |
| Chat view, thread list, approval cards with diff preview | BIT-US-0147, BIT-US-0149, BIT-T-0454, BIT-T-0458 | | |
| Journal review card, suggestion chips | BIT-US-0151, BIT-US-0152, BIT-T-0462, BIT-T-0465 | | |
| Graph view (global and local), settings panel, export | BIT-US-0157 | | |
| Status bar and Pando status / activity log | BIT-US-0140 | | |
| Theme generation: switching Light/Dark/System live restyles every screen (`cargo xtask tokens --check` is clean) | BIT-US-0116, BIT-T-0383 | | |

Also check: contrast of muted text, focus rings visible on every control, hover and pressed states of the kit components (the Gallery view), and no clipped text at 125% and 150% display scaling (Windows, Linux HiDPI).

## V5. Graph view (BIT-US-0157, BIT-T-0475)

Measured numbers and the node-ceiling recommendation are in [[performance-v2]]; the layout CPU cost is verified on Linux, but painting needs a real GPU.

1. Open a graph with about 3k and about 5k pages: the layout animates smoothly (target 60 fps while it settles; write down the fps you see), then the frame loop stops (Activity Monitor, Task Manager or `top`: idle CPU near 0 % with the graph view visible and settled).
2. Pan, zoom, hover (neighbours highlight), drag a node (layout re-warms, node follows the pointer), click opens the page, focus mode and hop count 1 to 6.
3. Labels show only at the zoom levels in the design; culling keeps the frame time flat when zoomed in.
4. Export: the file dialog opens on each OS and the file is valid.
5. Light and dark colours of nodes, tags, journals and the current page.

## V6. Input and IME (ime-test-checklist, C1-C23)

Run [[ime-test-checklist]] on macOS (M1-M5), Windows (W1-W4) and Linux (L1-L5, ibus and fcitx5, X11 and Wayland). The v2 cases are mandatory on every IME you test:

| Case | Check |
|---|---|
| C21 | AI ghost text: the suggestion disappears at the first preedit character and no request is made while marked text exists; Tab during composition accepts nothing (BIT-US-0153, BIT-T-0467) |
| C22 | Tab after composed text inserts the suggestion as one undo step; Ctrl/Cmd+Z removes only the suggestion |
| C23 | Compose box (Ctrl/Cmd+J): composition works in its field, the first Enter commits the preedit, the second sends; Esc closes it and the block keeps its caret |

## V7. Pando: setup, consent, search, chat, review (live run)

Needs a real Pando (the `pando` binary on PATH for managed mode) and, for chat, a configured model provider. Automated tests use mocks only. Reference: [[pando-integration]], docs/user/pando-setup.md.

| Step | Items | Linux | macOS | Windows |
|---|---|---|---|---|
| Settings > Pando: mode, URLs, token in the OS keychain (never displayed), Test connection reports version and the minimum check | BIT-US-0137, BIT-T-0428 | | | |
| **Managed mode** (default): Bitacora starts its own `pando serve`, the status chip turns connected, Restart works, Open log opens the file, the "shares your KB" hint is right for an absolute vs relative `Data.Directory`; quitting Bitacora stops the child (no orphan after a normal quit or after killing Bitacora) | BIT-US-0141, BIT-T-0437 | | | n/a |
| **Windows**: managed mode shows the "not available in this build, use external mode" state with a working path to external mode (no Job Object yet) | BIT-US-0141 | n/a | n/a | |
| macOS: killing Bitacora with SIGKILL leaves no Pando child (the lifeline watchdog is not wired yet; if one survives, record it as a known gap) | BIT-US-0141 | n/a | | n/a |
| External mode with a separately started `pando serve` | BIT-US-0137 | | | |
| Per-graph consent dialog: nothing is sent before consent; private pages and exclusions (page, namespace, `folder/`, `#tag`) keep content out; revoke offers purge and the KB really empties | BIT-US-0138, BIT-T-0430, BIT-T-0431, BIT-T-0432 | | | |
| Semantic index status, resync and purge in settings; `bitacora-cli semantic status/resync/purge` agree with it | BIT-T-0449, BIT-T-0451, BIT-US-0146 | | | |
| Palette hybrid mode: results mix lexical and semantic, the "unavailable" chip appears when Pando is stopped and search still works; related blocks in the Context tab | BIT-US-0145, BIT-T-0447, BIT-T-0448 | | | |
| Bitacora MCP registered with Pando: a Pando agent can read the graph through the least-privilege token and cannot read excluded pages | BIT-US-0139, BIT-T-0434 | | | |
| Chat: streaming answer, thread list, resume, delete; approval card shows the diff and applying it goes through the core queue and is undoable | BIT-US-0147, BIT-US-0149, BIT-T-0454, BIT-T-0458 | | | |
| Journal review (today and week) card and suggestion chips (accept applies an Op, dismiss is remembered) | BIT-US-0151, BIT-US-0152, BIT-T-0462, BIT-T-0465 | | | |
| Ghost text and compose box with a real model | BIT-US-0153, BIT-T-0467 | | | |
| Pando stopped mid-session: status chip and activity log show the reason, the app keeps working, reconnect is automatic | BIT-US-0140 | | | |

## V8. Performance per OS

Linux numbers are in [[performance-v2]]. On each other host run the same commands and add a column to its tables:

1. `bitacora --graph <dir> --perf-bench` on the benchmark graph (see [[performance-report-1.0]] section 1); target p99 frame time below 16.7 ms.
2. `cargo test -p bitacora-index --release --test bench_v2 -- --ignored --nocapture --test-threads=1` (Tasks view queries and graph data).
3. `cargo test -p bitacora-graph --release --test bench_scale -- --ignored --nocapture --test-threads=1` (layout scaling and idle CPU).
4. `cargo test -p bitacora-pando --release --test bench_v2 -- --ignored --nocapture --test-threads=1` (hybrid search and backlog throughput against the mock KB).
5. On a real GPU: graph view at 3k and 5k nodes while settling (fps), chat while streaming, and idle CPU with the graph view settled and the chat idle (target under 1 %).

## V9. Items currently `in_review` in gintrack (BIT, 2026-10-07)

Close each after the matching section passes on the OSes the item concerns.

| Item | Title | Section |
|---|---|---|
| BIT-T-0389 (BIT-US-0119) | CSD spike branch and frameless-window findings doc | V3 |
| BIT-T-0391, BIT-T-0392, BIT-T-0393, BIT-T-0394 (BIT-US-0120, BIT-US-0121) | AppTitleBar; macOS inset and server-decoration fallback; resize zones and tiling; frameless checklist and ADR-033 | V3 |
| BIT-T-0383 (BIT-US-0116) | Bitacora Dark/Light kit themes from tokens | V4 |
| BIT-T-0395, BIT-T-0396 (BIT-US-0122) | Top bar buttons and search field; tabs in the title bar | V3, V4 |
| BIT-T-0397, BIT-T-0398 (BIT-US-0123) | Sidebar nav, favorites and footer; calendar widget | V4 |
| BIT-T-0399, BIT-T-0400 (BIT-US-0124) | Block bullets, guides, task markers; journal and page headers | V4 |
| BIT-T-0402, BIT-T-0403 (BIT-US-0126) | Tasks view grouping and pills; overdue count query | V4 |
| BIT-T-0404, BIT-T-0405 (BIT-US-0127) | Palette, PDF popover and dialogs; settings window restyle | V4 |
| BIT-T-0406 (BIT-US-0128) | Responsive breakpoints, focus order, reduce-motion | V3 |
| BIT-T-0414 (BIT-US-0131) | pando-rs docs, semver policy and crates.io publication | owner step (outward-facing), not a per-OS check |
| BIT-T-0428 (BIT-US-0137) | Pando settings section | V7 |
| BIT-T-0430, BIT-T-0431, BIT-T-0432 (BIT-US-0138) | Consent, ContentPolicy, revoke and purge | V7 |
| BIT-T-0434 (BIT-US-0139) | Register Bitacora MCP with Pando, end to end | V7 |
| BIT-T-0437 (BIT-US-0141) | Supervise the managed `pando serve` | V7 |
| BIT-US-0140 | Pando connection status, degradation, activity log | V7 |
| BIT-T-0447, BIT-T-0448, BIT-T-0449 (BIT-US-0145) | Palette hybrid mode, related blocks, index status | V7 |
| BIT-T-0451 (BIT-US-0146) | CLI semantic status, resync, purge | V7 |
| BIT-T-0454 (BIT-US-0147), BIT-T-0458 (BIT-US-0149) | Thread list; approval card with diff | V7 |
| BIT-T-0462, BIT-T-0465 (BIT-US-0151, BIT-US-0152) | Review card UI; suggestion chips | V7 |
| BIT-T-0467 (BIT-US-0153) | Ghost text | V6, V7 |
| BIT-T-0475 (BIT-US-0157) | GraphCanvas painting | V5 |
| BIT-T-0486 (BIT-US-0162) | 1.x to 2.0 settings, layout and index migration tests | V1 |
| BIT-US-0165 | Reopen the last graph; graph menu | V2 |
| BIT-T-0487 (BIT-US-0163) | v2 user docs, release notes and v2.0.0 tag | owner step, see [[release-checklist-2.0]] |
| BIT-T-0484, BIT-T-0485 (BIT-US-0161) | This verification and the performance pass | V8 and this file |
