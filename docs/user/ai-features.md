# AI features

All features need [[pando-setup]] (Pando connected, graph consent) and respect your exclusions and `private::` content. Each can be switched off.

## Semantic and hybrid search
The palette (Ctrl/Cmd+K) shows keyword results first, then merges in meaning-based hits from Pando; those carry a "semantic" badge, and a "stale" badge when the block changed since it was indexed. If Pando is unreachable you see a hint and keyword results only. The **Related blocks** section of the right panel (Context tab) lists blocks similar to the one you are on. Settings > Pando has Resync and Remove semantic data. Snippets always come from your local files, never from Pando.

## For agents and scripts
MCP tools `semantic_search` and `related_blocks` (read scope; errors `SEMANTIC_DISABLED` / `SEMANTIC_UNAVAILABLE`). CLI, each with `--graph <path>`:

```
bitacora-cli semantic status
bitacora-cli semantic resync
bitacora-cli semantic purge
bitacora-cli semantic search "query" --limit 10
```
Exit code 3 means semantic search is not enabled or not connected. `bitacora-cli doctor` reports the Pando settings.

## Chat
Right panel > Agent tab (or the sparkle in the top bar). Answers stream as Markdown; `[[Page]]` links are clickable. Tool calls appear as collapsible cards. "+ Page" and "+ Selection" attach context (what the chip shows is exactly what is sent). Palette: "Ask Pando about the selection". Past threads can be resumed or deleted.

By default the assistant is read-only. Turn on "Can propose edits" to let it propose changes: each proposal shows as a diff card with **Apply** and **Deny**. Nothing is written until you apply. An applied edit is a normal undoable change (Ctrl/Cmd+Z), recorded in the agent activity log. Unanswered cards are denied after a timeout and when you close the panel, switch thread or graph, or quit.

## Journal review and recommendations
Journal review summarises a range of journal days (themes, mood, still-open tasks, next actions). Recommendations suggest related pages, links and tags for the current page, checked against your real blocks so nothing invented is shown. Both run only on demand by default (an optional daily review and auto-recommend are off by default), reuse a local cache until the text changes, and never write into your graph by themselves.

- **Review card:** above the journals feed, with **Review today** and **Review this week** (also in the command palette). It lists the summary, mood, themes, still-open tasks (click to open the block) and next actions.
- **Suggestion chips:** in the right panel's Context tab. Accepting a link or tag is a normal undoable edit; dismissed suggestions are remembered on this machine and not offered again.
- **Settings:** Settings > Pando has switches for journal review, recommendations, the daily review (with its time) and auto-recommend.

## Ghost text and Compose with AI (off by default)
Enable in Settings > Editor.
- **Ghost text:** after about 1.2 s of pause at the end of a block, a faint continuation appears. Tab accepts (one undo step); Esc or any key dismisses it. Never during IME composition or on property lines.
- **Compose with AI:** Ctrl/Cmd+J opens a box under the block. Type an instruction (optionally with the block as context), review the streamed draft, then Insert below, Replace block, Retry or Discard (Esc). Nothing changes the page until you choose.

Neither runs for excluded or private content.
