//! Block export for the semantic indexer (BIT-US-0142).
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod read_common;

use read_common::indexed;

const C: &str = "6650a1b2-0000-4000-8000-00000000000c";

#[test]
fn semantic_blocks_carry_breadcrumb_tags_and_page_context() {
    let fx = indexed(&[
        (
            "pages/Trip Plan.md",
            &format!(
                "tags:: travel\nprivate:: false\n\n- Itinerary\n  - Day one #flights\n    - Book the ferry\n      id:: {C}\n"
            ),
        ),
        ("journals/2024_05_01.md", "- Went for a walk\n"),
    ]);
    let paths = fx.reader.semantic_file_paths().expect("paths");
    assert_eq!(paths, ["journals/2024_05_01.md", "pages/Trip Plan.md"]);

    let blocks = fx
        .reader
        .semantic_blocks("pages/Trip Plan.md")
        .expect("blocks");
    let ferry = blocks.iter().find(|b| b.uuid == C).expect("ferry");
    assert_eq!(ferry.page_title, "Trip Plan");
    assert_eq!(ferry.breadcrumb, ["Itinerary", "Day one #flights"]);
    assert_eq!(ferry.tags, ["travel"]);
    assert_eq!(ferry.depth, 3);
    assert_eq!(
        ferry.page_properties,
        [
            ("tags".to_owned(), "travel".to_owned()),
            ("private".to_owned(), "false".to_owned())
        ]
    );
    assert!(!ferry.is_journal);
    let day = blocks
        .iter()
        .find(|b| b.content.starts_with("Day one"))
        .expect("day");
    assert_eq!(day.tags, ["travel", "flights"]);
    assert!(blocks.iter().any(|b| b.is_pre_block));

    let one = fx.reader.semantic_block(C).expect("one").expect("found");
    assert_eq!(&one, ferry);
    assert!(
        fx.reader
            .semantic_block("00000000-0000-4000-8000-000000000000")
            .expect("none")
            .is_none()
    );

    let j = fx
        .reader
        .semantic_blocks("journals/2024_05_01.md")
        .expect("journal");
    assert!(j[0].is_journal);
    assert_eq!(j[0].journal_day, Some(20_240_501));
    assert!(
        fx.reader
            .semantic_blocks("pages/missing.md")
            .expect("missing")
            .is_empty()
    );
}
