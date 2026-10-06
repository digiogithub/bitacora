#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Lossless tree round-trip and golden tests for the comment-preserving editor
//! (BIT-US-0071, BIT-T-0096..0098).
//!
//! The expected outputs below are hand-authored from rewrite-edn's documented behaviour (replace
//! a value in place, append an entry before the closing brace). The Babashka oracle mentioned in
//! BIT-T-0098 could not be run on this host; see the task comment. Intentional deviations from
//! rewrite-edn whitespace:
//! - a new entry in a one-entry-per-line map copies the indentation of the previous entry and is
//!   placed after any trailing comment of that entry (rewrite-edn inserts right after the value);
//! - `dissoc` of an entry alone on its lines removes the whole line(s).

use bitacora_config::{ConfigEditor, Cst, DEFAULT_CONFIG_EDN, Edn, EffectiveConfig, read_str};

const COMMENTED: &str = include_str!("data/commented-config.edn");

fn edited(src: &str, f: impl FnOnce(&mut ConfigEditor)) -> String {
    let mut ed = ConfigEditor::parse(src).expect("parse");
    f(&mut ed);
    ed.into_text()
}

/// Lines of `after` that differ from `before` as (removed, added) counts via a simple LCS-free
/// comparison: lines are compared as multisets, enough for the single-line-change checks here.
fn changed_lines(before: &str, after: &str) -> (Vec<String>, Vec<String>) {
    let b: Vec<&str> = before.lines().collect();
    let a: Vec<&str> = after.lines().collect();
    let mut rem: Vec<String> = Vec::new();
    let mut add: Vec<String> = a.iter().map(|s| (*s).to_owned()).collect();
    for l in &b {
        if let Some(i) = add.iter().position(|x| x == l) {
            add.remove(i);
        } else {
            rem.push((*l).to_owned());
        }
    }
    (rem, add)
}

// ---- round trip -----------------------------------------------------------------------------

fn assert_round_trip(name: &str, src: &str) {
    let cst = Cst::parse(src).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(cst.print(), src, "{name}: printed tree differs");
    assert_eq!(cst.source(), src);
}

#[test]
fn round_trip_own_configs() {
    assert_round_trip("commented", COMMENTED);
    assert_round_trip("default", DEFAULT_CONFIG_EDN);
    assert_round_trip("crlf", &COMMENTED.replace('\n', "\r\n"));
    assert_round_trip("empty", "");
    assert_round_trip("only comment", ";; nothing");
    assert_round_trip("commas", "{:a 1,, :b [1,2,3],}");
    assert_round_trip("unicode", "{:a \"héllo ✓\" ; ñ\n :b \\é}");
}

#[test]
fn round_trip_fixture_graph_configs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
    let mut stack = vec![root];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n == "config.edn") {
                let src = std::fs::read_to_string(&p).expect("read");
                assert_round_trip(&p.display().to_string(), &src);
                seen += 1;
            }
        }
    }
    eprintln!("round-tripped {seen} fixture config.edn files");
}

#[test]
fn round_trip_every_prefix_that_parses() {
    // Truncated inputs either fail to parse or print back exactly; they never panic.
    for (i, _) in COMMENTED.char_indices() {
        if let Ok(cst) = Cst::parse(&COMMENTED[..i]) {
            assert_eq!(cst.print(), &COMMENTED[..i]);
        }
    }
}

// ---- golden cases ---------------------------------------------------------------------------

#[test]
fn favorites_add_to_empty_vector_changes_one_line() {
    let out = edited(COMMENTED, |e| e.favorites_add("Projects").expect("add"));
    let (rem, add) = changed_lines(COMMENTED, &out);
    assert_eq!(rem, [" :favorites []"]);
    assert_eq!(add, [" :favorites [\"Projects\"]"]);
    let cfg = EffectiveConfig::from_texts(None, Some(&out));
    assert_eq!(cfg.favorites(), ["Projects"]);
}

#[test]
fn favorites_add_twice_is_idempotent_and_appends_inline() {
    let out = edited(COMMENTED, |e| {
        e.favorites_add("Projects").expect("add");
        e.favorites_add("projects").expect("dup");
        e.favorites_add("Inbox").expect("add");
    });
    assert!(out.contains(" :favorites [\"Projects\" \"Inbox\"]\n"));
}

#[test]
fn favorites_remove_inline() {
    let src = "{:favorites [\"A\" \"B\" \"C\"]}";
    assert_eq!(
        edited(src, |e| assert!(e.favorites_remove("b").expect("rm"))),
        "{:favorites [\"A\" \"C\"]}"
    );
    assert_eq!(
        edited(src, |e| assert!(e.favorites_remove("C").expect("rm"))),
        "{:favorites [\"A\" \"B\"]}"
    );
    assert_eq!(
        edited(src, |e| assert!(e.favorites_remove("A").expect("rm"))),
        "{:favorites [\"B\" \"C\"]}"
    );
    assert_eq!(
        edited(src, |e| assert!(!e.favorites_remove("Z").expect("rm"))),
        src
    );
}

#[test]
fn favorites_multiline_vector_keeps_layout_and_comments() {
    let src = "{:favorites [\"A\" ; first\n             \"B\"]\n :x 1}";
    let added = edited(src, |e| e.favorites_add("C").expect("add"));
    assert_eq!(
        added,
        "{:favorites [\"A\" ; first\n             \"B\"\n             \"C\"]\n :x 1}"
    );
    let removed = edited(src, |e| {
        assert!(e.favorites_remove("B").expect("rm"));
    });
    assert_eq!(removed, "{:favorites [\"A\" ; first\n]\n :x 1}");
}

#[test]
fn favorites_rename_replaces_only_the_string() {
    let src = "{:favorites [\"Old\" \"Keep\"] ; c\n}";
    let out = edited(src, |e| {
        assert!(e.favorites_rename("old", "New \"1\"").expect("rn"))
    });
    assert_eq!(out, "{:favorites [\"New \\\"1\\\"\" \"Keep\"] ; c\n}");
}

#[test]
fn insert_missing_key_adds_exactly_one_line_before_closing_brace() {
    let value = Edn::Map(vec![(Edn::kw("page"), Edn::str("Home"))]);
    let out = edited(COMMENTED, |e| {
        e.assoc(&["default-home"], &value).expect("assoc")
    });
    let (rem, add) = changed_lines(COMMENTED, &out);
    assert!(rem.is_empty(), "{rem:?}");
    assert_eq!(add, [" :default-home {:page \"Home\"}"]);
    // Placed after the last entry, before the trailing comment block.
    assert!(out.contains(" :nothing nil\n :default-home {:page \"Home\"}\n\n ;; trailing note"));
    let cfg = EffectiveConfig::from_texts(None, Some(&out));
    assert_eq!(
        cfg.default_home().and_then(|h| h.page).as_deref(),
        Some("Home")
    );
}

#[test]
fn insert_after_last_entry_with_trailing_comment_keeps_the_comment_on_its_line() {
    let src = "{:a 1\n :b 2 ; why\n}";
    let out = edited(src, |e| e.assoc(&["c"], &Edn::Int(3)).expect("assoc"));
    assert_eq!(out, "{:a 1\n :b 2 ; why\n :c 3\n}");
}

#[test]
fn insert_into_single_line_and_empty_maps() {
    assert_eq!(
        edited("{:a 1}", |e| e.assoc(&["b"], &Edn::Bool(true)).expect("a")),
        "{:a 1 :b true}"
    );
    assert_eq!(
        edited("{}", |e| e.assoc(&["b"], &Edn::Bool(true)).expect("a")),
        "{:b true}"
    );
}

#[test]
fn assoc_replaces_existing_value_in_place() {
    let out = edited(COMMENTED, |e| {
        e.assoc(&["file/name-format"], &Edn::kw("legacy"))
            .expect("assoc");
    });
    let (rem, add) = changed_lines(COMMENTED, &out);
    assert_eq!(
        rem,
        [" :file/name-format :triple-lowbar   ; new-style names"]
    );
    assert_eq!(add, [" :file/name-format :legacy   ; new-style names"]);
}

#[test]
fn nested_path_update_and_creation() {
    let out = edited(COMMENTED, |e| {
        e.assoc(&["default-templates", "journals"], &Edn::str("Weekly"))
            .expect("assoc");
    });
    let (rem, add) = changed_lines(COMMENTED, &out);
    assert_eq!(rem, [" :default-templates {:journals \"Daily\"}"]);
    assert_eq!(add, [" :default-templates {:journals \"Weekly\"}"]);

    // Missing intermediate maps are created.
    let out = edited("{:a 1}", |e| {
        e.assoc(&["x", "y", "z"], &Edn::Int(1)).expect("assoc");
    });
    assert_eq!(out, "{:a 1 :x {:y {:z 1}}}");

    // New key inside an existing nested map.
    let out = edited("{:m {:a 1}}", |e| {
        e.assoc(&["m", "b"], &Edn::Int(2)).expect("assoc");
    });
    assert_eq!(out, "{:m {:a 1 :b 2}}");
}

#[test]
fn update_in_sees_the_current_value() {
    let out = edited("{:n 1}", |e| {
        e.update_in(&["n"], |cur| {
            Edn::Int(cur.and_then(Edn::as_int).unwrap_or(0) + 41)
        })
        .expect("update");
        e.update_in(&["m"], |cur| {
            assert!(cur.is_none());
            Edn::Int(7)
        })
        .expect("update");
    });
    assert_eq!(out, "{:n 42 :m 7}");
}

#[test]
fn dissoc_removes_entry_lines_and_keeps_neighbouring_comments() {
    let out = edited(COMMENTED, |e| {
        assert!(e.dissoc(&["macros"]).expect("dissoc"))
    });
    let (rem, add) = changed_lines(COMMENTED, &out);
    assert!(add.is_empty(), "{add:?}");
    // The two-line macros map goes away; the comment above it stays.
    assert_eq!(rem.len(), 2, "{rem:?}");
    assert!(rem[0].starts_with(" :macros {\"hello\""));
    assert!(out.contains(" ;; Macros: shallow merged over the global config.\n"));
    assert!(!out.contains(":macros"));
    let cfg = EffectiveConfig::from_texts(None, Some(&out));
    assert!(cfg.diagnostics().is_empty());
    assert!(cfg.get("macros").is_none());
}

#[test]
fn dissoc_inline_and_with_trailing_comment() {
    assert_eq!(
        edited("{:a 1 :b 2 :c 3}", |e| assert!(
            e.dissoc(&["a"]).expect("d")
        )),
        "{:b 2 :c 3}"
    );
    assert_eq!(
        edited("{:a 1 :b 2 :c 3}", |e| assert!(
            e.dissoc(&["c"]).expect("d")
        )),
        "{:a 1 :b 2}"
    );
    assert_eq!(
        edited("{:a 1\n :b 2 ; keep me\n :c 3}", |e| assert!(
            e.dissoc(&["b"]).expect("d")
        )),
        "{:a 1\n ; keep me\n :c 3}"
    );
    assert_eq!(
        edited("{:a 1}", |e| assert!(!e.dissoc(&["zz"]).expect("d"))),
        "{:a 1}"
    );
    assert_eq!(
        edited("{:m {:a 1 :b 2}}", |e| assert!(
            e.dissoc(&["m", "a"]).expect("d")
        )),
        "{:m {:b 2}}"
    );
}

#[test]
fn default_home_rename() {
    let src = "{:default-home {:page \"Home\" :sidebar \"Contents\"}}";
    let out = edited(src, |e| {
        assert!(e.default_home_rename("home", "Start").expect("rn"))
    });
    assert_eq!(
        out,
        "{:default-home {:page \"Start\" :sidebar \"Contents\"}}"
    );
    let out = edited(src, |e| {
        assert!(!e.default_home_rename("other", "Start").expect("rn"))
    });
    assert_eq!(out, src);
}

#[test]
fn crlf_config_keeps_crlf_for_inserted_lines() {
    let src = "{:a 1\r\n :b 2\r\n}\r\n";
    let out = edited(src, |e| e.assoc(&["c"], &Edn::Int(3)).expect("assoc"));
    assert_eq!(out, "{:a 1\r\n :b 2\r\n :c 3\r\n}\r\n");
    let out = edited(src, |e| assert!(e.dissoc(&["a"]).expect("d")));
    assert_eq!(out, "{\r\n :b 2\r\n}\r\n");
}

#[test]
fn discards_are_untouched_by_edits() {
    let src = "{:a 1\n #_:old #_{:x 1}\n :favorites [#_\"gone\" \"A\"]}";
    let out = edited(src, |e| e.favorites_add("B").expect("add"));
    assert_eq!(
        out,
        "{:a 1\n #_:old #_{:x 1}\n :favorites [#_\"gone\" \"A\" \"B\"]}"
    );
    let out = edited(src, |e| e.assoc(&["a"], &Edn::Int(2)).expect("assoc"));
    assert!(out.starts_with("{:a 2\n #_:old #_{:x 1}"));
}

#[test]
fn empty_document_gets_a_map() {
    assert_eq!(
        edited("", |e| e.assoc(&["a"], &Edn::Int(1)).expect("a")),
        "{:a 1}"
    );
    assert_eq!(
        edited(";; hi", |e| e.assoc(&["a"], &Edn::Int(1)).expect("a")),
        ";; hi\n{:a 1}"
    );
}

#[test]
fn errors_on_non_map_paths_and_roots() {
    assert!(ConfigEditor::parse("[1 2]").is_err());
    assert!(ConfigEditor::parse("{:a").is_err());
    let mut ed = ConfigEditor::parse("{:a 1}").expect("parse");
    assert!(ed.assoc(&["a", "b"], &Edn::Nil).is_err());
    assert!(ed.vec_push(&["a"], &Edn::Nil).is_err());
}

#[test]
fn edited_output_rereads_to_the_expected_value() {
    let out = edited(COMMENTED, |e| {
        e.favorites_add("Projects").expect("fav");
        e.assoc(&["hidden"], &Edn::strs(["/x"])).expect("hidden");
        e.assoc(
            &["default-home"],
            &Edn::Map(vec![(Edn::kw("page"), Edn::str("H"))]),
        )
        .expect("home");
        assert!(e.dissoc(&["macros"]).expect("macros"));
    });
    let v = read_str(&out).expect("valid").expect("form");
    assert_eq!(v.get("favorites"), Some(&Edn::strs(["Projects"])));
    assert_eq!(v.get("hidden"), Some(&Edn::strs(["/x"])));
    assert_eq!(v.get("macros"), None);
    assert_eq!(
        v.get("default-home").and_then(|h| h.get("page")),
        Some(&Edn::str("H"))
    );
    // Everything else is semantically unchanged.
    let before = read_str(COMMENTED).expect("valid").expect("form");
    assert_eq!(v.get("dates"), before.get("dates"));
    assert_eq!(v.get("default-queries"), before.get("default-queries"));
}
