# A tour of the 2.0 interface

## Look
Bitacora Light and Bitacora Dark share one palette, type scale and spacing system. Appearance (System / Light / Dark) is in Settings. `logseq/custom.css` overrides still apply; theme files do not (see [[upgrade-from-1x]]).

## Frameless window and top bar
The top bar (52 px) holds navigation and tabs on the left, the page title in the centre and actions on the right (the sparkle opens the Agent tab; the Pando control shows connection status, see [[pando-setup]]). Drag it to move the window, double-click to maximise. On Linux, right-click it for the window menu. Resize from any window edge. The window controls (minimise, maximise, close) are drawn by Bitacora; macOS keeps its native traffic lights. Narrow windows collapse the sidebars.

## Click modifiers: where things open
The same three clicks work everywhere a page or block can be opened: page links `[[page]]`, tags `#tag`, block references `((uuid))`, block bullets, journal titles, favorites and recents in the left sidebar, linked references, query results, Tasks rows, All pages rows, graph nodes and search results.

| Click | Result |
|---|---|
| Click | Opens in the current tab (a bullet zooms into the block; text you click in a block edits it, as before) |
| Shift+click | Opens in the right sidebar |
| Ctrl+click (Cmd+click on macOS) | Opens in a new tab; the tab you clicked in keeps what it showed |

In the search palette, Enter opens in the current tab, Shift+Enter in the right sidebar; a mouse click honours the same modifiers. In the graph view, focusing a node moved to Alt+click (see [[graph-view-guide]]).

## Reopen last graph and the Graph menu
At startup Bitacora reopens your most recent graph. If its folder has disappeared you get a warning, the entry is removed and the picker is shown. Graph commands (native menu bar on macOS; command palette and keys elsewhere):

| Command | Key |
|---|---|
| Open graph... | Ctrl/Cmd+O |
| Open recent (submenu) | |
| Close graph | Ctrl/Cmd+Shift+W |

"Switch graph" in the sidebar still shows the picker. Linux and Windows have no in-window menu bar yet; use the palette (Ctrl/Cmd+K) or the keys.

## Tasks view
Palette: "Go to: Tasks". Open tasks are grouped Overdue, This week (today to +6 days), Later and No date, using the earlier of SCHEDULED and DEADLINE. Filter chips (with counts) narrow by marker (TODO, DOING...), priority A/B/C and page. Click a row to open its block (Shift+click: right sidebar, Ctrl/Cmd+click: new tab); click the checkbox to complete it; click the marker to cycle it. All changes are normal undoable edits to your Markdown.

## Right panel
Toggle the right dock. Two tabs:
- **Context:** local graph of the current page, page properties, backlinks with counts and snippets, **Related blocks** (semantic, needs [[pando-setup]]) and the stack of blocks you opened with Shift+click.
- **Agent:** the AI chat, see [[ai-features]]. Without Pando it shows "Pando not configured".

## Settings
Settings > Pando is described in [[pando-setup]]; Settings > Editor holds the AI assist switches. Graph view settings are in [[graph-view-guide]].
