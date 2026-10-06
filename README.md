# Bitacora

A fast, cross-platform desktop outliner for your **Logseq Markdown graphs**, written in Rust with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/).

- **Logseq-compatible**: opens an existing Logseq graph folder and keeps its paths, file names and Markdown format intact. You can keep using Logseq on the same graph.
- **Own index**: SQLite + FTS5 for instant search, backlinks, journals and tasks.
- **Git sync built in**: automatic commit / pull / push; metadata conflicts resolve themselves, content conflicts are shown block by block — never as conflict markers in your notes.
- **MCP server**: an always-on Model Context Protocol endpoint over HTTP (localhost, token-protected) so AI agents can search and edit your notes.

> Status: early design phase. See [docs/architecture.md](docs/architecture.md) and the backlog (gintrack project `BIT`).

## Documentation

- [Architecture & decisions](docs/architecture.md)
- [Knowledge base index](docs/README.md)
- [Contributor / agent guide](AGENTS.md)

## License

[MIT](LICENSE)
