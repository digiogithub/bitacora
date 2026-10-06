# Provenance: ignore-rules

Hand-authored for Bitacora (MIT). No content copied from Logseq (ADR-015). Every file holds its own path as text.
Exercises the scanner's ignore rules (`bitacora-core` `scan.rs`, spec `01-file-graph-layout.md` §1.1): hidden files and
directories, `logseq/bak`, `logseq/.recycle`, `logseq/version-files`, `node_modules`, case-sensitive extensions
(`pages/UPPER.MD` is skipped), non-page assets and the `:hidden` config prefixes (applied by tests through a config
string, e.g. `["/archived" "test.md"]`). `.DS_Store` and symlink cases cannot be committed (gitignored / not portable)
and are covered by unit tests instead. Update the manifest with `cargo xtask fixtures update`.
