// Regenerates fixtures/markdown/outline/*.expected.json.
//
// `start` and `level` come from mldoc 1.5.7 (see outline.js). `parent` (index into `blocks`, or null
// for a top-level block) is written by hand below, because mldoc only reports levels and the tree
// is built from them by Logseq's own algorithm; each row was derived by hand from the documented
// rules in docs/analysis/logseq/02-markdown-block-syntax.md section 2.1.
//
// Usage (from this directory, after `npm install`):  node gen-outline-expected.js
const fs = require('fs');
const path = require('path');
const { Mldoc } = require('mldoc');

const dir = path.join(__dirname, '..', '..', 'fixtures', 'markdown', 'outline');
const config = JSON.stringify({
  toc: false,
  parse_outline_only: false,
  heading_number: false,
  keep_line_break: true,
  format: 'Markdown',
  heading_to_list: false,
});

const parents = {
  'atx-headings.md': [null, null, null, 2, null],
  'begin-quote.md': [null, null],
  'blank-lines.md': [null, null, 1],
  'bom-bullet.md': [null],
  'bom.md': [null, null],
  'crlf.md': [null, 0, null],
  'empty-bullets.md': [null, null, null, 2],
  'fenced-code.md': [null, null, null, null],
  'front-matter.md': [null],
  'irregular-indent.md': [null, 0, 1, 1, null],
  'mixed-indent.md': [null, 0, 1, 1, 3, 1, null],
  'no-trailing-newline.md': [null, null],
  'only-text.md': [],
  'two-space-indent.md': [null, 0, 1, 0, null],
  'unclosed-begin-quote.md': [null, null, null],
  'unclosed-fence.md': [null, null, null],
};

for (const [name, ps] of Object.entries(parents)) {
  const text = fs.readFileSync(path.join(dir, name), 'utf8');
  const ast = JSON.parse(Mldoc.parseJson(text, config));
  const heads = ast.filter(([node]) => node[0] === 'Heading');
  if (heads.length !== ps.length) throw new Error(`${name}: ${heads.length} blocks, ${ps.length} parents`);
  const blocks = heads.map(([node, pos], i) => ({
    start: pos.start_pos,
    level: node[1].unordered ? node[1].level : 1,
    parent: ps[i],
  }));
  const out = path.join(dir, name.replace(/\.md$/, '.expected.json'));
  fs.writeFileSync(out, JSON.stringify({ blocks }, null, 2) + '\n');
}
