// Differential helper: prints the block segmentation mldoc 1.5.7 produces for Markdown files, so the
// expectations of the Rust outline fixtures (and the open questions in
// docs/analysis/logseq/02-markdown-block-syntax.md section 12) rest on observed behaviour.
//
// Usage (from this directory):   npm install && node outline.js <file.md>...
//
// Output per file: {"blocks": [{"start": <utf8 byte offset>, "level": <raw level>}]}
// `level` follows Logseq's extraction: bullet headings use mldoc's level (indent chars + 1) and
// headings without a bullet are forced to level 1 (`unordered: false`).
const fs = require('fs');
const { Mldoc } = require('mldoc');

// Same JSON config Logseq passes to mldoc (deps/graph-parser/.../mldoc.cljc default-config).
const config = JSON.stringify({
  toc: false,
  parse_outline_only: false,
  heading_number: false,
  keep_line_break: true,
  format: 'Markdown',
  heading_to_list: false,
});

for (const file of process.argv.slice(2)) {
  const text = fs.readFileSync(file, 'utf8');
  const ast = JSON.parse(Mldoc.parseJson(text, config));
  const blocks = ast
    .filter(([node]) => node[0] === 'Heading')
    .map(([node, pos]) => ({
      start: pos.start_pos,
      level: node[1].unordered ? node[1].level : 1,
    }));
  console.log(JSON.stringify({ file, blocks }));
}
