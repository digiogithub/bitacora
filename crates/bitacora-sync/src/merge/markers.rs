//! Conflict marker handling (BIT-SP-0006.R8): detection of markers written by other tools and
//! splitting of well-formed regions into base/ours/theirs. We never write markers.

/// Result of looking for marker regions in a text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Regions {
    /// No marker region.
    None,
    /// Every `<<<<<<<` has its `=======` and `>>>>>>>`: the three texts.
    WellFormed {
        /// Common text plus the `|||||||` sections (diff3 style); plain markers have none.
        base: String,
        /// Common text plus the first sections.
        ours: String,
        /// Common text plus the last sections.
        theirs: String,
    },
    /// Unbalanced or out-of-order markers.
    Malformed,
}

fn trim_eol(l: &str) -> &str {
    l.trim_end_matches(['\n', '\r'])
}

fn is_start(l: &str) -> bool {
    let l = trim_eol(l);
    l == "<<<<<<<" || l.starts_with("<<<<<<< ")
}
fn is_end(l: &str) -> bool {
    let l = trim_eol(l);
    l == ">>>>>>>" || l.starts_with(">>>>>>> ")
}
fn is_base(l: &str) -> bool {
    let l = trim_eol(l);
    l == "|||||||" || l.starts_with("||||||| ")
}
fn is_sep(l: &str) -> bool {
    trim_eol(l) == "======="
}

/// True when the text has a start or end marker line (a lone `=======` is ordinary Markdown and
/// never counts by itself).
pub fn has_marker_line(s: &str) -> bool {
    s.split_inclusive('\n').any(|l| is_start(l) || is_end(l))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum St {
    Normal,
    Ours,
    Base,
    Theirs,
}

/// Splits marker regions into the three versions.
pub fn split_markers(text: &str) -> Regions {
    if !has_marker_line(text) {
        return Regions::None;
    }
    let (mut base, mut ours, mut theirs) = (String::new(), String::new(), String::new());
    let mut st = St::Normal;
    for l in text.split_inclusive('\n') {
        match st {
            St::Normal => {
                if is_start(l) {
                    st = St::Ours;
                } else if is_end(l) {
                    return Regions::Malformed;
                } else {
                    base.push_str(l);
                    ours.push_str(l);
                    theirs.push_str(l);
                }
            }
            St::Ours | St::Base => {
                if is_start(l) || is_end(l) {
                    return Regions::Malformed;
                } else if st == St::Ours && is_base(l) {
                    st = St::Base;
                } else if is_sep(l) {
                    st = St::Theirs;
                } else if is_base(l) {
                    return Regions::Malformed;
                } else if st == St::Ours {
                    ours.push_str(l);
                } else {
                    base.push_str(l);
                }
            }
            St::Theirs => {
                if is_end(l) {
                    st = St::Normal;
                } else if is_start(l) || is_base(l) {
                    return Regions::Malformed;
                } else {
                    theirs.push_str(l);
                }
            }
        }
    }
    if st != St::Normal {
        return Regions::Malformed;
    }
    Regions::WellFormed { base, ours, theirs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_diff3_regions() {
        let t = "- shared\n<<<<<<< HEAD\n- mine\n=======\n- theirs\n>>>>>>> origin/main\n- tail\n";
        assert_eq!(
            split_markers(t),
            Regions::WellFormed {
                base: "- shared\n- tail\n".into(),
                ours: "- shared\n- mine\n- tail\n".into(),
                theirs: "- shared\n- theirs\n- tail\n".into(),
            }
        );
        let t = "<<<<<<< a\n- m\n||||||| b\n- old\n=======\n- t\n>>>>>>> c\n";
        let Regions::WellFormed { base, ours, theirs } = split_markers(t) else {
            panic!("not well formed");
        };
        assert_eq!(
            (base.as_str(), ours.as_str(), theirs.as_str()),
            ("- old\n", "- m\n", "- t\n")
        );
    }

    #[test]
    fn malformed_and_none() {
        assert_eq!(split_markers("- a\n- b\n"), Regions::None);
        assert_eq!(split_markers("- a\n=======\n- b\n"), Regions::None);
        assert_eq!(split_markers("<<<<<<< x\n- a\n"), Regions::Malformed);
        assert_eq!(split_markers("- a\n>>>>>>> y\n"), Regions::Malformed);
        assert_eq!(
            split_markers("<<<<<<< x\n<<<<<<< y\n=======\n>>>>>>> z\n"),
            Regions::Malformed
        );
        assert!(has_marker_line("a\r\n<<<<<<< HEAD\r\n"));
        assert!(!has_marker_line("Title\n=======\n"));
    }
}
