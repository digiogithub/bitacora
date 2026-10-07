# Bitacora

A fast, cross-platform desktop outliner for your **Logseq Markdown graphs**, written in Rust with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://gpui-kit.com/).

- **Logseq-compatible**: opens an existing Logseq graph folder and keeps its paths, file names and Markdown format intact. You can keep using Logseq on the same graph.
- **Own index**: SQLite + FTS5 for instant search, backlinks, journals and tasks.
- **Git sync built in**: automatic commit / pull / push; metadata conflicts resolve themselves, content conflicts are shown block by block — never as conflict markers in your notes.
- **MCP server**: an always-on Model Context Protocol endpoint over HTTP (localhost, token-protected) so AI agents can search and edit your notes.

**New in 2.0:** a redesigned, frameless interface (Bitacora Light/Dark), reopen-last-graph and a Graph menu, a tasks view, a Context/Agent right panel, a graph view with settings and SVG/PNG export, and an optional [Pando](docs/user/pando-setup.md) integration for semantic search, chat with approved edits, ghost text and Compose with AI (all off until you opt in, per graph). See [What is new](docs/user/whats-new-2.0.md), the [upgrade guide](docs/user/upgrade-from-1x.md) and the [CHANGELOG](CHANGELOG.md).

> Status: 2.0 release preparation. See [docs/architecture.md](docs/architecture.md) and the backlog (gintrack project `BIT`).

## Documentation

- [User guide](docs/user/README.md)
- [Changelog](CHANGELOG.md)
- [Architecture & decisions](docs/architecture.md)
- [Knowledge base index](docs/README.md)
- [Contributor / agent guide](AGENTS.md)

## License

[MIT](LICENSE)
