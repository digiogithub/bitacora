# GPUI and GPUI Kit for Bitacora

> Checked on **2026-10-06**. Sources: crates.io API, docs.rs, `github.com/zed-industries/zed` (main), `github.com/longbridge/gpui-kit` (main), `gpui-kit.com`.
> Scope: items 1–3 of the Rust stack research (GPUI fundamentals, GPUI Kit components and gaps, text-editing strategy). The rest of the crate stack and the workspace layout are in [[crate-stack]].
> Related: [[architecture]], [[block-editor]], [[sqlite-index-schema]], [[git-sync-merge]], [[mcp-server]].

## Summary

- **GPUI** is Zed's GPU-accelerated, hybrid immediate/retained-mode UI framework (Apache-2.0). It is pre-1.0 and breaks its API often. The last **official** crates.io release is `gpui 0.2.2` (2025-10-22), so it is about a year old and far behind Zed `main`. Since then the framework has been split into `gpui` plus `gpui_platform` / `gpui_macos` / `gpui_windows` / `gpui_linux` / `gpui_wgpu` / `gpui_web`. The Linux renderer moved from **blade to wgpu** (Zed PR #46758, Feb 2026). AccessKit accessibility has landed.
- **gpui-kit.com is the same project as Longbridge's `gpui-component`, renamed.** In v0.6.0 (2026-09-03) the repo became `longbridge/gpui-kit`, and the work was split into layers: `gpui-kit` (the umbrella crate), `gpui-base` (unstyled behaviour), `gpui-component` (styled components, which keeps the old crate name), `gpui-kit-assets` (Lucide icons) and the optional `gpui-shell` (JS runtime). Current version **0.7.1** (2026-10-05). License **Apache-2.0** (docs are CC BY 4.0). About 16.2k stars. It is used in production by Longbridge Pro.
- **How GPUI Kit depends on GPUI:** it does not use a git dependency on Zed or the stale `gpui 0.2.2`. It pins **exact** versions of the weekly **`gpui-pre`** snapshot crates (`gpui-pre =0.3.8`, a snapshot of `zed@279fe07`), which Jason Lee (huacnlee, Longbridge) publishes to crates.io. These crates are renamed back to `gpui` / `gpui_platform` through Cargo `package =` aliases, and `gpui-kit` re-exports GPUI. **Bitacora should depend only on `gpui-kit = "=0.7.1"` and use `gpui_kit::gpui::*`.** It should never add its own `gpui` dependency.
- GPUI Kit covers most of Bitacora's chrome: Sidebar, Tree, VirtualList, Dock, Tabs, Resizable, Dialog, Sheet, Notification, Popover, Command palette, Menu, DatePicker, Calendar, DataTable, Settings, Theme, TextView (Markdown) and the Input / Textarea / Editor family, including IME support and atomic inline tokens.
- **The core of the product we must build ourselves is the [[block-editor]]**: an outline of independently editable blocks with bullets, folding, inline rendering of `[[refs]]`, `((block refs))`, `#tags` and properties, multi-block selection, drag and drop of blocks, and a block-level undo history. We should build it on GPUI's `EntityInputHandler`/`ElementInputHandler`, a rope or `String` per block, and `StyledText`/`TextRun` shaping, borrowing ideas (not code) from GPUI Kit's Input.

---

## 1. GPUI fundamentals

### 1.1 Programming model

| Concept | What it is | Notes for Bitacora |
|---|---|---|
| `Application` / `App` | `gpui_platform::application().run(\|cx: &mut App\| { ... })` starts the platform event loop. `App` is the root context that owns every entity, global, keymap and window. | With GPUI Kit, use its "one window entry point" (v0.7.0), which also creates the Root/overlay host that dialogs and notifications need. |
| `Entity<T>` / `WeakEntity<T>` | Ref-counted handles to state owned by `App`. Create with `cx.new(\|cx\| T::new(cx))`. Read with `entity.read(cx)`. Mutate with `entity.update(cx, \|this, cx\| ...)`. | Model the graph store, page views, each block editor and the sidebar as entities. Use weak handles in long-lived callbacks to avoid cycles. |
| `Context<T>` | Context passed when updating entity `T`. Provides `cx.notify()` (re-render), `cx.emit(event)` (with `EventEmitter<E>`), `cx.subscribe`, `cx.observe`, `cx.spawn`, `cx.listener(...)`. | Domain events such as `BlockChanged`, `PageRenamed` and `IndexUpdated` should flow through `emit`/`subscribe`. |
| `Window` | Per-window state, passed separately (`&mut Window`) to render and handlers. Handles focus, input handlers, text system, bounds and appearance. | Multi-window (for example a page in a new window) is supported. |
| Views / `Render` | `impl Render for T { fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement }`. Entities that implement `Render` are views. `RenderOnce` is for stateless components such as GPUI Kit's `Button::new()`. | The element tree is rebuilt each frame for dirty views, Tailwind-like builders (`div().flex().gap_2()`), and Taffy (flexbox/grid) layout. |
| Elements | Low-level `Element` trait (`request_layout` / `prepaint` / `paint`) for custom drawing: text, cursors, selections, quads, paths, SVG, images. | The block-text element of the [[block-editor]] will be a custom `Element`, like `examples/input.rs`. |
| Lists | `uniform_list` (fixed row height, virtualized) and `list` (`ListState`, variable heights, virtualized). | A long page with thousands of blocks needs `list` with variable heights. |
| Actions | `actions!(ns, [Save, Indent])` or `#[derive(Action)]` with data. Dispatched along the focus path. Handled with `.on_action(cx.listener(Self::save))`. | Every command (indent, outdent, move up/down, toggle TODO, search) is an action, so it is also reusable from the command palette and [[mcp-server]]-triggered UI. |
| Keybindings | `cx.bind_keys([KeyBinding::new("cmd-s", Save, Some("Editor"))])`. Elements declare `.key_context("Editor")`. Supports multi-stroke sequences and context predicates. | Ship a default keymap that mirrors Logseq (`tab`, `shift-tab`, `alt-shift-up/down`, `cmd-enter`), plus a user keymap file. |
| Focus | `FocusHandle` (`cx.focus_handle()`), `.track_focus(&handle)`, `window.focus(&handle)`, the `Focusable` trait, `on_focus` / `on_blur` subscriptions, `tab_stop` / `tab_index`. | Moving focus between blocks maps directly onto focus handles, one per block editor or one shared handle plus a cursor model (see §3). |
| Globals | `cx.set_global(T)` / `cx.global::<T>()` for app-wide singletons. | Settings, theme, the tokio handle and the graph registry. |
| Async | `cx.spawn(async move \|this, cx\| ...)` runs on the **foreground** (main-thread) executor. `cx.background_spawn(fut)` runs on a thread pool. Both return `Task<T>`, and **dropping a Task cancels it** (call `.detach()` to keep it running). Timers come from `cx.background_executor().timer(dur)`. | GPUI's executor is not tokio. See §1.4 and [[crate-stack]]. |
| Testing | `#[gpui::test]`, `TestAppContext`, `VisualTestContext`, simulated keystrokes, a deterministic executor. GPUI Kit adds UI integration-testing helpers. | Essential for testing [[block-editor]] keystroke sequences headlessly. |
| Accessibility | AccessKit integration (`examples/a11y.rs`: roles, labels and actions on elements). | New, so coverage in GPUI Kit components needs to be verified. |

Example list on Zed `main`, `crates/gpui/examples/`: `input.rs`, `text.rs`, `text_layout.rs`, `text_wrapper.rs`, `uniform_list.rs`, `list_example.rs`, `drag_drop.rs`, `tree.rs`, `popover.rs`, `focus_visible.rs`, `tab_stop.rs`, `set_menus.rs`, `system_notifications.rs`, `window*.rs`, `a11y.rs`, `testing.rs`, `data_table.rs`. Source: <https://github.com/zed-industries/zed/tree/main/crates/gpui/examples>.

### 1.2 Text input and IME

- GPUI exposes the platform text-input protocol through the **`EntityInputHandler`** trait. Its methods are `text_for_range`, `selected_text_range`, `marked_text_range`, `unmark_text`, `replace_text_in_range`, `replace_and_mark_text_in_range` and `bounds_for_range`. Offsets are in **UTF-16**, so the app converts to and from UTF-8. A view registers the handler during paint with `window.handle_input(&focus_handle, ElementInputHandler::new(bounds, entity), cx)`. Source: `crates/gpui/examples/input.rs`.
- **macOS:** `NSTextInputClient`, which is mature because Zed relies on it for CJK input, dead keys, the emoji/character palette and dictation.
- **Windows:** IME composition is supported (Zed's Windows stable build shipped in Oct 2025).
- **Linux:** Wayland `text-input-v3` and X11 XIM are supported. In practice this is the most fragile platform (fcitx5/ibus differences, compositor quirks). *Not verified per compositor; needs an explicit test matrix.*
- GPUI Kit v0.7.1 release notes mention "improved IME editing, inline-token interactions", which suggests the component layer is still being hardened.

### 1.3 Theming

- GPUI itself has no theme system. It provides colours (`Hsla`/`Rgba`), styles and `window.appearance()` (light or dark from the OS).
- GPUI Kit provides `Theme` / `ThemeColor` / `ThemeRegistry` with semantic tokens and many bundled themes (Default Light/Dark, Aurora, Ayu, Catppuccin variants, Everforest, Flexoki, Gruvbox, macOS Classic, Tokyo Night, ...). It loads JSON theme files and can follow the system appearance.
- **Do not reuse Zed's `theme` / `ui` crates.** Most of Zed outside `gpui` (and a few utility crates) is GPL-3.0/AGPL. Only `gpui` and the crates it depends on are Apache-2.0. Check the license of every crate copied from the Zed repo.

### 1.4 Async executors

- GPUI ships its own executors: a foreground executor (the main/UI thread) and a background thread pool. They are *not* tokio, so tokio-dependent futures (axum, reqwest-with-tokio, `rmcp`, `tokio-rusqlite`) panic with "no reactor running" if they run on them.
- Zed solves this with the small `gpui_tokio` crate (`crates/gpui_tokio`). It creates a 2-worker tokio runtime (or accepts a `Handle`), stores it as a GPUI global, and `Tokio::spawn(cx, fut)` returns a GPUI `Task` that aborts the tokio task when dropped. It is **not published** as a `gpui-pre-*` crate. It is about 80 lines (Apache-2.0, same license as gpui), so we can vendor or re-implement it in `bitacora-app`.
- Recommended pattern for Bitacora:
  - A dedicated tokio runtime (owned outside GPUI) runs the [[mcp-server]] (axum + `rmcp`) and any network I/O.
  - Domain operations go through a `bitacora-core` service API that is **sync or runtime-agnostic**.
  - UI to service calls use `cx.background_spawn` for CPU or blocking SQLite work, and `Tokio::spawn` for tokio-bound work.
  - Service to UI notifications use an `async-channel`/`flume` receiver polled by a `cx.spawn` foreground task, which updates entities and calls `cx.notify()`.

### 1.5 Platform support and GPU backends

| Platform | Windowing | Text | GPU backend | Status (2026-10) |
|---|---|---|---|---|
| macOS (11+; Apple Silicon and Intel) | AppKit (`gpui_macos`, `gpui_apple`) | CoreText | **Metal** | Most mature (Zed's primary platform). Needs Xcode plus command-line tools; `runtime_shaders` avoids the need for the Metal compiler at build time. |
| Windows 10/11 | Win32 (`gpui_windows`) | DirectWrite | **Direct3D 11** (DirectX renderer) | Stable since Zed for Windows GA (Oct 2025). No extra features needed. MSVC toolchain. |
| Linux X11 | `x11rb` + xkbcommon (`gpui_linux`) | cosmic-text / font-kit (fontconfig) | **wgpu** (Vulkan; wgpu 29 in Zed `main`) | Stable. Feature flag `x11`. |
| Linux Wayland | `wayland-client` + protocols (`gpui_linux`) | same | **wgpu** | Stable. Feature flag `wayland`. Client-side decorations; `layer_shell` example. |
| FreeBSD | as Linux | | wgpu | Community. |
| Web (WASM) | `gpui_web` | | WebGPU | Experimental; GPUI Kit has a `story-web` crate. |
| iOS / Android | GPUI Kit v0.6.2 claims mobile support | | | Experimental; out of scope. |

- **Blade to wgpu (Linux):** "gpui: Remove blade, reimplement linux renderer with wgpu (#46758)", Feb 2026. It fixed freezes on NVIDIA and Smithay-based compositors and used about 20% less CPU. Sources: <https://git.secluded.site/zed/commit/af8ea0d6c26192c45f44f473c0d4a7d6f72ed018>, <https://news.hada.io/topic?id=26669>.
- wgpu on Linux needs a working **Vulkan** driver (`libvulkan1` plus a Mesa or vendor ICD). Very old GPUs and VMs without Vulkan (or with only llvmpipe) are a risk. Whether wgpu's GL fallback is enabled in gpui is *unverified*.
- `gpui-fast` (0.1.x, Longbridge) is an optional alternative "pre" backend with rendering and layout optimizations, selectable through the GPUI Kit `gpui-fast` feature. It is very new, so avoid it initially.

### 1.6 How to depend on GPUI

| Option | Crate / source | Pros | Cons |
|---|---|---|---|
| A. Official crates.io `gpui` | `gpui 0.2.2` (2025-10-22) | Official | About a year stale. Predates the platform split, the wgpu renderer and AccessKit. Incompatible with current GPUI Kit. |
| B. Git dependency on Zed | `gpui = { git = "https://github.com/zed-industries/zed", rev = "..." }` plus `gpui_platform` | Latest fixes | Pulls Zed's whole workspace into resolution, with heavy fetch and a `[patch]` dance. Cannot publish to crates.io. We would have to pick a rev that GPUI Kit also compiles against. |
| **C. Through GPUI Kit's pinned snapshot (recommended)** | `gpui-kit = "=0.7.1"`, which pins `gpui-pre =0.3.8`, `gpui-pre-platform =0.3.8` and `gpui-pre-web =0.3.8` (snapshot of `zed@279fe07`) | One dependency, a coherent tested set, normal crates.io resolution, roughly weekly snapshot cadence | Depends on a third party (Longbridge/huacnlee) to keep publishing snapshots. Exact pins mean upgrading GPUI Kit is an explicit, possibly breaking, step. |

GPUI Kit's workspace manifest explains the exact pin: "any snapshot may change GPUI's API ... a caret requirement let the weekly release move applications onto a newer snapshot that gpui-component did not compile with (#3156)". Its CI enforces exact pins (`script/check-gpui-pin.ts`). Source: <https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml>.

Recommended `Cargo.toml` (workspace):

```toml
[workspace.dependencies]
gpui-kit = { version = "=0.7.1", default-features = true }   # re-exports gpui, gpui-base, gpui-component, assets
# Never add `gpui = ...` directly; use `gpui_kit::gpui`.
```

If we need a GPUI type that GPUI Kit does not re-export (for example `gpui_platform` features), alias the same snapshot crate with the same exact pin:

```toml
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.8", features = ["font-kit", "x11", "wayland", "runtime_shaders"] }
```

### 1.7 Build requirements

| OS | Requirements |
|---|---|
| All | Latest stable Rust (GPUI Kit uses **edition 2024**; GPUI follows Zed's toolchain, so expect "latest stable"). `cmake`/`clang` for some native dependencies. |
| macOS | Xcode plus command-line tools (Metal). Use the `runtime_shaders` feature (on by default in GPUI Kit's pin) so the Metal shader compiler is not needed at build time. |
| Windows | MSVC Build Tools plus Windows SDK. Nothing else (GPUI Kit's CI skips its bootstrap script on Windows). |
| Linux (Ubuntu 24.04) | GPUI Kit's `script/install-linux.sh` installs: `gcc g++ clang libfontconfig-dev libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev libasound2-dev libvulkan1 vulkan-validationlayers` (plus `libwebkit2gtk-4.1-dev`, needed only for its webview crate). Zed's `script/linux` additionally lists `libglib2.0-dev libgit2-dev libsqlite3-dev libva-dev lld mold` (most are Zed-specific). Use `mold` or `lld` for linking speed. |

### 1.8 Packaging

| Tool | Version | Formats | Notes |
|---|---|---|---|
| `cargo-packager` (CrabNebula) | 0.11.8 (2025-11) | macOS `.app` and `.dmg`; Windows NSIS and WiX `.msi`; Linux `.deb`, AppImage, pacman | Best single tool for all three OSes. Its updater (`cargo-packager-updater` 0.2.3) is slow-moving. |
| `cargo-bundle` | 0.12.0 (2026-09) | `.app`, `.deb`, `.rpm`, `.msi` (experimental) | Simpler, weaker on Windows. |
| `cargo-dist` | 0.32.0 | Archives, shell/PowerShell installers, MSI, Homebrew | CLI-oriented (good for `bitacora-cli`), not `.app`/`.dmg`. |
| `velopack` | 1.2.161 (2026-09) | Installer plus **delta auto-update** on Windows, macOS and Linux | Strong candidate for auto-update; see [[crate-stack]]. |
| Flatpak | `flatpak-builder` plus `flatpak-cargo-generator.py` | Flathub | Zed is on Flathub, which proves GPUI works in the sandbox. Needs portals (file chooser, open URI). |
| Zed's own scripts | `script/bundle-mac`, `bundle-linux`, `bundle-windows.ps1` (Inno Setup) | dmg, tar.gz, exe | A good reference for codesigning, notarization and the `.desktop` file. |

Codesigning and notarization (macOS: Developer ID plus `notarytool`; Windows: Authenticode or Azure Trusted Signing) are needed to avoid Gatekeeper and SmartScreen warnings. This is a release-engineering task, not a crate choice.

### 1.9 Apps built with GPUI outside Zed

From <https://github.com/zed-industries/awesome-gpui> (stars as of 2026-10): Longbridge Pro (trading, closed source; GPUI Kit's origin), **Navop** (DB/SSH workspace, 1.8k), **Waku** (coding-agent desktop, 1.6k), **tty7** (terminal, 1.2k), **GitComet** (git GUI, 0.9k), **Arbor** (agentic workflows, 0.8k), **zedis** (Redis GUI, 2.1k), **OpenLogi** (22.9k), **disktree** (2.4k), **Sonora** and **hummingbird** (music), **nohrs** (file explorer), Loungy (launcher, archived). Libraries: GPUI Kit/gpui-component (16.2k), GPUIX (React bindings), Ely GPUI Component, Base GPUI. None of them is a block outliner, so Bitacora's [[block-editor]] would be novel in this ecosystem.

### 1.10 Known limitations

- Pre-1.0 with frequent breaking changes. The official crates.io release is stale, and the ecosystem depends on third-party snapshots.
- Documentation is thin: rustdoc plus examples plus reading Zed's source. The real reference is Zed's `editor` crate, which is GPL, so read it for ideas only.
- Text: there is no built-in rich-text or contenteditable equivalent. Each multi-line editor is custom. Bidi/RTL support is limited.
- Linux IME and HiDPI fractional scaling vary by compositor. Vulkan is needed on Linux.
- Accessibility (AccessKit) is new, and component coverage is unknown.
- Binaries are large (roughly 30–60 MB release builds before stripping or LTO) and full builds are long. Use `mold`, sccache and a split debug-info profile.

---

## 2. GPUI Kit (formerly gpui-component)

### 2.1 Identity

| Field | Value |
|---|---|
| Website | <https://gpui-kit.com> (docs at `/docs`, components at `/component`, plus `/base` and `/shell`) |
| Repo | <https://github.com/longbridge/gpui-kit> (old URL `longbridge/gpui-component` redirects). About 16.2k stars, about 1k forks. |
| Crates | `gpui-kit` 0.7.1 (umbrella, re-exports GPUI), `gpui-component` 0.7.1 (styled UI; same crate name since 2024), `gpui-base` 0.7.1 (unstyled behaviour, state, drag and drop, dock model), `gpui-kit-assets` 0.7.1 (Lucide icons), `gpui-shell` 0.1.0 (QuickJS-based JS extension runtime), `gpui-fps`, `gpui-wry` (webview) |
| Relation | Same project. Renamed GPUI Component to **GPUI Kit** in **v0.6.0 (2026-09-03)** with the layered split. Earlier versions: 0.4.2 (Nov 2025), 0.5.1 (Feb 2026), 0.6.6 (Sep 2026), 0.7.0 (2026-09-28), 0.7.1 (2026-10-05). |
| License | Apache-2.0 (code); CC BY 4.0 (docs prose and illustrations) |
| GPUI dependency | Exact pin on `gpui-pre =0.3.8` / `gpui-pre-platform =0.3.8` / `gpui-pre-web =0.3.8` (crates.io snapshots of Zed `main`) |
| Notable internals | Text model on `ropey =2.0.0-beta.1` (metrics: chars, LF lines, UTF-16). `syntect` and optional `tree-sitter-<lang>` features. `lsp-types 0.97`. `rust-i18n 4.2` for its own strings. `notify 8.2`. |
| Docs | <https://docs.rs/gpui-kit>, the `story` gallery (`cargo run` in the repo), and examples: `hello_world`, `input`, `editor`, `dock`, `markdown`, `stream-markdown`, `html`, `large-text`, `sidebar`, `text_selection`, `focus_trap`, `dialog_overlay`, `webview`, ... |
| Maturity | Ships in a commercial app (Longbridge Pro). Very active (weekly releases), but API churn is high: 0.6 renamed and restructured everything, and 0.7 changed the window entry point. Treat it as pre-1.0. |
| CI | macOS (aarch64), Ubuntu and Windows test matrix; clippy `-D warnings`; exact-pin check |

### 2.2 Component inventory and fit for Bitacora

There are 83 components according to <https://gpui-kit.com/component>. Fit: ✅ use as is, 🟡 use with customization, ⬜ not needed now.

| Category | Component | Bitacora use | Fit |
|---|---|---|---|
| Layout | **Root** | Required root view (overlay host for dialogs and notifications) | ✅ |
| Layout | **Sidebar** | Left sidebar: journals, all pages, favorites, recent, graph switcher | ✅ |
| Layout | **Resizable** | Main/right-sidebar split | ✅ |
| Advanced | **Dock** (DockArea, panels, tab groups, splits, JSON persistence, zoom) | Workspace with main page, right sidebar (Logseq "shift-click opens in sidebar"), backlinks panel, graph view | ✅ |
| Advanced | **Tabs** | Open pages as tabs (optional UX) | ✅ |
| Layout | **Dialog**, **Sheet** | Confirmations, rename page, git conflict resolution, settings | ✅ |
| Layout | **Notification** | Toasts: "Synced", "Merge conflict", "Index rebuilt" | ✅ |
| Layout | **Popover** | Anchored popups for autocomplete and ref previews on hover | 🟡 (needs anchoring to a text range, via `bounds_for_range`) |
| Layout | **Scrollable**, **StatusBar**, **Toolbar**, **GroupBox**, **DescriptionList** | Status bar (sync state, word count), toolbars, page properties | ✅ |
| Advanced | **Command** | Command palette (`cmd-k` search, `cmd-shift-p` actions) | ✅ (feed it with `nucleo` results) |
| Advanced | **List**, **VirtualList** | Search results, all-pages list, backlinks list | ✅ |
| Advanced | **Tree** | File/namespace tree (`a/b/c` namespaces), table of contents | 🟡 (no drag and drop documented; not suitable for the block outline itself) |
| Advanced | **DataTable** | "All pages" table, query results table (Logseq `{{query}}` table view) | ✅ |
| Advanced | **Calendar**; Form: **DatePicker**, **TimeField** | Journal navigation, `SCHEDULED:`/`DEADLINE:` pickers, `/date` command | ✅ |
| Advanced | **Menu** (context menus, app menu) | Block context menu, bullet right-click | ✅ |
| Advanced | **Settings** | Preferences UI backed by `config.edn` plus app settings | 🟡 |
| Advanced | **Chart** | Optional (stats, graph metrics) | ⬜ |
| Form | **Input** (single line, masks, events, inline tokens, paste hook) | Search fields, rename, property values | ✅ |
| Form | **Textarea** (multi-line, auto-grow, inline atomic tokens) | Simple multi-line fields; a possible stop-gap block editor for an MVP | 🟡 |
| Form | **Editor** (code editor: tree-sitter, multi-cursor, folding, search, decorations, LSP hooks, handles 200K lines) | Raw-Markdown "source" view of a page, code blocks, `config.edn` and `custom.css` editing, conflict file editing | ✅ |
| Form | **Select**, **Combobox**, **Checkbox**, **Switch**, **Radio**, **Slider**, **NumberInput**, **ColorPicker**, **Form** | Settings, query builder, property editors | ✅ |
| Basic | **TextView** (Markdown/HTML rendering, selection, link click hook, images, range highlights, `MarkdownPlugin` for custom syntax) | Read-only rendering: hover previews of a ref, search result snippets, embedded page previews, help/changelog | 🟡 (a plugin is needed for `[[ ]]`/`(( ))`/`#tag`; not an editing surface) |
| Basic | **Icon** (Lucide via `gpui-kit-assets`) | Bullets, toolbar and sidebar icons | ✅ |
| Basic | Button, DropdownButton, Badge, Tag, Kbd, Tooltip, Label, Avatar, Accordion, Collapsible, Alert, Progress, Spinner, Skeleton, Pagination, Toggle | General chrome. Tag for `#tags`, Kbd for shortcuts help, Collapsible for the references section. | ✅ |
| Basic | Message, MessageScroller, Bubble, Attachment, Questionnaire, Speech, Rating, Marker, Stepper, Carousel, OtpInput | Chat/AI-oriented components; possibly for a future AI side panel | ⬜ |
| Layout | **Theme** (registry, light/dark/system switching, many bundled themes) | Theme switching, user themes; mapping Logseq `custom.css` is not possible | ✅ |
| Extra | `gpui-shell` (JS runtime), `gpui-wry` (webview) | Possible future plugin system or embedded web content (YouTube embeds, PDFs) | ⬜ (evaluate later) |

### 2.3 Gaps we must build

| Gap | Why GPUI Kit doesn't cover it | Approach |
|---|---|---|
| **Block outline editor** ([[block-editor]]) | Textarea and Editor are single-document text controls. We need N blocks, each with its own text, bullets, indentation guides, collapse arrows and children, keyboard navigation *across* blocks (up/down at the first or last line moves to the next block), Enter to split and Backspace at the start to merge, Tab/Shift-Tab to re-parent. | Custom `BlockEditor` entity plus a custom `BlockTextElement` built on `EntityInputHandler` (see §3), hosted in a virtualized GPUI `list`. |
| **Inline rendering of refs while editing** | Logseq shows raw Markdown in the focused block and rendered output in the others. Rendered output must show `[[Page]]` as a link chip, `((uuid))` resolved to the referenced block text, `#tag`, `TODO`/`DONE` markers, properties `key:: value` and inline images. | Unfocused blocks: build a `StyledText`/`InteractiveText` with `TextRun`s and highlights from the parsed inline AST (or a TextView with a `MarkdownPlugin`). Focused block: raw text with syntax-colouring runs. Measure the performance of a page with 5k blocks. |
| **Bullets, drag and drop of blocks** | GPUI has `on_drag` / `on_drop` / `drag_over` primitives (`examples/drag_drop.rs`) and GPUI Kit's base layer has a DnD model for Dock, but there is no outline DnD. | Custom: draggable bullet, drop indicators (before, after, as child), auto-scroll, multi-block selection drag. |
| **Multi-block selection** | Selection inside a single text control only | A block-range selection model in `BlockEditor` (shift-click, shift-up/down, Esc to select the block) plus copy as Markdown. |
| **Autocomplete popups anchored at the caret** (`[[`, `((`, `/`, `#`, `<`) | Popover exists but must be anchored to the caret rectangle | Use `bounds_for_range` from our input handler, Popover plus List, with the `nucleo` matcher over the [[sqlite-index-schema]] titles/FTS. |
| **Block refs and embeds** (`{{embed [[page]]}}`, `{{embed ((uuid))}}`) | — | Recursive nested `BlockEditor` instances with cycle guard. |
| **Linked and unlinked references panel** | — | List or Collapsible groups fed from the index. |
| **Diff and merge viewer** ([[git-sync-merge]]) | Editor has decorations (range fills), but there is no side-by-side or 3-way diff view | Two or three `Editor` instances, read-only, with range decorations from `similar`/`imara-diff` hunks, synced scrolling, plus "take ours/theirs/both" actions per hunk or block. |
| **Graph view** | Chart is not a force-directed graph | Custom `canvas`/paint element with a simple force layout (later). |
| **Query blocks** (`{{query ...}}`, advanced queries) | — | Render the results with DataTable or List, fed by `bitacora-index`. |
| **PDF viewer and annotations** | — | Out of scope for v1 (possibly via `gpui-wry` webview). |

---

## 3. Text-editing strategy in GPUI

### 3.1 Options

| Option | Description | Pros | Cons | Verdict |
|---|---|---|---|---|
| A. GPUI Kit **Textarea** per block | One `TextareaState` per visible block, with inline tokens for refs | Fastest to build. IME, selection, undo, clipboard and soft wrap come for free. Atomic inline tokens fit `[[ref]]` chips. | Limited control over inline styling (bold, italic, inline code, TODO markers), cross-block cursor movement, and how keys like Up/Down/Enter/Backspace at the edges propagate. One entity per block costs memory. API churn. | MVP / prototype only |
| B. GPUI Kit **Editor** for the whole page | Treat the page as a single Markdown document | Robust, handles large files, has decorations | It is a code editor, not an outliner: no bullets, no per-block collapse or drag, and the UX diverges from Logseq | Use only for the "raw source" view and conflict editing |
| **C. Custom block-text element on `EntityInputHandler`** | Fork the pattern of `gpui/examples/input.rs` (Apache-2.0) into a multi-line, wrapping, styled `BlockTextElement`. One `BlockEditor` entity owns the focused block's buffer, cursor and selection. Unfocused blocks render as `StyledText`. | Full control of the outliner UX, inline styling, IME, caret-anchored popups, and the ability to move across blocks. Only one live editor at a time keeps it cheap. | Most work: wrapping, hit testing, selection painting, IME, undo, accessibility. | **Recommended** for v1, built incrementally (start from input.rs; borrow designs from GPUI Kit's Input, which is Apache-2.0, so code reuse with attribution is allowed) |
| D. Embed a web editor (`gpui-wry`) | ProseMirror/CodeMirror in a webview | Rich editing for free | Defeats the purpose of a native GPUI app: two UI stacks, focus and IME problems, heavy | Rejected |

### 3.2 Recommended design (summary; details in [[block-editor]])

1. **Model:** `bitacora-core` holds the outline tree. Each block has an id (UUID v7 internally; the `id::` property is kept for refs), `content: String` (raw Markdown, the source of truth, preserved byte for byte), properties, children and a collapsed flag. Blocks are short (usually under 1 KB), so a **`String` per block is enough**. A rope (`ropey 1.6.1` stable, or `2.0.0-beta.1` as GPUI Kit uses; `crop 0.4.3` is an alternative B-tree rope with UTF-16 metrics) is only needed for the page-level source view, which GPUI Kit's Editor already handles.
2. **Focused block:** a `BlockEditor` entity with `text: String`, `selection: Range<usize>` (UTF-8), `reversed`, `marked_range` (IME) and `goal_x` for vertical movement. It implements `EntityInputHandler` with UTF-8 to UTF-16 conversion helpers, and registers `ElementInputHandler` in `paint`.
3. **Layout:** `window.text_system().shape_text(...)` or `WrappedLine` (soft wrap at the container width) with `TextRun`s for syntax colouring: dimmed `[[`/`]]` brackets, bold, code, the `TODO` keyword, and an underline for IME marked text. Hit testing uses `index_for_position` / `position_for_index` on the wrapped lines.
4. **Unfocused blocks:** parse the inline AST (from `bitacora-markdown`) into a `StyledText::new(display_text).with_highlights(...)` or `with_runs`. Wrap it in `InteractiveText` with `on_click(ranges, ...)` so clicking a `[[ref]]` navigates and clicking elsewhere focuses the block at the matching caret offset. Map display offsets to source offsets so a click puts the caret at the right raw-text position. Cache the shaped output per (block id, content hash, width).
5. **Keyboard:** actions bound in the `"BlockEditor"` key context: `Newline` (split), `Indent`/`Outdent`, `MoveUp`/`MoveDown` (move to the adjacent block at the edges, keeping `goal_x`), `MoveBlockUp`/`MoveBlockDown`, `ToggleTodo`, `SelectBlock` (Esc), `DeleteBackward` at offset 0 (merge with the previous block), `Undo`/`Redo`, `Copy`/`Cut`/`Paste` (multi-line paste turns into blocks).
6. **Undo:** a **page-level transaction log** of structural ops (insert, delete, move, set-content with text diff) in `bitacora-core`, so undo crosses block boundaries and matches what is written to disk. Typing is coalesced by time or word boundary. Per-input undo stacks (as in Textarea) do not work for an outliner.
7. **IME:** follow input.rs exactly for `replace_and_mark_text_in_range` / `marked_text_range` / `unmark_text`. Return correct `bounds_for_range` so candidate windows are positioned correctly. Test with macOS Japanese/Pinyin, Windows Microsoft IME, and Linux fcitx5 + ibus on X11 and Wayland.
8. **Autocomplete:** on typing `[[`, `((`, `/` or `#`, open a Popover anchored at `bounds_for_range(caret)`. Results come from the [[sqlite-index-schema]] (FTS5 prefix query) re-ranked with `nucleo-matcher`.
9. **Accessibility:** expose the block as an AccessKit text node (role `TextInput` / `MultilineTextInput`), *if the gpui-pre snapshot exposes text accessibility APIs; to verify*.

### 3.3 Relevant GPUI text APIs

| API | Purpose |
|---|---|
| `TextRun { len, font, color, background_color, underline, strikethrough }` | Style spans over a string (by UTF-8 length) |
| `StyledText::new(text).with_runs(runs)` / `.with_highlights(...)` | Element rendering styled, wrapped text |
| `InteractiveText::new(id, styled).on_click(ranges, cb).on_hover(...)` / `.tooltip(...)` | Clickable ranges (links, refs) |
| `window.text_system().shape_line(...)` / `shape_text(...)` → `ShapedLine` / `WrappedLine` | Manual shaping for custom elements: `x_for_index`, `index_for_x`, `paint` |
| `EntityInputHandler` / `ElementInputHandler` / `window.handle_input` | Platform text input and IME bridge |
| `UTF16Selection` | Selection reported to the platform |
| `cx.write_to_clipboard(ClipboardItem::new_string(..))` / `read_from_clipboard` | Clipboard (GPUI Kit's Input also has an `on_paste` hook for images and files) |

---

## Risks

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | **GPUI API churn** plus a dependency on **third-party `gpui-pre` snapshots** (one maintainer at Longbridge). If they stop, we must move to a Zed git dependency. | High | Exact pins. A thin `bitacora-app::ui` facade over GPUI/GPUI Kit types. A scheduled CI job that tries the next GPUI Kit release. Document the Zed-git fallback (Option B, §1.6). |
| R2 | **GPUI Kit breaking changes** (0.6 rename, 0.7 window entry point within a month) | Medium–High | Upgrade deliberately (for example monthly). Pin with `=`. Wrap the components we use in our own `ui::` module. |
| R3 | **Custom block editor complexity** (wrapping, hit testing, IME, undo, a11y) | High; this is the product's core | Prototype first (spike: 1,000 blocks, IME on 3 OSes). Fall back to Textarea per block for the MVP. Build a keystroke-level test suite with `#[gpui::test]`. |
| R4 | **Linux variance** (Vulkan availability, Wayland compositors, IME frameworks, fractional scaling) | Medium | Test matrix: GNOME/Wayland, KDE/Wayland, X11, NVIDIA proprietary; Flatpak build; document the Vulkan requirement. |
| R5 | **License contamination** from copying Zed's GPL crates (`editor`, `ui`, `theme`, `markdown`, `rope`?) | High (legal) | Copy only from `gpui*` crates (Apache-2.0) and GPUI Kit (Apache-2.0). Check each crate's `license` field before borrowing code. |
| R6 | Thin documentation; behaviour learned from source | Medium | Budget time for spikes. Keep an internal "GPUI cookbook" note. |
| R7 | Accessibility of custom elements | Medium | Use AccessKit roles from day one on BlockEditor and bullets. |
| R8 | Rendering performance with large pages and the shaped-text cache | Medium | Virtualized `list`, cached shaping, measured with `gpui-fps`. |

## Recommendations

1. Depend on **`gpui-kit = "=0.7.1"`** only (it brings `gpui-pre =0.3.8`). Import GPUI through `gpui_kit::gpui`. Do not use crates.io `gpui 0.2.2` or a Zed git dependency unless R1 materializes.
2. Use GPUI Kit for all chrome: Root, Sidebar, Dock/Tabs/Resizable, Dialog/Sheet, Notification, Popover, Command, List/VirtualList, Tree (namespaces), DataTable (queries), Calendar/DatePicker, Menu, Settings, Theme, Icon (Lucide), Editor (raw source, config.edn, conflicts), TextView (previews).
3. Build a custom **BlockEditor** (Option C) on `EntityInputHandler`, starting from `gpui/examples/input.rs`. Use a `String` per block, a page-level op log for undo, `StyledText`/`InteractiveText` for unfocused blocks, and Popover anchored by `bounds_for_range` for autocomplete. Allow Textarea-per-block as an MVP fallback.
4. Run tokio on its own runtime (pattern from `gpui_tokio`, vendored). Keep `bitacora-core` executor-agnostic.
5. Package with **cargo-packager** (dmg, msi/NSIS, deb, AppImage). Add Flatpak later. Evaluate **velopack** for auto-update (see [[crate-stack]]).
6. First spike (2–3 weeks): GPUI Kit window plus Sidebar plus a virtualized list of 1,000 BlockEditors with IME on macOS, Windows and Linux (X11 and Wayland). Use it to decide R3 and R4.

## Open questions

- Will Zed resume official crates.io `gpui` releases (the README now documents `gpui` + `gpui_platform` "version = \"*\"" usage)? If so, will GPUI Kit switch from `gpui-pre` to them?
- Does the `gpui-pre` snapshot expose AccessKit text-editing roles that are good enough for a custom editor?
- Is wgpu's GLES fallback enabled in gpui on Linux (for machines without Vulkan)?
- Can GPUI Kit's Input "inline tokens" mechanism be reused (Apache-2.0) as the basis for ref chips inside our BlockEditor, or is it tied to Input's internals?
- Can TextView's `MarkdownPlugin` render Logseq inline syntax efficiently enough for read-only page views (journals scroll), or do we need our own `StyledText` pipeline everywhere?
- ~~What is the binary size and cold-start time of a GPUI Kit app with tree-sitter features disabled?~~ Measured by the spike: see [[block-editor-spike-report]] (release binary about 43.6 MB with `strip = "debuginfo"`, first frame about 135 ms on Linux/X11 with software Vulkan; macOS and Windows still to measure).
- Mobile (GPUI Kit v0.6.2 claims iOS and Android): is it worth tracking for a future Bitacora mobile app?

## Sources

- GPUI Kit site and components: <https://gpui-kit.com/>, <https://gpui-kit.com/docs>, <https://gpui-kit.com/component> (input, textarea, editor, tree, text-view, dock pages)
- GPUI Kit repo, manifest and CI: <https://github.com/longbridge/gpui-kit>, <https://github.com/longbridge/gpui-kit/blob/main/Cargo.toml>, <https://github.com/longbridge/gpui-kit/releases>
- docs.rs: <https://docs.rs/crate/gpui-kit/latest>
- crates.io: <https://crates.io/crates/gpui-kit>, <https://crates.io/crates/gpui-component>, <https://crates.io/crates/gpui-pre>, <https://crates.io/crates/gpui>
- GPUI: <https://www.gpui.rs/>, <https://github.com/zed-industries/zed/tree/main/crates/gpui>, `crates/gpui/README.md`, `crates/gpui/examples/input.rs`, `crates/gpui/examples/a11y.rs`, `crates/gpui_tokio/src/gpui_tokio.rs`
- Linux renderer migration: <https://git.secluded.site/zed/commit/af8ea0d6c26192c45f44f473c0d4a7d6f72ed018>
- Ecosystem: <https://github.com/zed-industries/awesome-gpui>
- Zed Linux deps: <https://github.com/zed-industries/zed/blob/main/script/linux>
