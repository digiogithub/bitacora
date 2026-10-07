# Graph view

Open it from the sidebar, the palette ("Go to: Graph") or `g g`. Pages are nodes, links are edges. The layout is a live force simulation that stops using CPU once it settles.

## Interaction
- Drag the background to pan, scroll to zoom at the cursor.
- Click a node to open the page (Shift+click: right sidebar, Ctrl/Cmd+click: new tab). Drag a node to pin it; releasing unpins it.
- Alt+click toggles a node in the **focus** set. The view shows the focus nodes plus everything within N hops (the - and + buttons, 1 to 6); "Reset focus" clears it.
- The local graph of the current page is in the right panel, Context tab.
- The view refreshes by itself when pages change, without jumping.

## Settings panel (gear button)
- **Nodes:** journals, orphan pages, built-in pages, excluded pages (pages with `exclude-from-graph-view:: true`) and Pause simulation.
- **Search:** dims every node that does not match the label filter.
- **Forces:** link distance, charge strength and charge range, with Logseq's ranges and a reset.

Choices are saved in your graph's `config.edn` under `:graph/settings` and `:graph/forcesettings`, the same keys Logseq 0.10 uses, so both apps agree. Only those entries change; the rest of the file, comments included, is preserved.

## Export
Export saves what you see (current filters and focus) as **SVG** (with page labels) or **PNG** (no labels, 2x size, at most 8192 px per side).
