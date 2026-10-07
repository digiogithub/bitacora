//! Golden tests for moving or clearing a `SCHEDULED:` / `DEADLINE:` date in place: only the
//! `YYYY-MM-DD Ddd` bytes change, everything else on the line and in the block stays.

use bitacora_markdown::edit::state::{clear_planning, move_planning_date};

#[test]
fn moves_the_date_and_recomputes_the_weekday() {
    assert_eq!(
        move_planning_date(
            "TODO a\nSCHEDULED: <2025-11-20 Thu>",
            "SCHEDULED:",
            2025,
            11,
            21
        ),
        "TODO a\nSCHEDULED: <2025-11-21 Fri>"
    );
}

#[test]
fn keeps_repeaters_times_and_other_lines() {
    let src =
        "TODO a\nSCHEDULED: <2025-11-20 Thu 09:30 .+1d>\nDEADLINE: <2025-12-01 Mon +1w>\nid:: x";
    assert_eq!(
        move_planning_date(src, "SCHEDULED:", 2026, 1, 5),
        "TODO a\nSCHEDULED: <2026-01-05 Mon 09:30 .+1d>\nDEADLINE: <2025-12-01 Mon +1w>\nid:: x"
    );
    assert_eq!(
        move_planning_date(src, "DEADLINE:", 2025, 12, 2),
        "TODO a\nSCHEDULED: <2025-11-20 Thu 09:30 .+1d>\nDEADLINE: <2025-12-02 Tue +1w>\nid:: x"
    );
}

#[test]
fn keeps_inactive_brackets_and_missing_weekday() {
    assert_eq!(
        move_planning_date("a\nSCHEDULED: [2025-11-20]", "SCHEDULED:", 2025, 11, 21),
        "a\nSCHEDULED: [2025-11-21 Fri]"
    );
}

#[test]
fn writes_a_fresh_line_when_missing() {
    assert_eq!(
        move_planning_date("a\nbody", "DEADLINE:", 2025, 11, 21),
        "a\nDEADLINE: <2025-11-21 Fri>\nbody"
    );
}

#[test]
fn clears_a_line_or_only_its_entry() {
    let src = "a\nSCHEDULED: <2025-11-20 Thu .+1d>\nDEADLINE: <2025-12-01 Mon>\nbody";
    assert_eq!(
        clear_planning(src, "SCHEDULED:"),
        "a\nDEADLINE: <2025-12-01 Mon>\nbody"
    );
    let shared = "a\nSCHEDULED: <2025-11-20 Thu> DEADLINE: <2025-12-01 Mon>";
    assert_eq!(
        clear_planning(shared, "SCHEDULED:"),
        "a\nDEADLINE: <2025-12-01 Mon>"
    );
    assert_eq!(
        clear_planning(shared, "DEADLINE:"),
        "a\nSCHEDULED: <2025-11-20 Thu>"
    );
    assert_eq!(clear_planning("a\nb", "DEADLINE:"), "a\nb");
}
