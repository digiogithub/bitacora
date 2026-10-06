//! Robustness properties of the inline scanner, block analysis and page properties: they never
//! panic and every span is a valid, ordered range on char boundaries.

use bitacora_markdown::ParserOptions;
use bitacora_markdown::block::analyze;
use bitacora_markdown::inline::{InlineToken, scan};
use bitacora_markdown::page_props::page_properties;
use bitacora_markdown::properties::PropertyConfig;
use proptest::prelude::*;

/// Text over an alphabet dense in Logseq syntax characters.
fn syntax_text() -> impl Strategy<Value = String> {
    let atoms = prop::sample::select(vec![
        "[[",
        "]]",
        "[",
        "]",
        "(",
        ")",
        "((",
        "))",
        "{{",
        "}}",
        "{{{",
        "}}}",
        "#",
        "#[[",
        "`",
        "``",
        "$",
        "$$",
        "\\",
        "<",
        ">",
        "</b>",
        "<b>",
        "*",
        "**",
        "_",
        "__",
        "~~",
        "==",
        "^^",
        "!",
        ",",
        ":",
        ";",
        ".",
        "'",
        "\"",
        "\n",
        " ",
        "\t",
        "a",
        "b",
        "1",
        "-",
        "/",
        "é",
        "日",
        "TODO ",
        "[#A] ",
        "SCHEDULED: ",
        "<2024-01-01 Mon .+1d>",
        ":LOGBOOK:\n",
        ":END:\n",
        "key:: v\n",
        "#+title: x\n",
        "```\n",
        "---\n",
        "http://x.y/",
        "embed ",
        "6500c1a4-0000-4000-8000-000000000001",
    ]);
    prop::collection::vec(atoms, 0..40).prop_map(|v| v.concat())
}

fn assert_token_ok(text: &str, t: &InlineToken) {
    let s = t.span();
    assert!(s.start < s.end && s.end <= text.len(), "{t:?} in {text:?}");
    assert!(text.is_char_boundary(s.start) && text.is_char_boundary(s.end));
}

proptest! {

    #[test]
    fn scan_never_panics_and_tokens_are_ordered(text in syntax_text()) {
        let toks = scan(&text);
        let mut at = 0;
        for t in &toks {
            assert_token_ok(&text, t);
            prop_assert!(t.span().start >= at, "overlap in {text:?}");
            at = t.span().end;
        }
    }

    #[test]
    fn block_analysis_never_panics(text in syntax_text()) {
        let a = analyze(&text, &PropertyConfig::default(), ParserOptions::default());
        // The refs API is total.
        let _ = a.refs.pages();
    }

    #[test]
    fn page_properties_never_panic(text in syntax_text()) {
        let p = page_properties(text.as_bytes(), ParserOptions::default());
        for prop in &p.properties {
            prop_assert!(prop.line.end <= text.len());
            prop_assert!(prop.key_span.end <= text.len() && prop.value_span.end <= text.len());
        }
    }
}
