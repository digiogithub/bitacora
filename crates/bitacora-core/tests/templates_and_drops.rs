//! Template insertion, cross-page moves and Alt-drop block references (BIT-US-0105,
//! BIT-US-0106). Written from the documented rules (ADR-015).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use bitacora_config::EffectiveConfig;
use bitacora_core::date::Date;
use bitacora_core::editor::{
    BlockId, Cmd, CommitError, Refusal, Target, TemplateContext, Workspace,
};
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;

fn load(ws: &mut Workspace, title: &str, text: &str) -> PageKey {
    let key = PageKey::from_title(title);
    ws.load_page(
        key.clone(),
        title,
        GraphPath::new(&format!("pages/{title}.md")).ok(),
        text.as_bytes(),
    );
    key
}

fn ser(ws: &Workspace, key: &PageKey) -> String {
    String::from_utf8(ws.page(key).expect("page").serialize()).expect("utf8")
}

fn id(ws: &Workspace, key: &PageKey, text: &str) -> BlockId {
    ws.page(key)
        .expect("page")
        .blocks
        .values()
        .find(|b| b.text == text)
        .unwrap_or_else(|| panic!("no block {text:?}"))
        .id
}

fn ctx() -> TemplateContext {
    let cfg = EffectiveConfig::from_texts(None, Some("{}"));
    TemplateContext::new(
        Date::new(2025, 11, 14).expect("date"),
        "09:05",
        &cfg,
        "Target",
    )
}

const TPL: &str = "- Meetings\n  template:: meeting\n  - Date: <% today %> at <% time %>\n  - Notes on <% current page %>\n    - <% tomorrow %>\n";

#[test]
fn template_names_are_listed_and_variables_expand() {
    let mut ws = Workspace::new();
    load(&mut ws, "tpl", TPL);
    assert_eq!(ws.template_names(), vec!["meeting".to_owned()]);
    let blocks = ws.template_blocks("Meeting", &ctx()).expect("template");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Date: [[Nov 14th, 2025]] at 09:05");
    assert_eq!(blocks[1].text, "Notes on [[Target]]");
    assert_eq!(blocks[1].children[0].text, "[[Nov 15th, 2025]]");
    assert!(ws.template_blocks("nope", &ctx()).is_none());
}

#[test]
fn inserting_a_template_replaces_the_trigger_block_in_one_undo_step() {
    let mut ws = Workspace::new();
    load(&mut ws, "tpl", TPL);
    let key = load(&mut ws, "Target", "- first\n- /meet\n- last\n");
    let before = ser(&ws, &key);
    let target = id(&ws, &key, "/meet");
    let tx = ws
        .run(
            "Insert template",
            &Cmd::InsertTemplate {
                target,
                trigger: 0..5,
                name: "meeting".into(),
                ctx: ctx(),
            },
        )
        .expect("insert");
    assert_eq!(
        ser(&ws, &key),
        "- first\n- Date: [[Nov 14th, 2025]] at 09:05\n- Notes on [[Target]]\n\t- [[Nov 15th, 2025]]\n- last\n"
    );
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws, &key), before);
}

#[test]
fn a_block_with_other_text_keeps_it_and_gets_the_template_after() {
    let mut ws = Workspace::new();
    load(&mut ws, "tpl", TPL);
    let key = load(&mut ws, "Target", "- Agenda /meet\n- last\n");
    let target = id(&ws, &key, "Agenda /meet");
    let before = ser(&ws, &key);
    let tx = ws
        .run(
            "Insert template",
            &Cmd::InsertTemplate {
                target,
                trigger: 7..12,
                name: "meeting".into(),
                ctx: ctx(),
            },
        )
        .expect("insert");
    let out = ser(&ws, &key);
    assert!(
        out.starts_with("- Agenda\n- Date: [[Nov 14th, 2025]]"),
        "{out}"
    );
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws, &key), before);
}

#[test]
fn unknown_template_is_refused_without_touching_the_page() {
    let mut ws = Workspace::new();
    let key = load(&mut ws, "Target", "- /x\n");
    let target = id(&ws, &key, "/x");
    let err = ws
        .run(
            "Insert template",
            &Cmd::InsertTemplate {
                target,
                trigger: 0..2,
                name: "none".into(),
                ctx: ctx(),
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        CommitError::Refused(Refusal::NothingApplicable)
    ));
    assert_eq!(ser(&ws, &key), "- /x\n");
}

#[test]
fn moving_blocks_across_pages_is_one_transaction() {
    let mut ws = Workspace::new();
    let a = load(&mut ws, "A", "- a1\n\t- a1x\n- a2\n- a3\n");
    let b = load(&mut ws, "B", "- b1\n- b2\n");
    let (sa, sb) = (ser(&ws, &a), ser(&ws, &b));
    let (a1, a3, b1) = (id(&ws, &a, "a1"), id(&ws, &a, "a3"), id(&ws, &b, "b1"));
    let tx = ws
        .run(
            "Move blocks",
            &Cmd::MoveBlocks {
                ids: vec![a1, a3],
                target: Target::LastChild(b1),
            },
        )
        .expect("move");
    assert_eq!(ser(&ws, &a), "- a2\n");
    assert_eq!(ser(&ws, &b), "- b1\n\t- a1\n\t\t- a1x\n\t- a3\n- b2\n");
    ws.undo(&tx).expect("undo");
    assert_eq!((ser(&ws, &a), ser(&ws, &b)), (sa, sb));
}

#[test]
fn dropping_into_the_own_descendants_is_refused() {
    let mut ws = Workspace::new();
    let a = load(&mut ws, "A", "- a1\n\t- a1x\n- a2\n");
    let (a1, a1x) = (id(&ws, &a, "a1"), id(&ws, &a, "a1x"));
    for target in [
        Target::Before(a1x),
        Target::After(a1x),
        Target::LastChild(a1x),
        Target::FirstChild(a1),
        Target::Before(a1),
    ] {
        let r = ws.run(
            "Move",
            &Cmd::MoveBlocks {
                ids: vec![a1],
                target,
            },
        );
        assert!(
            matches!(r, Err(CommitError::Refused(Refusal::TargetInsideSelection))),
            "{target:?}"
        );
    }
}

#[test]
fn alt_drop_adds_a_reference_block_and_the_id_in_one_step() {
    let mut ws = Workspace::new();
    let a = load(&mut ws, "A", "- source\n- other\n");
    let b = load(&mut ws, "B", "- b1\n");
    let (src, b1) = (id(&ws, &a, "source"), id(&ws, &b, "b1"));
    let (sa, sb) = (ser(&ws, &a), ser(&ws, &b));
    let tx = ws
        .run(
            "Drop reference",
            &Cmd::DropBlockRef {
                sources: vec![src],
                target: Target::After(b1),
            },
        )
        .expect("drop ref");
    let a_after = ser(&ws, &a);
    let uuid = ws.block(src).and_then(|x| x.uuid).expect("uuid");
    assert!(a_after.contains(&format!("id:: {}", uuid.hyphenated())));
    assert_eq!(ser(&ws, &b), format!("- b1\n- (({}))\n", uuid.hyphenated()));
    ws.undo(&tx).expect("undo");
    assert_eq!((ser(&ws, &a), ser(&ws, &b)), (sa, sb));
}

#[test]
fn alt_drop_of_several_blocks_adds_references_in_order() {
    let mut ws = Workspace::new();
    let a = load(&mut ws, "A", "- one\n- two\n- three\n");
    let (one, two, three) = (id(&ws, &a, "one"), id(&ws, &a, "two"), id(&ws, &a, "three"));
    let before = ser(&ws, &a);
    let tx = ws
        .run(
            "Drop references",
            &Cmd::DropBlockRef {
                sources: vec![one, two],
                target: Target::Before(three),
            },
        )
        .expect("refs");
    let (u1, u2) = (
        ws.block(one).and_then(|b| b.uuid).expect("id one"),
        ws.block(two).and_then(|b| b.uuid).expect("id two"),
    );
    let out = ser(&ws, &a);
    let r1 = out
        .find(&format!("- (({}))", u1.hyphenated()))
        .expect("ref one");
    let r2 = out
        .find(&format!("- (({}))", u2.hyphenated()))
        .expect("ref two");
    let t3 = out.find("- three").expect("three");
    assert!(r1 < r2 && r2 < t3, "{out}");
    ws.undo(&tx).expect("undo");
    assert_eq!(ser(&ws, &a), before);
}

#[test]
fn an_empty_alt_drop_is_refused() {
    let mut ws = Workspace::new();
    let a = load(&mut ws, "A", "- one\n");
    let one = id(&ws, &a, "one");
    let err = ws
        .run(
            "Drop references",
            &Cmd::DropBlockRef {
                sources: Vec::new(),
                target: Target::After(one),
            },
        )
        .unwrap_err();
    assert!(matches!(err, CommitError::Refused(Refusal::EmptySelection)));
}
