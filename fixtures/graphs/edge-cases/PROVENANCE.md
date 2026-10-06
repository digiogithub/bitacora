# Provenance: edge-cases and edge-cases-legacy-names

Hand-authored for Bitacora (MIT, same license as the repository). No content was copied from Logseq sources or tests
(ADR-015); each file is our own text exercising one quirk documented in the analysis pages. Files are generated
byte-exactly (CRLF, BOM, tabs and trailing whitespace are intentional), so never reformat them. Update the manifest
with `cargo xtask fixtures update` after any change. The manual check "opens in Logseq 0.10.15 without errors" is
pending (needs the Logseq desktop app).

Doc references: `02` = `docs/analysis/logseq/02-markdown-block-syntax.md`, `01` = `docs/analysis/logseq/01-file-graph-layout.md`
(`s` = section number). Logseq source cites (`path:line`) are in those sections.

## edge-cases/pages (config: `:file/name-format :triple-lowbar`)

| File | Quirk | Reference |
|---|---|---|
| indent-tabs.md | tab indentation | 02 s2.1, s2.2 |
| indent-2-spaces.md | 2-space indentation | 02 s2.1 |
| indent-4-spaces.md | 4-space indentation | 02 s2.1 |
| indent-mixed-tabs-spaces.md | mixed tabs and spaces | 02 s2.1, s8 |
| indent-irregular.md | irregular indent jumps (several levels at once) | 02 s2.1, s8 |
| heading-no-bullet.md | ATX heading without bullet | 02 s2.6 |
| heading-property.md | `heading:: true` and numeric heading property | 02 s2.6 |
| continuation-lines.md | continuation lines, blank line inside a block | 02 s2.3 |
| empty-blocks.md | empty blocks, empty parent with child | 02 s2.7 |
| whitespace-only-lines.md | whitespace-only lines between blocks | 02 s2.7, s8 |
| trailing-whitespace.md | trailing spaces and tabs | 02 s8 |
| pre-block-properties.md | `key:: value` page properties (pre-block) | 02 s2.5 |
| pre-block-yaml-front-matter.md | YAML front matter | 02 s2.5, 01 s8 |
| pre-block-directive-deep.md | `#+title:` directive deep in a page | 02 s2.5 |
| no-bullets.md | file with no bullets | 02 s2.5, s8 |
| inline-refs.md | `[[page]]`, nested refs, `#tag`, `#[[multi word]]`, `((uuid))`, `{{embed}}`, `{{query}}`, escapes, inline code | 02 s5.1, s5.2 |
| property-values.md | quoted values, empty value, URL, comma lists, non-property lookalike | 02 s3.1, s3.2 |
| tasks-markers.md | TODO/DOING/NOW/LATER/WAIT/DONE/CANCELED/CANCELLED/IN-PROGRESS, priorities | 02 s5.3 |
| scheduled-deadline.md | `SCHEDULED:` / `DEADLINE:` with repeaters | 02 s5.3 |
| logbook-drawer.md | `:LOGBOOK:` drawer with CLOCK lines | 02 s5.3 |
| collapsed-and-id.md | `collapsed::`, `id::` (first and not first property) | 02 s3.3, s4 |
| crlf.md | CRLF line endings | 01 s8, 02 s8 |
| bom-utf8.md | UTF-8 BOM | 01 s8, 02 s8 |
| bom-crlf.md | BOM plus CRLF | 01 s8 |
| mixed-line-endings.md | LF and CRLF in one file | 01 s8 |
| no-trailing-newline.md | no final newline | 02 s8 |
| unicode-中文-🚀.md | CJK and emoji title and content, combining vs precomposed | 01 s3.1, s3.7 |
| code-fence-unclosed.md | unclosed code fence swallows following bullets | 02 s8 |
| code-fence-closed.md | closed ``` and ~~~ fences containing bullet-like text | 02 s6, s8 |
| escaping.md | escaped `-`, `*`, `_`; quote and ordered-looking text | 02 s6, s8 |
| star-plus-bullets.md | `*` and `+` bullets | 02 s2.1 |
| org-page.org | `.org` page next to Markdown | 01 s7 |
| Projects___Bitacora___Design.md | namespaced title `Projects/Bitacora/Design` (triple-lowbar) | 01 s3.2, s3.5 |
| aa%3F%23___bbb___ccc.md | percent-encoded reserved chars plus namespace | 01 s3.2 |
| foo%5F%5F%5Fbar.md | literal `___` in title | 01 s3.2 |
| CON___.md | Windows reserved name | 01 s3.2 |
| ends with.___.md | title ending with a dot | 01 s3.2 |
| Case Sensitive.md | mixed-case name (also has a `logseq/bak` copy) | 01 s3.6 |
| archived.md | page under the config `:hidden` list | 01 s1.1, s2.2 |
| assets-reference.md | links to `assets/pixel.png` | 01 s5 |

## edge-cases/journals

| File | Quirk | Reference |
|---|---|---|
| 2024_05_01.md | journal in `yyyy_MM_dd` form | 01 s4 |
| 2024_05_02.md | journal with BOM and CRLF | 01 s4, s8 |
| 2030_12_31.md | future journal, no trailing newline | 01 s4 |

## edge-cases/logseq and assets

| File | Quirk | Reference |
|---|---|---|
| logseq/config.edn | triple-lowbar config, journal formats, `:hidden` | 01 s2.2 |
| logseq/bak/pages/Case Sensitive.md.bak | Logseq backup copy; must be ignored by the page scanner | 01 s1.1, s11 |
| logseq/.recycle/Deleted Page.md | recycled page; must be ignored | 01 s1.1, s10.3 |
| logseq/custom.css | custom CSS, not a page | 01 s1 |
| assets/pixel.png | 1x1 PNG asset (binary) | 01 s5 |

## edge-cases-legacy-names (config has no `:file/name-format`, so legacy naming)

| File | Quirk | Reference |
|---|---|---|
| pages/Version 1.0.md | `.` is the namespace separator in legacy names | 01 s3.3 |
| pages/Projects%2FBitacora.md | `%2F` encoded namespace | 01 s3.3 |
| pages/legacy-title-property.md | `title::` page property in legacy graphs | 01 s3.4 |
| pages/Q%3A why%3F.md | percent-encoded reserved characters | 01 s3.3 |
| logseq/config.edn | no `:file/name-format` key | 01 s2.2 |
