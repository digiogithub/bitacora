---
tags: [plan, validation, owner]
---
# Owner manual validation checklist

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
