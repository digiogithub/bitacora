//! Template: `proptest` identity property on the placeholder splitter.
//!
//! Failure persistence: proptest writes minimal failing inputs to
//! `proptest-regressions/<test file>.txt` next to this crate's `Cargo.toml` (the
//! directory is committed, never git-ignored) and replays them on every run. Commit new
//! regression files together with the fix.

mod common;

use proptest::prelude::*;

proptest! {
    /// `join(split(s)) == s` for arbitrary text, including CRLF, tabs and a BOM.
    #[test]
    fn split_join_is_identity(s in "(\u{feff})?([a-z\t \r\n-]|\u{4e2d}|\u{1f680}){0,200}") {
        let lines = common::split_lines(&s);
        prop_assert_eq!(common::join_lines(&lines), s);
    }

    #[test]
    fn split_join_is_identity_for_any_string(s in any::<String>()) {
        let lines = common::split_lines(&s);
        prop_assert_eq!(common::join_lines(&lines), s);
    }
}
