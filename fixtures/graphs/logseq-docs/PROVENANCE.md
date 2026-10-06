# Provenance: logseq-docs

- Source: https://github.com/logseq/docs
- Commit: `b18138a678f445d67157411ca349238da23668ec` ("fix: broken code links", 2025-11-07), the last commit
  before Logseq 0.10.15 was tagged (2025-11-14), i.e. a file-graph-era snapshot.
- Retrieved: 2026-10-06.
- License: MIT, Copyright (c) 2023 Logseq (see `LICENSE.md`, copied unmodified). Allowed by ADR-021.
- Copied paths: `pages/`, `journals/`, `logseq/config.edn`, `LICENSE.md`.
- Excluded: `assets/`, `gifs/`, `screenshots/`, `whiteboards/`, `script/`, `logseq/custom.css`,
  `logseq/metadata.edn`, `logseq/srs-of-matrix.edn`, `bb.edn`, `typos.toml` and the top-level Markdown docs. Asset
  links inside pages therefore stay unresolved.
- Contents: 242 files in `pages/` (Markdown plus a few `.org`), 91 files in `journals/`, 1.7 MB in total.
- Modifications: none. `diff -r` against a fresh clone at the commit above, limited to the copied paths, is empty.
  Do not edit these files; refresh the snapshot by re-importing and running `cargo xtask fixtures update`.
