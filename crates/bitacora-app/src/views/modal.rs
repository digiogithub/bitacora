//! Shared pieces of the sync overlays: the scrim + card frame, small labelled rows and the
//! word-level diff used by the conflict resolver and the page history.
//!
//! The frame follows the popover surface of the design system (`raised`, `line` border,
//! `radius_popover`, `shadow_lg`), styled from the `BitacoraTheme` global at render time.

use crate::ui::text_edit::MouseButton;
use crate::ui::theme::{ActiveBitacoraTheme as _, BitacoraTheme, Theme, TypeStyleExt as _};
use crate::ui::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    StatefulInteractiveElement as _, Styled as _, Window, div, h_flex, px, v_flex,
};
use crate::views::dims;

/// An element built from the design theme at render time, so the helpers below keep their
/// `theme`-only signatures while styling from the design tokens.
#[derive(IntoElement)]
struct Themed(Box<dyn FnOnce(&BitacoraTheme) -> AnyElement>);

impl RenderOnce for Themed {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        (self.0)(cx.bitacora())
    }
}

fn themed(build: impl FnOnce(&BitacoraTheme) -> AnyElement + 'static) -> AnyElement {
    Themed(Box::new(build)).into_any_element()
}

/// A dimmed full-window layer with a centred card; `on_dismiss` runs for a click outside the
/// card. The card swallows its own clicks.
pub fn modal(
    id: &'static str,
    _theme: &Theme,
    width: f32,
    on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    content: impl IntoElement,
) -> AnyElement {
    let content = content.into_any_element();
    themed(move |t| {
        div()
            .id(id)
            .absolute()
            .inset_0()
            // Occlude the views below: the wheel scrolls the modal, never the page behind it.
            .occlude()
            .flex()
            .justify_center()
            .items_center()
            .bg(dims::SCRIM)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                on_dismiss(window, cx);
            })
            .child(
                v_flex()
                    .id((id, 1usize))
                    .role(crate::ui::a11y::Role::Dialog)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .w(px(width))
                    .max_w_full()
                    .max_h_full()
                    .bg(t.colors.raised)
                    .text_color(t.colors.text)
                    .type_style(&t.type_scale.ui)
                    .border_1()
                    .border_color(t.colors.line)
                    .rounded(t.metrics.radius_popover)
                    .shadow_lg()
                    .overflow_hidden()
                    .child(content),
            )
            .into_any_element()
    })
}

/// A muted caption above a value, used for the labelled rows of the panels.
pub fn labelled(_theme: &Theme, label: String, value: impl IntoElement) -> AnyElement {
    let value = value.into_any_element();
    themed(move |t| {
        v_flex()
            .gap_0p5()
            .child(
                div()
                    .type_style(&t.type_scale.caption)
                    .text_color(t.colors.muted)
                    .child(label),
            )
            .child(div().type_style(&t.type_scale.ui_small).child(value))
            .into_any_element()
    })
}

/// Title bar of a card.
pub fn title_bar(_theme: &Theme, title: String, trailing: impl IntoElement) -> AnyElement {
    let trailing = trailing.into_any_element();
    themed(move |t| {
        h_flex()
            .px(t.metrics.space[5])
            .py(t.metrics.space[4])
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(t.colors.line)
            .child(
                div()
                    .id("dialog-title")
                    .role(crate::ui::a11y::Role::Heading)
                    .aria_label(title.clone())
                    .flex_1()
                    .type_style(&t.type_scale.h3_block)
                    .text_color(t.colors.text)
                    .child(title),
            )
            .child(trailing)
            .into_any_element()
    })
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
