# Upgrading from 1.x to 2.0

Your graph folder is never rewritten by the upgrade. The first start of 2.0 migrates only Bitacora's own machine-local files, and shows a short notice when something changed.

## What happens automatically

| Item | Result |
|---|---|
| `settings.json` | Backed up once to `settings.json.1x.bak` (never overwritten). The old `light_theme` and `dark_theme` keys are dropped; every other setting is kept. If the file cannot be parsed it is kept as is, backed up, and you get a notice. |
| `workspace.json` (window layout) | If it is from 1.x it is backed up and removed, so the new default layout is built. Newer files are untouched. |
| `recent-graphs.json`, `keymap.json`, `mcp-tokens.json` | Same format; untouched. Your keybindings and MCP tokens keep working. |
| Search index | A cache; rebuilt from the graph whenever needed. |
| `logseq/custom.css` and the graph itself | Never touched. |

## What is different

- **Themes.** 2.0 ships two themes, Bitacora Light and Bitacora Dark, following your System / Light / Dark choice. The 1.x themes (Paper, Solarized, Midnight) are gone, and **custom theme files in `config/themes/*.json` are no longer loaded**. They stay on disk (one notice is shown, once). Colour-scheme files come back after 2.0. Until then use `logseq/custom.css` for tweaks.
- **Window.** The native title bar is replaced by Bitacora's own bar.
- **Startup.** Bitacora reopens your last graph instead of showing the picker. Turn it off in Settings > General ("Reopen last graph"). `--graph <path>` still wins.
- **Right sidebar** is now the right panel (wider, Context and Agent tabs); the local graph moved into it.
- **Pando and AI** are new and disabled by default. Nothing is sent anywhere until you enable Pando and consent per graph.

## Going back

Restore `settings.json.1x.bak` over `settings.json` before starting 1.x. The index will rebuild.
