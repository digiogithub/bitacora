//! Read-only page view: renders a page's blocks (BIT-US-0074).
//!
//! A deliberately simple page host: it loads one page file, builds a [`PageModel`] and shows
//! the visible rows in a virtualized list. Clicking a ref, tag or link emits
//! [`PageEvent::Navigate`]; real navigation arrives with BIT-US-0075.

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use bitacora_markdown::properties::PropertyConfig;
use rust_i18n::t;

use crate::render::highlight::TokenClass;
use crate::render::inline::{Emphasis, ImageRef, NavTarget, Role, TextLayout};
use crate::render::model::{BlockModel, BodyItem, CodeBlock, PageModel, Row};
use crate::ui::text_edit::{
    FontStyle, FontWeight, HighlightStyle, InteractiveText, ListAlignment, ListState, ObjectFit,
    StrikethroughStyle, StyledText, UnderlineStyle, hsla, img, list,
};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, Context, EventEmitter, FluentBuilder as _, Hsla, IconName,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    StyledImage as _, Task, Window, div, h_flex, icon, px, v_flex,
};

/// Events emitted by the page view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    /// The user clicked a ref, tag or link.
    Navigate(NavTarget),
}

/// What the view currently shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LoadState {
    /// No page.
    #[default]
    Empty,
    /// The file is being read.
    Loading,
    /// The page is rendered.
    Loaded,
    /// Reading failed.
    Failed(String),
}

/// The read-only page view.
pub struct PageView {
    title: Option<String>,
    graph_root: Option<PathBuf>,
    model: PageModel,
    state: LoadState,
    list_state: ListState,
    cfg: PropertyConfig,
    load_task: Option<Task<()>>,
    rendered_rows: usize,
}

impl std::fmt::Debug for PageView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PageView")
            .field("title", &self.title)
            .field("rows", &self.model.rows.len())
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<PageEvent> for PageView {}

/// Resolves an image `src` (`../assets/a.png`) to a file inside the graph. Remote URLs and
/// paths that leave the graph folder give `None`.
pub fn resolve_asset(graph_root: &Path, src: &str) -> Option<PathBuf> {
    let src = src.trim();
    if src.is_empty() || src.contains("://") || src.starts_with("data:") {
        return None;
    }
    let src = src.strip_prefix("file:").unwrap_or(src);
    // Relative paths are written from a page in `pages/` or `journals/`.
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
}

impl PageView {
    /// An empty view.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            title: None,
            graph_root: None,
            model: PageModel::default(),
            state: LoadState::Empty,
            list_state: ListState::new(0, ListAlignment::Top, px(400.)),
            cfg: PropertyConfig::default(),
            load_task: None,
            rendered_rows: 0,
        }
    }

    /// Current load state.
    pub fn state(&self) -> &LoadState {
        &self.state
    }

    /// The rendered model.
    pub fn model(&self) -> &PageModel {
        &self.model
    }

    /// Page title shown above the blocks.
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// Sets the property settings (from `config.edn`) used when building models.
    pub fn set_property_config(&mut self, cfg: PropertyConfig) {
        self.cfg = cfg;
    }

    /// Reads `file` on a background thread and shows it.
    pub fn open_file(
        &mut self,
        graph_root: PathBuf,
        file: PathBuf,
        title: String,
        cx: &mut Context<Self>,
    ) {
        self.state = LoadState::Loading;
        self.title = Some(title.clone());
        self.graph_root = Some(graph_root);
        cx.notify();
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { std::fs::read(&file).map_err(|e| e.to_string()) })
                .await;
            // The view may have been closed meanwhile; nothing to update then.
            let _ = this.update(cx, |view, cx| match read {
                Ok(bytes) => view.set_source(&title, &bytes, cx),
                Err(message) => {
                    view.state = LoadState::Failed(message);
                    cx.notify();
                }
            });
        }));
    }

    /// Shows a page from its bytes.
    pub fn set_source(&mut self, title: &str, source: &[u8], cx: &mut Context<Self>) {
        self.title = Some(title.to_owned());
        let mut model = PageModel::from_source(source, &self.cfg, &crate::render::inline::NoBlocks);
        // Second pass: resolve block refs that point into this page.
        let known: HashMap<String, String> = model
            .rows
            .iter()
            .filter_map(|r| Some((r.block.id.clone()?, r.block.title.text.clone())))
            .collect();
        if !known.is_empty() {
            model = PageModel::from_source(source, &self.cfg, &known);
        }
        self.list_state.reset(model.rows.len());
        self.model = model;
        self.state = LoadState::Loaded;
        cx.notify();
    }

    /// How many rows have been drawn so far (tests and diagnostics).
    pub fn rendered_rows(&self) -> usize {
        self.rendered_rows
    }

    /// Reports a click on a rendered ref.
    pub fn activate(&mut self, target: NavTarget, cx: &mut Context<Self>) {
        cx.emit(PageEvent::Navigate(target));
    }

    fn render_row(&mut self, ix: usize, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.model.rows.get(ix) else {
            return div().into_any_element();
        };
        self.rendered_rows += 1;
        let this = cx.entity();
        let nav: Nav = std::rc::Rc::new(move |target: NavTarget, cx: &mut App| {
            this.update(cx, |view, cx| view.activate(target, cx));
        });
        render_block_row(ix, row, self.graph_root.as_deref(), cx.theme(), &nav)
    }
}

type Nav = std::rc::Rc<dyn Fn(NavTarget, &mut App)>;

fn heading_size(level: u8) -> f32 {
    [28., 24., 20., 18., 16., 15.][usize::from(level.clamp(1, 6)) - 1]
}

/// Highlight styles for a layout. With `strike_all` the gaps are struck through too.
fn highlights(
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

fn style_for(
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

fn text_element_owned(
    id: (&'static str, usize),
    layout: TextLayout,
    theme: &crate::ui::theme::Theme,
    strike_all: bool,
    on_nav: Option<Nav>,
) -> AnyElement {
    let styled = StyledText::new(SharedString::from(layout.text.clone()))
        .with_highlights(highlights(&layout, theme, strike_all));
    let ranges: Vec<Range<usize>> = layout.links.iter().map(|(r, _)| r.clone()).collect();
    let targets: Vec<NavTarget> = layout.links.iter().map(|(_, t)| t.clone()).collect();
    match on_nav {
        Some(nav) if !ranges.is_empty() => InteractiveText::new(id, styled)
            .on_click(ranges, move |ix, _, cx| {
                if let Some(target) = targets.get(ix) {
                    nav(target.clone(), cx);
                }
            })
            .into_any_element(),
        _ => styled.into_any_element(),
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

fn code_element(ix: usize, code: &CodeBlock, theme: &crate::ui::theme::Theme) -> AnyElement {
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

/// Draws one block row. Free function so tests and the fixture smoke test can render it.
fn render_block_row(
    ix: usize,
    row: &Row,
    graph_root: Option<&Path>,
    theme: &crate::ui::theme::Theme,
    dispatch: &Nav,
) -> AnyElement {
    let block: &BlockModel = &row.block;
    let finished = block.marker.as_ref().is_some_and(|m| m.finished);
    let strike = block.is_cancelled();
    let title_size = block.heading.map(heading_size);

    let mut title_line = h_flex().items_start().gap_2().flex_wrap();
    if let Some(marker) = &block.marker {
        if let Some(checked) = marker.checkbox {
            title_line = title_line.child(checkbox(checked, theme));
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
                ("title", ix),
                block.title.clone(),
                theme,
                strike,
                Some(dispatch.clone()),
            )),
    );
    let mut content = v_flex().flex_1().min_w_0().gap_1().child(title_line);
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
        let part = ix * 1000 + n;
        match item {
            BodyItem::Text(layout) => {
                content = content.child(text_element_owned(
                    ("body", part),
                    layout.clone(),
                    theme,
                    false,
                    Some(dispatch.clone()),
                ));
                for image in &layout.images {
                    content = content.child(image_element(image, graph_root, theme));
                }
            }
            BodyItem::Code(code) => content = content.child(code_element(part, code, theme)),
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
                        )),
                );
            }
        }
    }

    if !block.properties.is_empty() {
        let mut table = v_flex()
            .gap_0p5()
            .p_2()
            .rounded(px(6.))
            .bg(theme.secondary)
            .text_sm();
        for (n, prop) in block.properties.iter().enumerate() {
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
                        ("prop", ix * 1000 + n),
                        prop.value.clone(),
                        theme,
                        false,
                        Some(dispatch.clone()),
                    )),
            );
        }
        content = content.child(table);
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

    // Bullet: collapsed blocks with children get a ring.
    let bullet = div()
        .mt(px(7.))
        .size(px(6.))
        .flex_none()
        .rounded_full()
        .when(block.collapsed && row.has_children, |d| {
            d.border_2().border_color(theme.muted_foreground)
        })
        .when(!(block.collapsed && row.has_children), |d| {
            d.bg(theme.muted_foreground)
        });

    h_flex()
        .id(("block", ix))
        .w_full()
        .items_start()
        .gap_2()
        .py(px(2.))
        .pl(px(12. + row.depth as f32 * 24.))
        .pr(px(12.))
        .child(
            div()
                .w(px(12.))
                .flex_none()
                .flex()
                .justify_center()
                .when(row.block_index.is_some(), |d| d.child(bullet)),
        )
        .child(content)
        .into_any_element()
}

impl Render for PageView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let body: AnyElement = match &self.state {
            LoadState::Empty => centered(t!("panel.page_host.empty").to_string()),
            LoadState::Loading => centered(t!("page.loading").to_string()),
            LoadState::Failed(message) => {
                centered(t!("page.load_failed", error = message).to_string())
            }
            LoadState::Loaded => list(
                self.list_state.clone(),
                cx.processor(|this, ix: usize, window, cx| this.render_row(ix, window, cx)),
            )
            .size_full()
            .into_any_element(),
        };
        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .when_some(self.title.clone(), |d, title| {
                d.child(
                    div()
                        .px(px(24.))
                        .pt(px(16.))
                        .pb(px(8.))
                        .text_size(px(26.))
                        .font_weight(FontWeight::BOLD)
                        .child(title),
                )
            })
            .child(div().flex_1().min_h_0().child(body))
    }
}

fn centered(text: String) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Entity;
    use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
    use crate::{settings::AppSettings, theme};

    fn setup(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::ui::init(cx);
            theme::install(cx, AppSettings::default(), None);
        });
    }

    fn open(cx: &mut TestAppContext) -> (Entity<PageView>, &mut VisualTestContext) {
        cx.add_window_view(|_, cx| PageView::new(cx))
    }

    const SAMPLE: &str = "title:: T\n\n- TODO [#A] hi [[Bob]] #tag\n  SCHEDULED: <2024-05-01 Wed>\n- code\n  ```rust\n  fn x() {}\n  ```\n\t- child ![a](../assets/a.png)\n";

    #[gpui_test]
    fn shows_the_empty_state_then_renders_rows(cx: &mut TestAppContext) {
        setup(cx);
        let (view, cx) = open(cx);
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Empty
        );
        view.update(cx, |v, cx| v.set_source("T", SAMPLE.as_bytes(), cx));
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loaded
        );
        assert_eq!(view.read_with(cx, |v, _| v.model().rows.len()), 4);
        assert!(view.read_with(cx, |v, _| v.rendered_rows()) > 0);
    }

    #[gpui_test]
    fn clicking_a_ref_emits_navigation(cx: &mut TestAppContext) {
        setup(cx);
        let (view, cx) = open(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = events.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&view, move |_, event: &PageEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
        });
        view.update(cx, |v, cx| v.set_source("T", SAMPLE.as_bytes(), cx));
        // The title of the TODO block links to Bob and tag.
        let target = view.read_with(cx, |v, _| v.model().rows[1].block.title.links[0].1.clone());
        view.update(cx, |v, cx| v.activate(target, cx));
        assert_eq!(
            *events.borrow(),
            vec![PageEvent::Navigate(NavTarget::Page("Bob".into()))]
        );
    }

    #[gpui_test]
    fn open_file_loads_in_the_background(cx: &mut TestAppContext) {
        setup(cx);
        let tmp = tempfile::tempdir().expect("tmp");
        let file = tmp.path().join("page.md");
        std::fs::write(&file, SAMPLE).expect("write");
        let (view, cx) = open(cx);
        view.update(cx, |v, cx| {
            v.open_file(tmp.path().to_path_buf(), file.clone(), "Page".into(), cx);
        });
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loading
        );
        cx.executor().allow_parking();
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Loaded
        );
        view.update(cx, |v, cx| {
            v.open_file(
                tmp.path().to_path_buf(),
                tmp.path().join("missing.md"),
                "X".into(),
                cx,
            );
        });
        cx.run_until_parked();
        assert!(matches!(
            view.read_with(cx, |v, _| v.state().clone()),
            LoadState::Failed(_)
        ));
    }

    #[test]
    fn asset_paths_stay_inside_the_graph() {
        let root = Path::new("/g");
        assert_eq!(
            resolve_asset(root, "../assets/a.png"),
            Some(PathBuf::from("/g/assets/a.png"))
        );
        assert_eq!(
            resolve_asset(root, "assets/a.png"),
            Some(PathBuf::from("/g/pages/assets/a.png"))
        );
        assert_eq!(resolve_asset(root, "../../etc/passwd"), None);
        assert_eq!(resolve_asset(root, "/etc/passwd"), None);
        assert_eq!(resolve_asset(root, "https://x.org/a.png"), None);
        assert_eq!(resolve_asset(root, ""), None);
    }

    /// Every fixture page builds a model without panicking, and a sample of them is drawn.
    #[gpui_test]
    fn renders_every_fixture_page(cx: &mut TestAppContext) {
        setup(cx);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/graphs");
        let mut pages = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "md") {
                    pages.push(path);
                }
            }
        }
        pages.sort();
        assert!(pages.len() > 300, "fixtures found: {}", pages.len());
        let (view, cx) = open(cx);
        let mut total_rows = 0;
        for page in &pages {
            let bytes = std::fs::read(page).expect("read fixture");
            view.update(cx, |v, cx| v.set_source("fixture", &bytes, cx));
            total_rows += view.read_with(cx, |v, _| v.model().rows.len());
        }
        cx.run_until_parked();
        assert!(total_rows > 1000, "rows: {total_rows}");
        assert!(view.read_with(cx, |v, _| v.rendered_rows()) > 0);
    }
}
