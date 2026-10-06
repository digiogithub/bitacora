//! Shared pieces of the sync overlays: the scrim + card frame, small labelled rows and the
//! word-level diff used by the conflict resolver and the page history.

use crate::ui::text_edit::MouseButton;
use crate::ui::theme::Theme;
use crate::ui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, Window,
    div, h_flex, px, v_flex,
};

/// A dimmed full-window layer with a centred card; `on_dismiss` runs for a click outside the
/// card. The card swallows its own clicks.
pub fn modal(
    id: &'static str,
    theme: &Theme,
    width: f32,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    content: impl IntoElement,
) -> AnyElement {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .justify_center()
        .items_center()
        .bg(theme.foreground.opacity(0.28))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_dismiss(window, cx);
        })
        .child(
            v_flex()
                .id((id, 1usize))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .w(px(width))
                .max_w_full()
                .max_h_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .border_1()
                .border_color(theme.border)
                .rounded(px(8.))
                .shadow_lg()
                .child(content),
        )
        .into_any_element()
}

/// A muted caption above a value, used for the labelled rows of the panels.
pub fn labelled(theme: &Theme, label: String, value: impl IntoElement) -> AnyElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(label),
        )
        .child(div().text_sm().child(value))
        .into_any_element()
}

/// Title bar of a card.
pub fn title_bar(theme: &Theme, title: String, trailing: impl IntoElement) -> AnyElement {
    h_flex()
        .px_4()
        .py_3()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_1().text_lg().child(title))
        .child(trailing)
        .into_any_element()
}

/// One piece of a word-level diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// The text, including the whitespace that follows it.
    pub text: String,
    /// The word is not in the other text (added or changed).
    pub changed: bool,
}

fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, c) in text.char_indices() {
        let space = c.is_whitespace();
        if i > 0 && in_space && !space {
            out.push(&text[start..i]);
            start = i;
        }
        in_space = space;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// Marks the words of `text` that are not part of the longest common word sequence with
/// `reference` (the base of a conflict, or the other side). Quadratic in the word count,
/// which is small for one block; very long blocks fall back to "everything changed".
pub fn word_diff(reference: &str, text: &str) -> Vec<Piece> {
    let a: Vec<&str> = tokens(reference);
    let b: Vec<&str> = tokens(text);
    let key = |s: &str| s.trim_end().to_owned();
    if a.len().saturating_mul(b.len()) > 4_000_000 {
        return b
            .iter()
            .map(|w| Piece {
                text: (*w).to_owned(),
                changed: true,
            })
            .collect();
    }
    let ak: Vec<String> = a.iter().map(|s| key(s)).collect();
    let bk: Vec<String> = b.iter().map(|s| key(s)).collect();
    // LCS table over words.
    let mut lcs = vec![vec![0u32; bk.len() + 1]; ak.len() + 1];
    for i in (0..ak.len()).rev() {
        for j in (0..bk.len()).rev() {
            lcs[i][j] = if ak[i] == bk[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut common = vec![false; bk.len()];
    let (mut i, mut j) = (0, 0);
    while i < ak.len() && j < bk.len() {
        if ak[i] == bk[j] {
            common[j] = true;
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    b.iter()
        .zip(common)
        .map(|(w, c)| Piece {
            text: (*w).to_owned(),
            changed: !c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_changed_words() {
        assert!(word_diff("a b c", "a b c").iter().all(|p| !p.changed));
    }

    #[test]
    fn changed_and_added_words_are_marked() {
        let pieces = word_diff("the quick fox", "the slow fox jumps");
        let changed: Vec<&str> = pieces
            .iter()
            .filter(|p| p.changed)
            .map(|p| p.text.trim())
            .collect();
        assert_eq!(changed, ["slow", "jumps"]);
        let joined: String = pieces.iter().map(|p| p.text.as_str()).collect();
        assert_eq!(joined, "the slow fox jumps");
    }

    #[test]
    fn empty_reference_marks_everything() {
        assert!(word_diff("", "x y").iter().all(|p| p.changed));
        assert!(word_diff("x", "").is_empty());
    }
}
