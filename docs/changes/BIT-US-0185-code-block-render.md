# BIT-US-0185: fenced code block opening a bullet renders inside its box, with line numbers

Plan: [[bit-m-0011-owner-improvements-plan]]

## Root cause
`BlockModel::from_content` (`crates/bitacora-app/src/render/model.rs`) always treats the first
content line as the title. For `- ```` the title became the literal "```", the code line became a
paragraph, and the closing "```" opened a new, unclosed (empty) fence, hence the empty code box.
The parser (`bitacora-markdown`) was correct: the block content is
"```\nfastcgi_param HTTPS on;\n```" with bullet and tab/continuation indent stripped.

## Changes
- `BlockModel::from_content`: a first line starting with a fence opens `Region::Fence` directly (language
  taken from the rest of the line); the block has no title. Click-to-caret offsets (`line_src`) still map.
- `views/block_view.rs`: `code_element` now renders a line-number gutter (`gutter_numbers`, muted,
  right-aligned, monospace) beside the code; the empty title row is skipped when the body starts with code.
  Copy button unchanged; editing still shows raw text.
- No file bytes change; round trip unchanged.

## Tests
- `render::model::tests::fence_on_the_first_line_is_a_code_block_without_title` (no language / `nginx`).
- `views::block_view::look_tests::{code_gutter_numbers_every_line, nested_fence_without_language_renders_code_inside_the_box}`.
- `bitacora-markdown/tests/roundtrip_suite.rs`: two spec cases and `nested_tab_fence_is_one_block_with_clean_content`.
