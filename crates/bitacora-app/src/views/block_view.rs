//! Drawing of one block row, shared by the page view, the references and the journals feed.

use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use rust_i18n::t;

use crate::editor::dnd::{BlockDrag, DragPreview, indicator_left};
use crate::editor::row::{RowEdit, TextHook};
use crate::nav::OpenIn;
use crate::render::highlight::TokenClass;
use crate::render::inline::{Emphasis, ImageRef, NavTarget, Role, TextLayout};
use crate::render::model::{BlockModel, BodyItem, CodeBlock, PropertyRow, Row};
use crate::ui::text_edit::{
    FontStyle, FontWeight, HighlightStyle, InteractiveText, ObjectFit, StrikethroughStyle,
    StyledText, UnderlineStyle, hsla, img,
};
use crate::ui::{
    AnyElement, App, AppContext as _, FluentBuilder as _, Hsla, IconName, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window, div, h_flex, icon, px, v_flex,
};

/// Resolves an image `src` (`../assets/a.png`) to a file inside the graph. Remote URLs and
/// paths that leave the graph folder give `None`.
pub fn resolve_asset(graph_root: &Path, src: &str) -> Option<PathBuf> {
    let src = src.trim();
    if src.is_empty() || src.contains("://") || src.starts_with("data:") {
        return None;
    }
    let src = src.strip_prefix("file:").unwrap_or(src);
    // `@alias/...` links point to a configured asset directory: not resolved here.
    if src.starts_with('@') {
        return None;
    }
    // Relative paths are written from a page in `pages/` or `journals/`.
    let relative = (|| {
        let mut parts: Vec<std::ffi::OsString> = vec!["pages".into()];
        for component in Path::new(src).components() {
            match component {
                Component::Normal(n) => parts.push(n.to_owned()),
                Component::ParentDir => {
                    parts.pop()?;
                }
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        let mut out = graph_root.to_path_buf();
        out.extend(parts);
        Some(out)
    })();
    if relative.as_ref().is_some_and(|p| p.exists()) {
        return relative;
    }
    // Local asset links (`^[./]*assets`) fall back to `<root>/assets` whatever the depth of the
    // page that holds them (`../../assets/x.png` from `pages/sub/x.md`).
    if bitacora_core::assets::is_local_asset_link(src)
        && let Some(path) = bitacora_core::recycle::asset_path_from_link(src)
    {
        let fallback = path.to_fs_path(graph_root);
        if fallback.exists() {
            return Some(fallback);
        }
    }
    relative
}

pub type Nav = std::rc::Rc<dyn Fn(NavTarget, OpenIn, &mut App)>;

fn heading_size(level: u8) -> f32 {
    [28., 24., 20., 18., 16., 15.][usize::from(level.clamp(1, 6)) - 1]
}

/// Highlight styles for a layout. With `strike_all` the gaps are struck through too.
pub(crate) fn highlights(
    layout: &TextLayout,
    theme: &crate::ui::theme::Theme,
    strike_all: bool,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let strike = StrikethroughStyle {
        thickness: px(1.),
        color: None,
    };
    let mut out = Vec::new();
    let mut at = 0;
    for s in &layout.styled {
        if strike_all && s.range.start > at {
            out.push((
                at..s.range.start,
                HighlightStyle {
                    strikethrough: Some(strike),
                    ..Default::default()
                },
            ));
        }
        out.push((
            s.range.clone(),
            style_for(s.role, s.emphasis, theme, strike_all),
        ));
        at = s.range.end;
    }
    if strike_all && at < layout.text.len() {
        out.push((
            at..layout.text.len(),
            HighlightStyle {
                strikethrough: Some(strike),
                ..Default::default()
            },
        ));
    }
    out
}

pub(crate) fn style_for(
    role: Role,
    emphasis: Emphasis,
    theme: &crate::ui::theme::Theme,
    strike_all: bool,
) -> HighlightStyle {
    let mut style = HighlightStyle::default();
    match role {
        Role::Plain => {}
        Role::PageRef | Role::Tag | Role::Link => style.color = Some(theme.info),
        Role::BlockRef => {
            style.color = Some(theme.foreground);
            style.background_color = Some(theme.muted);
            style.underline = Some(UnderlineStyle {
                thickness: px(1.),
                color: Some(theme.muted_foreground),
                wavy: false,
            });
        }
        Role::BlockRefDangling => style.color = Some(theme.danger),
        Role::Code => style.background_color = Some(theme.muted),
        Role::Math | Role::Dim => {
            style.color = Some(theme.muted_foreground);
            style.font_style = Some(FontStyle::Italic);
        }
        Role::Placeholder => {
            style.color = Some(theme.muted_foreground);
            style.background_color = Some(theme.muted);
        }
    }
    if emphasis.bold {
        style.font_weight = Some(FontWeight::BOLD);
    }
    if emphasis.italic {
        style.font_style = Some(FontStyle::Italic);
    }
    if emphasis.strike || strike_all {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        });
    }
    if emphasis.highlight {
        style.background_color = Some(hsla(0.14, 0.9, 0.55, 0.35));
    }
    style
}

pub(crate) fn text_element_owned(
    id: (&'static str, usize),
    layout: TextLayout,
    theme: &crate::ui::theme::Theme,
    strike_all: bool,
    on_nav: Option<Nav>,
    on_text: Option<TextHook>,
) -> AnyElement {
    let styled = StyledText::new(SharedString::from(layout.text.clone()))
        .with_highlights(highlights(&layout, theme, strike_all));
    let glyphs = styled.layout().clone();
    let source = layout.clone();
    let ranges: Vec<Range<usize>> = layout.links.iter().map(|(r, _)| r.clone()).collect();
    let targets: Vec<NavTarget> = layout.links.iter().map(|(_, t)| t.clone()).collect();
    let element = match on_nav {
        Some(nav) if !ranges.is_empty() => InteractiveText::new(id, styled)
            .on_click(ranges, move |ix, window, cx| {
                if let Some(target) = targets.get(ix) {
                    nav(
                        target.clone(),
                        OpenIn::from_shift(window.modifiers().shift),
                        cx,
                    );
                }
            })
            .into_any_element(),
        _ => styled.into_any_element(),
    };
    // Click-to-caret: a press on the text enters edit mode at the matching source offset;
    // presses on links are left to the link.
    match on_text {
        Some(hook) => div()
            .w_full()
            .on_mouse_down(
                crate::ui::text_edit::MouseButton::Left,
                move |event, window, cx| {
                    let shown = glyphs
                        .index_for_position(event.position)
                        .unwrap_or_else(|nearest| nearest);
                    cx.stop_propagation();
                    if source.target_at(shown).is_some() && !event.modifiers.shift {
                        return;
                    }
                    // No source position (an empty block has no text): caret at the end.
                    let offset = source.source_offset(shown).unwrap_or(usize::MAX);
                    hook(offset, event.modifiers.shift, window, cx);
                },
            )
            .child(element)
            .into_any_element(),
        None => element,
    }
}

fn image_element(
    image: &ImageRef,
    graph_root: Option<&Path>,
    theme: &crate::ui::theme::Theme,
) -> AnyElement {
    let local = graph_root.and_then(|root| resolve_asset(root, &image.src));
    match local {
        Some(path) => {
            let mut el = img(path)
                .object_fit(ObjectFit::Contain)
                .max_w(px(640.))
                .rounded(px(4.));
            if let Some(w) = image.width {
                el = el.w(px(w));
            }
            if let Some(h) = image.height {
                el = el.h(px(h));
            }
            el.into_any_element()
        }
        None => h_flex()
            .gap_1()
            .text_color(theme.muted_foreground)
            .child(icon(IconName::FileText))
            .child(if image.alt.is_empty() {
                image.src.clone()
            } else {
                image.alt.clone()
            })
            .into_any_element(),
    }
}

pub(crate) fn code_element(
    ix: usize,
    code: &CodeBlock,
    theme: &crate::ui::theme::Theme,
) -> AnyElement {
    let styles: Vec<(Range<usize>, HighlightStyle)> = code
        .tokens
        .iter()
        .map(|(range, class)| {
            let color: Hsla = match class {
                TokenClass::Keyword => hsla(0.75, 0.65, 0.7, 1.),
                TokenClass::String => theme.success,
                TokenClass::Comment => theme.muted_foreground,
                TokenClass::Number => theme.warning,
            };
            (
                range.clone(),
                HighlightStyle {
                    color: Some(color),
                    font_style: (*class == TokenClass::Comment).then_some(FontStyle::Italic),
                    ..Default::default()
                },
            )
        })
        .collect();
    v_flex()
        .id(("code", ix))
        .w_full()
        .p_2()
        .rounded(px(6.))
        .bg(theme.muted)
        .border_1()
        .border_color(theme.border)
        .when_some(code.language.clone(), |d, lang| {
            d.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(lang),
            )
        })
        .child(
            div()
                .font_family(theme.mono_font_family.clone())
                .text_size(theme.mono_font_size)
                .child(
                    StyledText::new(SharedString::from(code.text.clone())).with_highlights(styles),
                ),
        )
        .into_any_element()
}

fn checkbox(checked: bool, theme: &crate::ui::theme::Theme) -> AnyElement {
    div()
        .mt(px(3.))
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .border_1()
        .border_color(if checked {
            theme.primary
        } else {
            theme.muted_foreground
        })
        .when(checked, |d| {
            d.bg(theme.primary)
                .text_color(theme.primary_foreground)
                .child(icon(IconName::Check).size(px(10.)))
        })
        .into_any_element()
}

fn badge(text: String, color: Hsla, theme: &crate::ui::theme::Theme) -> AnyElement {
    let _ = theme;
    div()
        .px(px(6.))
        .rounded(px(4.))
        .text_xs()
        .text_color(color)
        .border_1()
        .border_color(color)
        .child(text)
        .into_any_element()
}

/// Callback with window access (clicks on a bullet or a bubble).
pub type Action = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

/// Draws the widget at body index `n` of a row. The host view creates the widget entities
/// before the row is drawn (drawing has no `App` access), so this only wraps them.
pub type WidgetBuild = std::rc::Rc<dyn Fn(usize) -> AnyElement>;

/// What a row can do when clicked.
#[derive(Clone)]
pub struct RowActions {
    /// Open a ref, tag, link or block.
    pub nav: Nav,
    /// Flip the view-only collapse state (bullet and arrow).
    pub toggle: Option<Action>,
    /// Open or close the list of blocks referencing this block (count bubble).
    pub referrers: Option<Action>,
    /// The block got the focus (a click on it): the editing-block protection hook of core
    /// (BIT-T-0344) learns which block the user is on.
    pub focus: Option<Action>,
    /// Edit-mode state and click callbacks (the page outline editor, BIT-US-0030).
    pub edit: Option<RowEdit>,
    /// Draws the row's query and embed widgets; without it they show as their source text.
    pub widgets: Option<WidgetBuild>,
}

impl std::fmt::Debug for RowActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowActions").finish_non_exhaustive()
    }
}

impl RowActions {
    /// Actions that only navigate.
    pub fn nav_only(nav: Nav) -> Self {
        Self {
            nav,
            toggle: None,
            referrers: None,
            focus: None,
            edit: None,
            widgets: None,
        }
    }
}

/// Draws one block row. Free function so tests and the fixture smoke test can render it.
/// `id` must be unique among the rows drawn in one frame (it keys interactive state).
pub fn render_block_row(
    id: usize,
    row: &Row,
    graph_root: Option<&Path>,
    theme: &crate::ui::theme::Theme,
    actions: &RowActions,
) -> AnyElement {
    let block: &BlockModel = &row.block;
    let finished = block.marker.as_ref().is_some_and(|m| m.finished);
    let strike = block.is_cancelled();
    let title_size = block.heading.map(heading_size);
    let dispatch = &actions.nav;
    let edit = actions.edit.as_ref();
    let on_text: Option<TextHook> = edit.map(|e| e.on_text.clone());

    let mut title_line = h_flex().items_start().gap_2().flex_wrap();
    if let Some(marker) = &block.marker {
        if let Some(checked) = marker.checkbox {
            let mut cb = checkbox(checked, theme);
            if let Some(e) = edit {
                let hook = e.on_checkbox.clone();
                cb = div()
                    .id(("checkbox", id))
                    .cursor_pointer()
                    .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(move |_, window, cx| hook(window, cx))
                    .child(cb)
                    .into_any_element();
            }
            title_line = title_line.child(cb);
        }
        if marker.show_label {
            title_line = title_line.child(badge(
                marker.label.clone(),
                if marker.finished {
                    theme.muted_foreground
                } else {
                    theme.info
                },
                theme,
            ));
        }
    }
    if let Some(p) = block.priority {
        let color = match p {
            'A' => theme.danger,
            'B' => theme.warning,
            _ => theme.info,
        };
        title_line = title_line.child(badge(format!("[#{p}]"), color, theme));
    }
    title_line = title_line.child(
        div()
            .flex_1()
            .min_w_0()
            .when(finished, |d| d.text_color(theme.muted_foreground))
            .when_some(title_size, |d, s| {
                d.text_size(px(s)).font_weight(FontWeight::BOLD)
            })
            .child(text_element_owned(
                ("title", id),
                block.title.clone(),
                theme,
                strike,
                Some(dispatch.clone()),
                on_text.clone(),
            )),
    );
    // `min_h`: an empty block still has a line to click on.
    let mut content = v_flex().flex_1().min_w_0().min_h(px(22.)).gap_1();
    // A block that is only a query or an embed has no title line above its widget.
    let widget_only = block.title.is_empty()
        && block.marker.is_none()
        && block.priority.is_none()
        && matches!(block.body.first(), Some(BodyItem::Widget(_)))
        && actions.widgets.is_some();
    if !widget_only {
        content = content.child(title_line);
    }
    for image in &block.title.images {
        content = content.child(image_element(image, graph_root, theme));
    }

    if !block.planning.is_empty() {
        let mut chips = h_flex().gap_2().flex_wrap();
        for chip in &block.planning {
            chips = chips.child(
                h_flex()
                    .gap_1()
                    .px(px(6.))
                    .rounded(px(4.))
                    .bg(theme.secondary)
                    .text_xs()
                    .child(icon(IconName::Calendar).size(px(12.)))
                    .child(format!("{} {}", chip.keyword, chip.text)),
            );
        }
        content = content.child(chips);
    }

    for (n, item) in block.body.iter().enumerate() {
        let part = id * 1000 + n;
        match item {
            BodyItem::Text(layout) => {
                content = content.child(text_element_owned(
                    ("body", part),
                    layout.clone(),
                    theme,
                    false,
                    Some(dispatch.clone()),
                    on_text.clone(),
                ));
                for image in &layout.images {
                    content = content.child(image_element(image, graph_root, theme));
                }
            }
            BodyItem::Code(code) => content = content.child(code_element(part, code, theme)),
            BodyItem::Widget(widget) => {
                content = content.child(match &actions.widgets {
                    Some(build) => build(n),
                    None => widget_source(widget, theme),
                });
            }
            BodyItem::Quote(layout) => {
                content = content.child(
                    div()
                        .pl_3()
                        .border_l_2()
                        .border_color(theme.border)
                        .text_color(theme.muted_foreground)
                        .child(text_element_owned(
                            ("quote", part),
                            layout.clone(),
                            theme,
                            false,
                            Some(dispatch.clone()),
                            on_text.clone(),
                        )),
                );
            }
        }
    }

    if !block.properties.is_empty() {
        content = content.child(properties_table(
            id,
            &block.properties,
            theme,
            Some(dispatch.clone()),
        ));
    }

    if let Some(log) = &block.logbook {
        let summary = match &log.total {
            Some(total) => t!("page.logbook_total", count = log.entries, total = total),
            None => t!("page.logbook", count = log.entries),
        };
        content = content.child(
            h_flex()
                .gap_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(icon(IconName::ChevronRight).size(px(12.)))
                .child(summary.to_string()),
        );
    }

    if let Some(referrers) = &row.referrers {
        let mut list = v_flex()
            .gap_0p5()
            .pl_2()
            .border_l_2()
            .border_color(theme.border)
            .text_sm();
        for (n, r) in referrers.iter().enumerate() {
            let nav = dispatch.clone();
            let uuid = r.uuid.clone();
            list = list.child(
                h_flex()
                    .id(("referrer", id * 1000 + n))
                    .gap_2()
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        nav(
                            NavTarget::Block(uuid.clone()),
                            OpenIn::from_shift(window.modifiers().shift),
                            cx,
                        );
                    })
                    .child(
                        div()
                            .text_color(theme.muted_foreground)
                            .child(r.page.clone()),
                    )
                    .child(div().text_color(theme.info).child(r.title.clone())),
            );
        }
        content = content.child(list);
    }

    if let Some(e) = edit {
        if let Some(build) = &e.editing {
            let mut editing = v_flex().flex_1().min_w_0().gap_1();
            if let Some(conflict) = &e.conflict {
                editing = editing.child(conflict(theme));
            }
            content = editing.child(build(theme));
        } else {
            let hook = e.on_text.clone();
            content = content.on_mouse_down(
                crate::ui::text_edit::MouseButton::Left,
                move |event, window, cx| {
                    cx.stop_propagation();
                    hook(usize::MAX, event.modifiers.shift, window, cx);
                },
            );
        }
    }
    let toggle_hook: Option<Action> = edit
        .map(|e| e.on_toggle.clone())
        .or_else(|| actions.toggle.clone());
    let collapsed = row.is_collapsed();
    let bullet = div()
        .mt(px(7.))
        .size(px(6.))
        .flex_none()
        .rounded_full()
        .when(collapsed, |d| {
            d.border_2().border_color(theme.muted_foreground)
        })
        .when(!collapsed, |d| d.bg(theme.muted_foreground));
    let toggle_slot = div()
        .id(("toggle", id))
        .w(px(14.))
        .flex_none()
        .flex()
        .justify_center()
        .pt(px(4.))
        .text_color(theme.muted_foreground)
        .when(row.has_children, |d| {
            d.child(
                icon(if collapsed {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                })
                .size(px(12.)),
            )
        })
        .when_some(
            toggle_hook.clone().filter(|_| row.has_children),
            |d, toggle| {
                d.cursor_pointer()
                    .on_click(move |_, window, cx| toggle(window, cx))
            },
        );
    let bullet_uuid = row.uuid.clone();
    let bullet_slot = div()
        .id(("bullet", id))
        .w(px(12.))
        .flex_none()
        .flex()
        .justify_center()
        .when(row.block_index.is_some(), |d| d.child(bullet))
        .when(
            bullet_uuid.is_some() || (toggle_hook.is_some() && row.has_children) || edit.is_some(),
            |d| {
                let toggle = toggle_hook.clone().filter(|_| row.has_children);
                let zoom = edit.map(|e| e.on_bullet.clone());
                let nav = actions.nav.clone();
                let uuid = bullet_uuid.clone();
                // Shift+click opens the block in the right sidebar; a plain click zooms into
                // the block when the page is editable, otherwise it folds.
                d.cursor_pointer().on_click(move |_, window, cx| {
                    if window.modifiers().shift
                        && let Some(uuid) = &uuid
                    {
                        nav(NavTarget::Block(uuid.clone()), OpenIn::Sidebar, cx);
                    } else if let Some(zoom) = &zoom {
                        zoom(window, cx);
                    } else if let Some(toggle) = &toggle {
                        toggle(window, cx);
                    }
                })
            },
        );
    // Dragging the bullet moves the block (or the selection it belongs to; BIT-US-0106).
    let bullet_slot = bullet_slot.when_some(edit.map(|e| e.drag.payload.clone()), |d, payload| {
        d.on_drag(payload, |p: &BlockDrag, _, _, cx| {
            cx.new(|_| DragPreview::new(p))
        })
    });
    let drop_indicator = edit.and_then(|e| {
        e.drag.zone.map(|zone| {
            let line = div()
                .absolute()
                .left(indicator_left(zone, e.drag.depth))
                .right(px(12.))
                .h(px(2.))
                .bg(theme.primary);
            match zone {
                crate::editor::dnd::DropZone::Before => line.top_0(),
                _ => line.bottom_0(),
            }
        })
    });
    let bubble = (row.ref_count > 0).then(|| {
        div()
            .id(("refcount", id))
            .mt(px(2.))
            .px(px(6.))
            .h(px(18.))
            .flex_none()
            .rounded_full()
            .bg(if row.referrers.is_some() {
                theme.primary
            } else {
                theme.muted
            })
            .text_color(if row.referrers.is_some() {
                theme.primary_foreground
            } else {
                theme.muted_foreground
            })
            .text_xs()
            .child(row.ref_count.to_string())
            .when_some(actions.referrers.clone(), |d, open| {
                d.cursor_pointer()
                    .on_click(move |_, window, cx| open(window, cx))
            })
    });

    h_flex()
        .id(("block", id))
        .relative()
        .w_full()
        .items_start()
        .gap_1()
        .py(px(2.))
        .pl(px(8. + row.depth as f32 * 24.))
        .pr(px(12.))
        .when(edit.is_some_and(|e| e.selected), |d| d.bg(theme.selection))
        .when_some(edit.map(|e| e.on_drop.clone()), |d, drop| {
            d.on_drop(
                move |paths: &crate::ui::text_edit::ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    drop(paths.paths(), window, cx);
                },
            )
        })
        .when_some(edit.map(|e| e.drag.clone()), |d, drag| {
            let on_move = drag.on_move.clone();
            let on_drop = drag.on_drop.clone();
            d.on_drag_move::<BlockDrag>(move |e, window, cx| {
                let payload = e.drag(cx).clone();
                on_move(&payload, e.event.position, e.bounds, window, cx);
            })
            .on_drop::<BlockDrag>(move |payload, window, cx| {
                cx.stop_propagation();
                on_drop(payload, window, cx);
            })
        })
        .when_some(edit.map(|e| e.on_drag.clone()), |d, drag| {
            d.on_mouse_move(move |event, window, cx| {
                if event.dragging() {
                    drag(window, cx);
                }
            })
        })
        .when_some(actions.focus.clone(), |d, focus| {
            d.on_mouse_down(
                crate::ui::text_edit::MouseButton::Left,
                move |_, window, cx| {
                    focus(window, cx);
                },
            )
        })
        .child(toggle_slot)
        .child(bullet_slot)
        .child(content)
        .children(bubble)
        .children(drop_indicator)
        .children(edit.and_then(|e| e.on_delete_asset.clone()).map(|delete| {
            div()
                .id(("delete-asset", id))
                .flex_none()
                .mt(px(2.))
                .px_1()
                .rounded(px(4.))
                .cursor_pointer()
                .text_color(theme.muted_foreground)
                .hover(|d| d.bg(theme.muted))
                .child(icon(IconName::Delete))
                .on_mouse_down(crate::ui::text_edit::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| delete(window, cx))
        }))
        .into_any_element()
}

/// A widget drawn as its source (no host view to run it): rows inside query results and
/// references.
fn widget_source(
    widget: &crate::render::widget::Widget,
    theme: &crate::ui::theme::Theme,
) -> AnyElement {
    use crate::render::widget::{EmbedTarget, Widget};
    let text = match widget {
        Widget::Query(q) => format!("{{{{query {}}}}}", q.source.lines().next().unwrap_or("")),
        Widget::Embed(EmbedTarget::Block(u)) => format!("{{{{embed (({u}))}}}}"),
        Widget::Embed(EmbedTarget::Page(p)) => format!("{{{{embed [[{p}]]}}}}"),
    };
    div()
        .px(px(6.))
        .rounded(px(4.))
        .bg(theme.muted)
        .text_color(theme.muted_foreground)
        .text_sm()
        .child(text)
        .into_any_element()
}

/// The properties table of a block or page.
pub fn properties_table(
    id: usize,
    properties: &[PropertyRow],
    theme: &crate::ui::theme::Theme,
    nav: Option<Nav>,
) -> AnyElement {
    let mut table = v_flex()
        .gap_0p5()
        .p_2()
        .rounded(px(6.))
        .bg(theme.secondary)
        .text_sm();
    for (n, prop) in properties.iter().enumerate() {
        table = table.child(
            h_flex()
                .gap_3()
                .child(
                    div()
                        .w(px(110.))
                        .flex_none()
                        .text_color(theme.muted_foreground)
                        .child(prop.key.clone()),
                )
                .child(text_element_owned(
                    ("prop", id * 1000 + n),
                    prop.value.clone(),
                    theme,
                    false,
                    nav.clone(),
                    None,
                )),
        );
    }
    table.into_any_element()
}
