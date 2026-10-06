//! Property tests for the outline splitter: for *every* input the spans partition the bytes
//! (`serialize(parse(bytes)) == bytes`), and the derived structures never panic or go out of bounds.

use bitacora_markdown::properties::{PropertyConfig, interpret, scan_properties};
use bitacora_markdown::{
    ParserOptions, UnclosedRegion, build_tree, content_of, pre_block_content, split, split_with,
};
use proptest::prelude::*;

const FRAGMENTS: &[&str] = &[
    "- ",
    "-",
    "- a",
    "-x",
    "  - b",
    "\t- c",
    " \t- d",
    "    - e",
    "## h",
    "#tag",
    "#",
    "text",
    "  more text",
    "```",
    "```js",
    "~~~",
    "  ```",
    "#+BEGIN_QUOTE",
    "#+end_quote",
    "#+BEGIN_SRC js",
    "#+END_SRC",
    "---",
    "key:: value",
    "  k:: v",
    "a.b:: 1",
    "my key:: v",
    "k::v",
    "empty::",
    "#+t: 1",
    ":PROPERTIES:",
    ":custom_id: x",
    ":END:",
    "> a:: b",
    "- ```",
    "- #+BEGIN_QUOTE",
    "café",
    "[[Señor]]",
    "\u{feff}",
    "\u{a0}- nb",
    "",
    " ",
    "\t",
    "id:: 6500c1a4-0000-4000-8000-000000000001",
];

const EOLS: &[&str] = &["\n", "\r\n", "\n", "\n\n", "\r", ""];

fn arb_text() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(
        (prop::sample::select(FRAGMENTS), prop::sample::select(EOLS)),
        0..24,
    )
    .prop_map(|parts| {
        parts
            .into_iter()
            .flat_map(|(f, e)| f.bytes().chain(e.bytes()).collect::<Vec<_>>())
            .collect()
    })
}

fn check(input: &[u8], opts: ParserOptions) {
    let outline = split_with(input, opts);

    // Spans partition the input, in order, without gaps.
    let mut pos = 0;
    for s in outline.spans() {
        assert_eq!(s.start, pos);
        assert!(s.end > s.start, "empty span {s:?}");
        pos = s.end;
    }
    assert_eq!(pos, input.len());
    let rebuilt: Vec<u8> = outline
        .spans()
        .flat_map(|s| s.slice(input).iter().copied())
        .collect();
    assert_eq!(rebuilt, input);

    // Block sub-spans tile the block.
    for b in &outline.blocks {
        assert_eq!(b.indent.start, b.span.start);
        assert_eq!(b.head_line.start, b.span.start);
        assert_eq!(b.head_line.end, b.body.start);
        assert_eq!(b.body.end, b.span.end);
        assert!(b.raw_level >= 1);
        let _ = content_of(input, b);
        let _ = scan_properties(input, b.span, opts);
    }
    if let Some(pre) = outline.pre_block {
        let _ = pre_block_content(input, pre);
        for g in scan_properties(input, pre, opts).groups {
            for l in g.lines {
                let _ = interpret(&l.key_norm, &l.value_raw, &PropertyConfig::default());
            }
        }
    }

    // The tree is a forest in which parents come first.
    for (i, n) in build_tree(&outline.blocks).iter().enumerate() {
        assert!(n.parent.is_none_or(|p| p < i));
        assert!(n.prev_sibling.is_none_or(|p| p < i));
        assert!(n.depth >= 1);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn spans_partition_arbitrary_bytes(input in prop::collection::vec(any::<u8>(), 0..400)) {
        check(&input, ParserOptions::default());
    }

    #[test]
    fn spans_partition_structured_text(input in arb_text()) {
        check(&input, ParserOptions::default());
        check(&input, ParserOptions { unclosed_region: UnclosedRegion::ToEof });
    }

    #[test]
    fn split_is_deterministic_and_default_matches_explicit(input in arb_text()) {
        prop_assert_eq!(split(&input), split_with(&input, ParserOptions::default()));
    }
}
