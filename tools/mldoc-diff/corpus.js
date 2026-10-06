// Corpus-level differential oracle: for ONE Markdown file prints, per block, what mldoc 1.5.7 derives
// (block start byte offset, raw level, marker, priority, property keys, referenced pages/tags and
// block refs). Run as an external oracle only; no mldoc code is copied.
//
// Usage (from this directory):   npm install && node corpus.js <file.md>
// ONE FILE PER PROCESS: mldoc keeps drawer state between parses. The Rust harness
// `crates/bitacora-markdown/tests/mldoc_corpus.rs` spawns this script for every fixture file.
//
// Output: {"preProps": [...keys], "blocks": [{"start", "level", "marker", "priority",
//          "props": [...keys], "pages": [...sorted unique], "blocks": [...sorted unique]}]}
// References come from inline syntax of the title and body only (property values are excluded), to
// match `BlockRefs::content` on the Rust side.
const fs = require('fs');
const { Mldoc } = require('mldoc');

const config = JSON.stringify({
  toc: false,
  parse_outline_only: false,
  heading_number: false,
  keep_line_break: true,
  format: 'Markdown',
  heading_to_list: false,
});

function nested(children, pages) {
  for (const [kind, v] of children || []) {
    if (kind === 'Nested_link') {
      pages.add(v.content.slice(2, -2).trim());
      nested(v.children, pages);
    }
  }
}

// Collects refs from any JSON subtree: inline nodes are `[kind, payload]` arrays.
function refs(node, pages, blocks) {
  if (!Array.isArray(node)) {
    if (node && typeof node === 'object') for (const v of Object.values(node)) refs(v, pages, blocks);
    return;
  }
  const [kind, v] = node;
  // `#+BEGIN_QUERY` bodies are parsed by mldoc but skipped by Logseq's reference walk.
  if (kind === 'Custom' && v === 'query') return;
  if (kind === 'Link' && v && Array.isArray(v.url)) {
    const [type, value] = v.url;
    if (type === 'Page_ref') pages.add(value.trim());
    else if (type === 'Block_ref') blocks.add(value);
    refs(v.label, pages, blocks);
    return;
  }
  if (kind === 'Nested_link' && v) {
    pages.add(v.content.slice(2, -2).trim());
    nested(v.children, pages);
    return;
  }
  if (kind === 'Tag' && Array.isArray(v)) {
    const first = v[0];
    if (first && first[0] === 'Plain') pages.add(first[1].trim());
    else if (first && first[0] === 'Link') pages.add(first[1].url[1].trim());
    else if (first && first[0] === 'Nested_link') {
      pages.add(first[1].content.slice(2, -2).trim());
      nested(first[1].children, pages);
    }
    for (const rest of v.slice(1)) refs(rest, pages, blocks);
    return;
  }
  if (kind === 'Macro' && v && v.name === 'embed') {
    for (const a of v.arguments || []) {
      const p = /^\[\[(.*)\]\]$/.exec(a.trim());
      const b = /^\(\((.*)\)\)$/.exec(a.trim());
      if (p) pages.add(p[1].trim());
      if (b) blocks.add(b[1].trim());
    }
    return;
  }
  for (const x of node) refs(x, pages, blocks);
}

const text = fs.readFileSync(process.argv[2], 'utf8');
const ast = JSON.parse(Mldoc.parseJson(text, config));

const preProps = [];
const blocks = [];
for (const [node, pos] of ast) {
  const [kind, v] = node;
  if (kind === 'Heading') {
    const pages = new Set();
    const bl = new Set();
    refs(v.title, pages, bl);
    blocks.push({
      start: pos.start_pos,
      level: v.unordered ? v.level : 1,
      marker: v.marker ?? null,
      priority: v.priority ?? null,
      props: [],
      _pages: pages,
      _blocks: bl,
    });
  } else if (kind === 'Property_Drawer') {
    const keys = v.map(([k]) => k);
    if (blocks.length === 0) preProps.push(...keys);
    else blocks[blocks.length - 1].props.push(...keys);
  } else if (blocks.length > 0) {
    const cur = blocks[blocks.length - 1];
    refs(node, cur._pages, cur._blocks);
  }
}
for (const b of blocks) {
  b.pages = [...b._pages].sort();
  b.blocks = [...b._blocks].sort();
  delete b._pages;
  delete b._blocks;
}
console.log(JSON.stringify({ preProps, blocks }));
