# Changelog

All notable changes to Bitacora. Format follows Keep a Changelog; versions follow SemVer.

## [2.0.0] - Unreleased

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
- Journal review and recommendation engines (cached locally, never written to the graph).
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
