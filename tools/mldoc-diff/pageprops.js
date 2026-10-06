// Differential helper for page properties: for every case of a corpus file prints the page
// properties Logseq 0.10.x derives with mldoc 1.5.7 (run as an external oracle only).
//
// Usage (from this directory):   node pageprops.js <cases.txt> > <cases.expected.json>
//
// The corpus separates cases with a line `====`. For each case the output has `properties`: the
// [key, value] pairs of the page. This mirrors Logseq:
//   * `collect-page-properties` (mldoc.cljc:117-131): if the AST has any Directive node (front
//     matter lines and `#+key:` lines, anywhere) they all become the page properties;
//   * otherwise the first Property_Drawer before the first Heading (pre-block, block.cljs:640-683).
// Keys are lower-cased and values trimmed, as in extract.cljc:227-241.
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

let corpus = fs.readFileSync(process.argv[2], 'utf8');
if (corpus.endsWith('\n')) corpus = corpus.slice(0, -1);
const cases = corpus.split('\n====\n');
const out = cases.map((text) => {
  const ast = JSON.parse(Mldoc.parseJson(text, config));
  const nodes = ast.map(([n]) => n);
  const directives = nodes.filter((n) => n[0] === 'Directive');
  let props = [];
  if (directives.length > 0) {
    props = directives.map((n) => [n[1], n[2]]);
  } else {
    for (const n of nodes) {
      if (n[0] === 'Heading') break;
      if (n[0] === 'Property_Drawer') {
        props = n[1].map(([k, v]) => [k, v]);
        break;
      }
    }
  }
  return {
    case: text,
    properties: props.map(([k, v]) => [k.toLowerCase(), v.trim()]),
  };
});
console.log(`[\n${out.map((r) => JSON.stringify(r)).join(',\n')}\n]`);
