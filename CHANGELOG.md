# Changelog

All notable changes to Bitacora. Format follows Keep a Changelog; versions follow SemVer.

## [2.1.0] - 2026-10-09

### Added
- Print: the title-bar printer button, Ctrl/Cmd+P, the command palette and the app menu open a print view of the current page in the browser with the system print dialog (also "Save as PDF").
- Title-bar tabs that do not fit collapse into one button with a dropdown listing every tab (activate or close each).
- French and Simplified Chinese UI translations; the system language is detected (Traditional Chinese falls back to Simplified).
- Code blocks show line numbers.

### Fixed
- A block that starts with a code fence renders its code inside the code box instead of a literal fence and an empty box.
- The AI journal review card is bounded and scrolls its body; its header and actions stay visible.

## [2.0.3] - 2026-10-08

### Added
- Today's journal lists the tasks scheduled for today, for tomorrow and the ones in DOING/NOW below its blocks.
- Settings gear in the title bar opens Settings directly (also Ctrl/Cmd+,).
- The footer shows the running version.
- Git over SSH: choose a private key per graph in Settings → Sync.
- Agent panel: tool calls and resolved approvals fold into one collapsible row.
- Agent model selector: enable Pando models in Settings → Pando (with filter) and pick one from the agent panel header.

### Fixed
- Git over HTTPS now asks for credentials and stores them in the system keychain; Settings → Sync can forget them.
- An empty journal day can get its first block, Enter and indent straight from the journals feed.
- The Tasks view no longer goes blank after rescheduling the last task under an active filter.
- The mouse wheel scrolls the open settings dialog instead of the view behind it.

## [2.0.2] - 2026-10-08

### Added
- New app icon from the design system in the macOS app and DMG, the Windows executable and installers, and the Linux Flatpak, .deb and AppImage packages; on Linux the window is linked to its desktop entry so the dock and task switcher show the icon.
- Project website published on GitHub Pages.

### Changed
- macOS builds are signed with a Developer ID and notarized; Windows executables and installers are Authenticode-signed.
- Repository links (crash reports, updates, About) point at digiogithub/bitacora.
- Dependency updates (sha2, getrandom, toml, GitHub Actions).

### Fixed
- Beta release builds no longer fail on Windows: pre-release tags ship the NSIS installer only, because MSI requires numeric versions.

## [2.0.1] - 2026-10-07

### Added
- Click a SCHEDULED/DEADLINE date chip to pick another date (or remove it) from a calendar, in outlines, tasks and references.
- Edit any block in place from linked references, backlinks, query results, tasks and the sidebar.
- Ctrl/Cmd+1/2/3 set the block marker to TODO / DOING / DONE; keymap settings show bindings grouped by context.
- Logseq special blocks: code fences with copy button, quotes, NOTE/TIP/IMPORTANT/CAUTION/WARNING/PINNED callouts, EXAMPLE/CENTER/VERSE, with `<` commands inserting Logseq markers; Enter and Tab stay inside code regions.
- Click modifiers everywhere: Shift+click opens in the right sidebar, Ctrl/Cmd+click opens in a new tab (graph focus toggle moved to Alt+click).
- Chat auto-approves Pando read tools and can remember allow/deny decisions for write tools per graph.

### Changed
- Pando settings keep a single "Revoke and remove my data" action.

### Fixed
- Graph view nodes no longer jitter: layouts are computed off-screen and only move while dragging.
- Title-bar search no longer overlaps other controls in narrow windows.
- macOS DMG smoke test, Flatpak build flags and runtime, and CI on all platforms.

## [2.0.0] - 2026-10-07

### Added
- New design system: Bitacora Light and Bitacora Dark themes generated from design tokens, embedded fonts, a component kit, restyled outline, journals, sidebar, palette, popovers and settings.
- Frameless main window with Bitacora's own top bar (content, tabs, window controls, narrow-window behaviour).
- Reopen the last graph at startup (setting "Reopen last graph", default on) and a Graph menu: Open graph, Open recent, Close graph (palette commands and keys; native menu on macOS).
- Tasks view grouped Overdue / This week / Later / No date with marker, priority and page filters.
- Right panel with Context (local graph, page properties, backlinks, related blocks, opened blocks) and Agent tabs.
- Graph view: force layout, focus and N hops, settings panel persisted in `config.edn` (Logseq 0.10 keys), SVG and PNG export.
- Optional Pando integration: managed (supervised `pando serve`) and external modes, per-graph consent, exclusions, status and degradation indicator, activity log, keychain tokens.
- Semantic and hybrid search in the command palette, related blocks, MCP tools `semantic_search` and `related_blocks`, and `bitacora-cli semantic status|resync|purge|search`.
- AI chat in the right panel with streaming answers, tool cards, threads, context chips, and `propose_edit` diffs that are applied only after explicit approval (undoable and audited).
- Journal review card (today / this week) and AI suggestion chips for links, tags and related pages; results cached locally, accepted suggestions are undoable edits.
- Inline ghost text and Compose with AI in the editor (both off by default).
- `docs/user/`: user guide for all of the above.

### Changed
- Right sidebar is now the 360 px right panel; the local graph moved into its Context tab.
- Settings migrate automatically from 1.x (backup `settings.json.1x.bak`; the 1.x layout file is reset).

### Removed
- The 1.x themes (Paper, Solarized, Midnight) and loading of user theme files in `config/themes/*.json`; they stay on disk. Colour-scheme files are planned after 2.0. `logseq/custom.css` is unchanged.

### Notes
- Pando and every AI feature are opt-in; nothing is sent until Pando is enabled and the graph has consent. See `docs/user/pando-setup.md`.
- A managed Pando shares the user's knowledge base only when the global Pando config sets an absolute `Data.Directory`.
- Upgrade guide: `docs/user/upgrade-from-1x.md`.
