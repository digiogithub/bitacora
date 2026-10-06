// Differential helper for the inline scanner: prints, for every line of a corpus file, the reference
// related inline tokens mldoc 1.5.7 finds (run as an external oracle only; no mldoc code is copied).
//
// Usage (from this directory):   npm install && node inline.js <cases.txt> > cases.expected.json
//
// Every line of the corpus is parsed as the title of a bullet block (`- <line>`). Output is a JSON
// array with one entry per line: {"case": <line>, "tokens": [...]}. Tokens are flattened in
// document order and normalised to the vocabulary of `tests/inline_fixtures.rs`:
//   ["page", name]  ["tag", name]  ["block", id]  ["macro", name, ...args]  ["code", text]
//   ["math"]  ["html"]  ["url"]  ["file", label]  ["search"]
// A nested page reference is emitted right after its parent.
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

function nestedChildren(children, out) {
  for (const [kind, v] of children) {
    if (kind === 'Nested_link') {
      out.push(['page', v.content.slice(2, -2)]);
      nestedChildren(v.children, out);
    }
  }
}

function plainLabel(label) {
  return label.map(([k, v]) => (k === 'Plain' ? v : '')).join('');
}

function walk(inlines, out) {
  for (const node of inlines) {
    const [kind, v] = node;
    switch (kind) {
      case 'Link': {
        const [type, value] = v.url;
        if (type === 'Page_ref') out.push(['page', value]);
        else if (type === 'Block_ref') out.push(['block', value]);
        else if (type === 'File') out.push(['file', plainLabel(v.label)]);
        else if (type === 'Search') out.push(['search']);
        else out.push(['url']);
        break;
      }
      case 'Nested_link':
        out.push(['page', v.content.slice(2, -2)]);
        nestedChildren(v.children, out);
        break;
      case 'Tag': {
        const first = v[0];
        if (first[0] === 'Plain') out.push(['tag', first[1]]);
        else if (first[0] === 'Link') out.push(['tag', first[1].url[1]]);
        else if (first[0] === 'Nested_link') {
          out.push(['tag', first[1].content.slice(2, -2)]);
          nestedChildren(first[1].children, out);
        }
        for (const rest of v.slice(1)) walk([rest], out);
        break;
      }
      case 'Macro':
        out.push(['macro', v.name, ...v.arguments]);
        break;
      case 'Code':
        out.push(['code', v]);
        break;
      case 'Latex_Fragment':
      case 'Displayed_Math':
        out.push(['math']);
        break;
      case 'Inline_Html':
        out.push(['html']);
        break;
      case 'Emphasis':
        walk(v[1], out);
        break;
      default:
        break;
    }
  }
}

const lines = fs.readFileSync(process.argv[2], 'utf8').split('\n').filter((l) => l.length > 0);
const result = lines.map((line) => {
  const ast = JSON.parse(Mldoc.parseJson(`- ${line}`, config));
  const tokens = [];
  for (const [node] of ast) {
    if (node[0] === 'Heading') walk(node[1].title, tokens);
  }
  return { case: line, tokens };
});
console.log(`[\n${result.map((r) => JSON.stringify(r)).join(",\n")}\n]`);
