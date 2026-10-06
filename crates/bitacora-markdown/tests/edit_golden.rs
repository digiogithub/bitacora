//! Golden tests (insta) for the surgical block edits: each case prints the input, the operation and
//! the exact output so a diff shows precisely which bytes an operation changes.

use bitacora_markdown::edit::identity::ensure_block_id;
use bitacora_markdown::edit::properties::{remove_property, set_property, set_property_values};
use bitacora_markdown::edit::state::{
    ClockTime, CollapseMode, Timestamp, clock_in, clock_out, set_collapsed, set_deadline,
    set_marker, set_scheduled,
};
use bitacora_markdown::image_meta::set_size;
use uuid::Uuid;

fn case(out: &mut String, op: &str, input: &str, output: &str) {
    out.push_str(&format!(
        "== {op}\n-- in\n{input:?}\n-- out\n{output:?}\n\n"
    ));
}

fn ts(day: u32, repeater: Option<&str>) -> Timestamp {
    Timestamp {
        active: true,
        year: 2024,
        month: 1,
        day,
        time: None,
        repeater: repeater.map(str::to_owned),
    }
}

fn clock(h: u32, m: u32) -> ClockTime {
    ClockTime {
        year: 2024,
        month: 1,
        day: 1,
        hour: h,
        minute: m,
        second: 0,
    }
}

#[test]
fn property_edits() {
    let mut o = String::new();
    let tests: [(&str, &str, &str, &str); 8] = [
        ("replace in place", "task\nb:: 1\na:: 2", "b", "3"),
        (
            "append to group",
            "task\nb:: 1\nbody text",
            "Status",
            "open",
        ),
        ("insert after title", "task\nbody", "k", "v"),
        ("insert on title only", "task", "k", "v"),
        ("empty content", "", "k", "v"),
        ("before fence", "```\nk:: 1\n```", "k", "2"),
        (
            "after planning lines",
            "task\nSCHEDULED: <2024-01-01 Mon>\nbody",
            "k",
            "v",
        ),
        (
            "drawer converted",
            "t\n:PROPERTIES:\n:a: 1\n:END:\nbody",
            "b",
            "2",
        ),
    ];
    for (name, input, key, value) in tests {
        case(
            &mut o,
            &format!("set_property {name} {key}={value}"),
            input,
            &set_property(input, key, value),
        );
    }
    case(
        &mut o,
        "set_property_values",
        "t",
        &set_property_values("t", "tags", &["a", "b c"]),
    );
    for (input, key) in [("a\nk:: v\nz:: 1\n", "k"), ("```\nk:: 1\n```", "k")] {
        case(
            &mut o,
            &format!("remove_property {key}"),
            input,
            &remove_property(input, key),
        );
    }
    insta::assert_snapshot!(o);
}

#[test]
fn id_collapsed_planning_logbook() {
    let id = Uuid::parse_str("6500c1a4-0000-4000-8000-000000000001").expect("uuid");
    let mut o = String::new();

    let src = "Parent block\ntags:: demo";
    case(
        &mut o,
        "ensure_block_id",
        src,
        &ensure_block_id(src, id, false).expect("id").0,
    );
    let with_id = ensure_block_id(src, id, false).expect("id").0;
    case(
        &mut o,
        "ensure_block_id again (no-op)",
        &with_id,
        &ensure_block_id(&with_id, Uuid::nil(), false).expect("id").0,
    );

    let src = "collapsed parent";
    let on = set_collapsed(src, true, CollapseMode::InFile);
    case(&mut o, "collapse", src, &on);
    case(
        &mut o,
        "expand",
        &on,
        &set_collapsed(&on, false, CollapseMode::InFile),
    );

    let src = "TODO [#A] Parent block #tag";
    case(
        &mut o,
        "scheduled",
        src,
        &set_scheduled(src, Some(&ts(1, Some(".+1d")))),
    );
    let both = set_deadline(
        &set_scheduled("title\nbody", Some(&ts(1, None))),
        Some(&ts(5, None)),
    );
    case(&mut o, "scheduled + deadline", "title\nbody", &both);
    case(&mut o, "unschedule", &both, &set_scheduled(&both, None));

    let base = "DOING work\nowner:: me\nbody";
    let ci = clock_in(base, clock(10, 0));
    case(&mut o, "clock_in new logbook after properties", base, &ci);
    let co = clock_out(&ci, clock(11, 30));
    case(&mut o, "clock_out", &ci, &co);
    case(&mut o, "clock_in merges", &co, &clock_in(&co, clock(12, 0)));

    case(
        &mut o,
        "marker TODO -> DONE",
        "TODO a\nb",
        &set_marker("TODO a\nb", Some("DONE")),
    );
    case(
        &mut o,
        "marker removed under heading",
        "## DONE a",
        &set_marker("## DONE a", None),
    );
    insta::assert_snapshot!(o);
}

#[test]
fn image_resize() {
    let t = "![a.png](../assets/a.png){:height 100, :width 200}";
    assert_eq!(
        set_size(t, 0, 300, 150),
        "![a.png](../assets/a.png){:height 150, :width 300}"
    );
}
