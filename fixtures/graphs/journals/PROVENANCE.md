# Provenance: journals

Hand-authored for Bitacora (MIT). No content copied from Logseq (ADR-015). Journal files named in several
`:journal/file-name-format` styles (`yyyy_MM_dd`, `yyyy-MM-dd`, the page-title style `MMM do, yyyy`, an unrecognised
`yyyyMMdd`, an `.org` journal) plus a journal-looking file under `pages/` and a duplicate by title. Journals are detected
by title, not by directory (`01-file-graph-layout.md` §4); tests load this graph with several config strings
(`:journal/page-title-format`, `:journals-directory`). Update the manifest with `cargo xtask fixtures update`.
