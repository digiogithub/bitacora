//! Drawing of the AI help of the editor (BIT-US-0153): the hint bar under a ghost continuation
//! and the "Compose with AI" box. Colours, sizes and type come from the design-system theme
//! (`ai`, `ai_bg`, `ai_line`); the buttons, chips and popover are the component kit's.

use super::ai::{ComposePhase, ComposeView};
use super::view::OutlineEditor;
use crate::ui::input::Input;
use crate::ui::text_edit::MouseButton;
use crate::ui::theme::{BitacoraTheme, TypeStyleExt as _};
use crate::ui::{
    AnyElement, App, Entity, FluentBuilder as _, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Styled as _, Window, deferred, div, h_flex, px, v_flex,
};
use crate::views::kit::{Button, Chip, ChipTone, Glyph, Kbd, PopoverShell, glyph};
use rust_i18n::t;

/// Key hint of the compose shortcut for the current platform.
const COMPOSE_KEYS: &str = if cfg!(target_os = "macos") {
    "\u{2318}J"
} else {
    "Ctrl J"
};

/// "✦ Suggestion · Tab accept · Esc dismiss", shown under the block while a ghost is visible.
pub fn hint_bar(design: &BitacoraTheme) -> AnyElement {
    let c = &design.colors;
    h_flex()
        .id("ai-ghost-hint")
        .gap(design.metrics.space[4])
        .items_center()
        .pt(design.metrics.space[2])
        .type_style(&design.type_scale.caption)
        .text_color(c.muted)
        .child(
            div()
                .text_color(c.ai)
                .font_weight(crate::ui::text_edit::FontWeight(600.))
                .child(format!("\u{2726} {}", t!("editor.ai.suggestion"))),
        )
        .child(
            h_flex()
                .gap(design.metrics.space[2])
                .items_center()
                .child(Kbd::new("Tab"))
                .child(t!("editor.ai.accept").to_string()),
        )
        .child(
            h_flex()
                .gap(design.metrics.space[2])
                .items_center()
                .child(Kbd::new("Esc"))
                .child(t!("editor.ai.dismiss").to_string()),
        )
        .into_any_element()
}

/// The compose box, floating under the edited block.
pub fn compose_box(
    editor: Entity<OutlineEditor>,
    view: ComposeView,
    design: BitacoraTheme,
) -> AnyElement {
    let panel = div()
        .absolute()
        .top_full()
        .left(px(0.))
        .mt_1()
        .child(ComposeBox {
            editor,
            view,
            design,
        });
    deferred(panel).with_priority(2).into_any_element()
}

#[derive(IntoElement)]
struct ComposeBox {
    editor: Entity<OutlineEditor>,
    view: ComposeView,
    design: BitacoraTheme,
}

impl RenderOnce for ComposeBox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            editor,
            view,
            design,
        } = self;
        let c = design.colors;
        let m = design.metrics.clone();
        // The popover shell takes focus when it first renders; the field takes it back on the
        // next frame, once it is part of the focus tree.
        if view.focus_pending.replace(false) {
            let input = view.input.clone();
            window.on_next_frame(move |window, cx| {
                input.update(cx, |i, cx| i.focus(window, cx));
            });
        }
        let running = view.phase == ComposePhase::Running;
        let ready = view.phase == ComposePhase::Ready;
        let has_instruction = !view.input.read(cx).value().trim().is_empty();

        let header = h_flex()
            .gap(m.space[4])
            .items_center()
            .pb(m.space[4])
            .border_b_1()
            .border_color(c.ai_line)
            .child(glyph(Glyph::Sparkle, px(16.), c.ai, cx))
            .child(
                div()
                    .flex_1()
                    .type_style(&design.type_scale.ui)
                    .child(Input::new(&view.input)),
            )
            .child(Kbd::new(COMPOSE_KEYS));

        let toggle = editor.clone();
        let chip = div()
            .id("ai-context-block")
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                toggle.update(cx, |this, cx| this.compose_toggle_block(cx));
            })
            .child(
                Chip::new(t!("editor.ai.this_block").to_string()).tone(if view.include_block {
                    ChipTone::Ai
                } else {
                    ChipTone::Dashed
                }),
            );
        let context = h_flex()
            .gap(m.space[3])
            .items_center()
            .pt(m.space[4])
            .type_style(&design.type_scale.caption)
            .text_color(c.muted)
            .child(t!("editor.ai.context").to_string())
            .child(chip)
            .child(Chip::new(view.page.clone()).tone(ChipTone::Neutral));

        let body = match &view.phase {
            ComposePhase::Failed(err) => div()
                .pt(m.space[4])
                .type_style(&design.type_scale.ui_small)
                .text_color(c.warn)
                .child(err.clone())
                .into_any_element(),
            _ if !view.preview.is_empty() => div()
                .id("ai-compose-draft")
                .pt(m.space[4])
                .type_style(&design.type_scale.body)
                .text_color(c.ai)
                .child(view.preview.clone())
                .when(running, |d| {
                    d.child(div().text_color(c.ai).child("\u{258d}"))
                })
                .into_any_element(),
            _ => div().into_any_element(),
        };

        let insert = editor.clone();
        let replace = editor.clone();
        let retry = editor.clone();
        let discard = editor.clone();
        let send = editor.clone();
        let actions = h_flex()
            .gap(m.space[3])
            .items_center()
            .pt(m.space[4])
            .child(if ready {
                Button::new("ai-insert")
                    .ai()
                    .compact()
                    .label(t!("editor.ai.insert_below").to_string())
                    .on_click(move |_, window, cx| {
                        insert.update(cx, |this, cx| this.compose_insert_below(window, cx));
                    })
            } else {
                Button::new("ai-generate")
                    .ai()
                    .compact()
                    .disabled(running || !has_instruction)
                    .label(t!("editor.ai.generate").to_string())
                    .on_click(move |_, window, cx| {
                        send.update(cx, |this, cx| this.compose_submit(window, cx));
                    })
            })
            .when(ready, |d| {
                d.child(
                    Button::new("ai-replace")
                        .secondary()
                        .compact()
                        .label(t!("editor.ai.replace").to_string())
                        .on_click(move |_, window, cx| {
                            replace.update(cx, |this, cx| this.compose_replace(window, cx));
                        }),
                )
            })
            .when(
                ready || matches!(view.phase, ComposePhase::Failed(_)),
                |d| {
                    d.child(
                        Button::new("ai-retry")
                            .secondary()
                            .compact()
                            .label(t!("editor.ai.retry").to_string())
                            .on_click(move |_, window, cx| {
                                retry.update(cx, |this, cx| this.compose_submit(window, cx));
                            }),
                    )
                },
            )
            .child(div().flex_1())
            .child(
                Button::new("ai-discard")
                    .ghost()
                    .compact()
                    .label(t!("editor.ai.discard").to_string())
                    .on_click(move |_, window, cx| {
                        discard.update(cx, |this, cx| this.compose_discard(window, cx));
                    }),
            );

        let dismiss = editor;
        PopoverShell::new("ai-compose")
            .width(px(560.))
            .on_dismiss(move |window, cx| {
                dismiss.update(cx, |this, cx| this.compose_discard(window, cx));
            })
            .child(
                v_flex()
                    .key_context(super::actions::context::COMPOSE_BOX)
                    .rounded(m.radius_popover)
                    .bg(c.ai_bg)
                    .border_1()
                    .border_color(c.ai_line)
                    .p(m.space[5])
                    .child(header)
                    .child(context)
                    .child(body)
                    .child(actions),
            )
    }
}
