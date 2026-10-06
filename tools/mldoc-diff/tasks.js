// Differential helper for task syntax: for every block of one Markdown file prints what mldoc 1.5.7
// derives (marker, priority, heading size, lifted SCHEDULED/DEADLINE dates, drawers). Run as an
// external oracle only; no mldoc code is copied.
//
// Usage (from this directory), ONE FILE PER PROCESS (mldoc keeps drawer state between parses):
//   node tasks.js <file.md> > <file>.expected.json
//
// Output: {"blocks": [{"marker", "priority", "heading", "scheduled", "deadline", "repeated",
//                      "drawers": [{"name", "lines": [...]}]}]}
// `scheduled` / `deadline` follow Logseq's lifting rules (`block.cljs:240-271`): a paragraph whose
// first or second inline is a timestamp, SCHEDULED/DEADLINE kinds only, as yyyymmdd integers.
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

const text = fs.readFileSync(process.argv[2], 'utf8');
const ast = JSON.parse(Mldoc.parseJson(text, config));

const blocks = [];
for (const [node] of ast) {
  const [kind, v] = node;
  if (kind === 'Heading') {
    blocks.push({
      marker: v.marker ?? null,
      priority: v.priority ?? null,
      heading: v.size ?? null,
      scheduled: null,
      deadline: null,
      repeated: false,
      drawers: [],
    });
  } else if (blocks.length > 0) {
    const cur = blocks[blocks.length - 1];
    if (kind === 'Paragraph') {
      const isTs = (x) => x && x[0] === 'Timestamp';
      if (isTs(v[0]) || isTs(v[1])) {
        for (const inl of v.filter(isTs)) {
          const [type, ts] = inl[1];
          if (type !== 'Scheduled' && type !== 'Deadline') continue;
          const d = ts.date;
          const n = d.year * 10000 + d.month * 100 + d.day;
          if (type === 'Scheduled') cur.scheduled = n;
          else cur.deadline = n;
          if (ts.repetition) cur.repeated = true;
        }
      }
    } else if (kind === 'Drawer') {
      cur.drawers.push({ name: node[1], lines: node[2].filter((l) => l !== '\n').map((l) => l.trim()) });
    }
  }
}
console.log(JSON.stringify({ blocks }, null, 1));
