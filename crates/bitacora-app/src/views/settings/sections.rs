//! General, Editor, Search & Index, Sync and Appearance sections.

use bitacora_config::{NameFormat, PreferredWorkflow};
use rust_i18n::t;

use super::{
    AppKey, ApplyMode, Field, GraphEdit, Pending, SettingsEvent, SettingsView, field, mode_badge,
    row,
};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::menu::DropdownMenu as _;
use crate::ui::switch::Switch;
use crate::ui::theme::Theme;
use crate::ui::{
    AnyElement, ClickEvent, Context, Disableable as _, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, Level, ParentElement as _, Sizable as _, Styled as _,
    div, h_flex, v_flex,
};

impl SettingsView {
    fn graph_commit(
        &mut self,
        edit: GraphEdit,
        window: &mut crate::ui::Window,
        cx: &mut Context<Self>,
    ) {
        self.request_graph(vec![edit], window, cx);
    }

    pub(crate) fn commit_listener(
        &self,
        which: Field,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut crate::ui::Window, &mut crate::ui::App) + 'static {
        cx.listener(move |this, _: &ClickEvent, window, cx| this.commit(which, window, cx))
    }

    pub(crate) fn render_general(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(values) = self.graph.clone() else {
            return self.no_graph(theme);
        };
        let root = self
            .ctx
            .root
            .as_ref()
            .map_or_else(String::new, |r| r.display().to_string());
        let today = crate::data::today_local();
        let preview = |input: &crate::ui::Entity<crate::ui::input::InputState>,
                       cx: &crate::ui::App| {
            let pattern = input.read(cx).value().to_string();
            match today {
                Some(day) => super::date_preview(&pattern, day)
                    .map(|p| t!("settings.general.preview", value = p).to_string())
                    .unwrap_or_else(|| t!("settings.general.preview_invalid").to_string()),
                None => String::new(),
            }
        };
        let title_preview = preview(&self.inputs.title_format, cx);
        let file_preview = preview(&self.inputs.file_format, cx);
        let name_format = values.name_format;
        let app = self.app(cx);

        let name_format_buttons = h_flex()
            .gap_1()
            .child(
                Button::new("settings-name-legacy")
                    .small()
                    .label("legacy")
                    .when(name_format == NameFormat::Legacy, |b| b.primary())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.graph_commit(GraphEdit::NameFormat(NameFormat::Legacy), window, cx);
                    })),
            )
            .child(
                Button::new("settings-name-triple")
                    .small()
                    .label("triple-lowbar")
                    .when(name_format == NameFormat::TripleLowbar, |b| b.primary())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.graph_commit(
                            GraphEdit::NameFormat(NameFormat::TripleLowbar),
                            window,
                            cx,
                        );
                    })),
            );

        let favorites = values.favorites.clone();
        v_flex()
            .gap_1()
            .child(row(
                theme,
                t!("settings.general.graph_path").to_string(),
                Some(root),
                div().id("settings-graph-path"),
            ))
            .child(row(
                theme,
                t!("settings.general.name_format").to_string(),
                Some(t!("settings.general.name_format_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, ApplyMode::ReindexRequired))
                    .child(name_format_buttons),
            ))
            .child(row(
                theme,
                t!("settings.general.title_format").to_string(),
                Some(format!(
                    "{} {title_preview}",
                    t!("settings.general.title_format_help")
                )),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, ApplyMode::ReindexRequired))
                    .child(field(
                        "settings-title-format",
                        &self.inputs.title_format,
                        200.,
                        self.commit_listener(Field::TitleFormat, cx),
                    )),
            ))
            .child(row(
                theme,
                t!("settings.general.file_format").to_string(),
                Some(format!(
                    "{} {file_preview}",
                    t!("settings.general.file_format_help")
                )),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, ApplyMode::ReindexRequired))
                    .child(field(
                        "settings-file-format",
                        &self.inputs.file_format,
                        200.,
                        self.commit_listener(Field::FileFormat, cx),
                    )),
            ))
            .child(row(
                theme,
                t!("settings.general.hidden").to_string(),
                Some(t!("settings.general.hidden_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, ApplyMode::ReindexRequired))
                    .child(field(
                        "settings-hidden",
                        &self.inputs.hidden,
                        240.,
                        self.commit_listener(Field::Hidden, cx),
                    )),
            ))
            .child(row(
                theme,
                t!("settings.general.favorites").to_string(),
                Some(t!("settings.general.favorites_help").to_string()),
                field(
                    "settings-favorite-add",
                    &self.inputs.favorite,
                    200.,
                    self.commit_listener(Field::Favorite, cx),
                ),
            ))
            .child(
                h_flex()
                    .id("settings-favorites")
                    .flex_wrap()
                    .gap_1()
                    .children(favorites.into_iter().enumerate().map(|(ix, page)| {
                        let name = page.clone();
                        Button::new(("settings-favorite", ix))
                            .small()
                            .outline()
                            .icon(IconName::Close)
                            .label(page)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.graph_commit(
                                    GraphEdit::FavoriteRemove(name.clone()),
                                    window,
                                    cx,
                                );
                            }))
                    })),
            )
            .child(row(
                theme,
                t!("settings.general.background").to_string(),
                Some(t!("settings.general.background_help").to_string()),
                Switch::new("settings-background")
                    .checked(app.keep_running_in_background)
                    .on_click(cx.listener(|this, on: &bool, window, cx| {
                        let on = *on;
                        crate::theme::edit_settings(cx, Some(window), |s| {
                            s.keep_running_in_background = on;
                        });
                        this.say(Level::Success, t!("settings.saved").to_string(), cx);
                    })),
            ))
            .child(row(
                theme,
                t!("settings.general.updates").to_string(),
                Some(t!("settings.general.updates_help").to_string()),
                Switch::new("settings-updates")
                    .checked(app.updates.enabled)
                    .on_click(cx.listener(|this, on: &bool, window, cx| {
                        let on = *on;
                        crate::theme::edit_settings(cx, Some(window), |s| s.updates.enabled = on);
                        this.say(Level::Success, t!("settings.saved").to_string(), cx);
                    })),
            ))
            .into_any_element()
    }

    pub(crate) fn render_editor(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let Some(values) = self.graph.clone() else {
            return self.no_graph(theme);
        };
        let workflow = values.workflow;
        v_flex()
            .gap_1()
            .child(row(
                theme,
                t!("settings.editor.workflow").to_string(),
                Some(t!("settings.editor.workflow_help").to_string()),
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("settings-workflow-now")
                            .small()
                            .label("NOW / LATER")
                            .when(workflow == PreferredWorkflow::Now, |b| b.primary())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.graph_commit(
                                    GraphEdit::PreferredWorkflow(PreferredWorkflow::Now),
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        Button::new("settings-workflow-todo")
                            .small()
                            .label("TODO / DOING")
                            .when(workflow == PreferredWorkflow::Todo, |b| b.primary())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.graph_commit(
                                    GraphEdit::PreferredWorkflow(PreferredWorkflow::Todo),
                                    window,
                                    cx,
                                );
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.editor.template").to_string(),
                Some(t!("settings.editor.template_help").to_string()),
                field(
                    "settings-template",
                    &self.inputs.template,
                    240.,
                    self.commit_listener(Field::Template, cx),
                ),
            ))
            .into_any_element()
    }

    pub(crate) fn render_search(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let on = self.app(cx).search.substring;
        v_flex()
            .gap_1()
            .child(row(
                theme,
                t!("settings.search.substring").to_string(),
                Some(t!("settings.search.substring_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, AppKey::SearchSubstring.apply_mode()))
                    .child(
                        Switch::new("settings-substring")
                            .checked(on)
                            .on_click(cx.listener(|this, on: &bool, window, cx| {
                                this.request_substring(*on, window, cx);
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.search.reindex").to_string(),
                Some(t!("settings.search.reindex_help").to_string()),
                Button::new("settings-reindex")
                    .small()
                    .disabled(self.ctx.root.is_none())
                    .icon(IconName::RefreshCw)
                    .label(t!("settings.search.reindex_button").to_string())
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                        cx.emit(SettingsEvent::Reindex);
                    })),
            ))
            .into_any_element()
    }

    pub(crate) fn render_sync(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let prefs = self.sync_prefs.clone();
        let status = match &self.sync_view {
            Some(view) => view.message.clone(),
            None if prefs.enabled => t!("sync.panel.starting").to_string(),
            None => t!("sync.panel.off").to_string(),
        };
        let remote = if prefs.remote_url.is_empty() {
            t!("sync.panel.none").to_string()
        } else {
            prefs.remote_url.clone()
        };
        let squash = self.squash();
        v_flex()
            .gap_1()
            .child(row(
                theme,
                t!("settings.sync.status").to_string(),
                Some(status),
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("settings-sync-now")
                            .small()
                            .primary()
                            .icon(IconName::RefreshCw)
                            .disabled(self.sync_view.is_none())
                            .label(t!("sync.panel.sync_now").to_string())
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.emit(SettingsEvent::SyncNow);
                            })),
                    )
                    .child(
                        Button::new("settings-sync-panel")
                            .small()
                            .label(t!("settings.sync.details").to_string())
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.emit(SettingsEvent::OpenSyncPanel);
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.sync.remote").to_string(),
                Some(format!("{} \u{b7} {}", prefs.branch, prefs.device)),
                div().id("settings-sync-remote").text_sm().child(remote),
            ))
            .child(row(
                theme,
                t!("settings.sync.enabled").to_string(),
                Some(t!("settings.sync.enabled_help").to_string()),
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("settings-sync-enable")
                            .small()
                            .disabled(self.ctx.root.is_none())
                            .label(if prefs.enabled {
                                t!("sync.panel.change").to_string()
                            } else {
                                t!("sync.panel.enable").to_string()
                            })
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                cx.emit(SettingsEvent::EnableSync);
                            })),
                    )
                    .when(prefs.enabled, |r| {
                        r.child(
                            Button::new("settings-sync-disable")
                                .small()
                                .label(t!("sync.panel.disable").to_string())
                                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                                    cx.emit(SettingsEvent::DisableSync);
                                })),
                        )
                    }),
            ))
            .child(row(
                theme,
                t!("settings.sync.timing").to_string(),
                Some(t!("settings.sync.timing_help").to_string()),
                h_flex()
                    .gap_2()
                    .items_center()
                    .children(mode_badge(theme, AppKey::SyncTiming.apply_mode()))
                    .child(
                        div()
                            .w(crate::ui::px(64.))
                            .child(crate::ui::input::Input::new(&self.inputs.sync_idle)),
                    )
                    .child(
                        div()
                            .w(crate::ui::px(64.))
                            .child(crate::ui::input::Input::new(&self.inputs.sync_max)),
                    )
                    .child(
                        div()
                            .w(crate::ui::px(64.))
                            .child(crate::ui::input::Input::new(&self.inputs.sync_fetch)),
                    )
                    .child(
                        Button::new("settings-sync-timing")
                            .small()
                            .label(t!("settings.apply").to_string())
                            .on_click(self.commit_listener(Field::SyncTiming, cx)),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.sync.squash").to_string(),
                Some(t!("settings.sync.squash_help").to_string()),
                Switch::new("settings-sync-squash")
                    .checked(squash)
                    .on_click(cx.listener(|this, on: &bool, _, cx| this.toggle_squash(*on, cx))),
            ))
            .into_any_element()
    }

    pub(crate) fn render_appearance(
        &mut self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::settings::ThemePreference;
        let app = self.app(cx);
        let mode_button =
            |id: &'static str, label: String, pref: ThemePreference, cx: &mut Context<Self>| {
                Button::new(id)
                    .small()
                    .label(label)
                    .when(app.mode == pref, |b| b.primary())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        crate::theme::set_preference(cx, Some(window), pref);
                        this.say(Level::Success, t!("settings.saved").to_string(), cx);
                    }))
            };
        let size = app.font_size.map_or_else(
            || t!("settings.appearance.font_default").to_string(),
            |s| format!("{s}"),
        );
        v_flex()
            .gap_1()
            .child(row(
                theme,
                t!("settings.appearance.mode").to_string(),
                None,
                h_flex()
                    .gap_1()
                    .child(mode_button(
                        "settings-mode-system",
                        t!("theme.system").to_string(),
                        ThemePreference::System,
                        cx,
                    ))
                    .child(mode_button(
                        "settings-mode-light",
                        t!("theme.light").to_string(),
                        ThemePreference::Light,
                        cx,
                    ))
                    .child(mode_button(
                        "settings-mode-dark",
                        t!("theme.dark").to_string(),
                        ThemePreference::Dark,
                        cx,
                    )),
            ))
            .child(row(
                theme,
                t!("settings.appearance.theme").to_string(),
                Some(t!("settings.appearance.theme_help").to_string()),
                Button::new("settings-theme-menu")
                    .small()
                    .icon(IconName::Palette)
                    .label(t!("status.theme_button").to_string())
                    .dropdown_menu(|menu, _, cx| crate::theme::build_menu(menu, cx)),
            ))
            .child(row(
                theme,
                t!("settings.appearance.font_size").to_string(),
                Some(format!(
                    "{} {size}",
                    t!("settings.appearance.font_size_help")
                )),
                field(
                    "settings-font-size",
                    &self.inputs.font_size,
                    80.,
                    self.commit_listener(Field::FontSize, cx),
                ),
            ))
            .child(row(
                theme,
                t!("settings.appearance.font_reset").to_string(),
                None,
                Button::new("settings-font-reset")
                    .small()
                    .label(t!("settings.appearance.font_reset_button").to_string())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        crate::theme::edit_settings(cx, Some(window), |s| s.font_size = None);
                        this.say(Level::Success, t!("settings.saved").to_string(), cx);
                    })),
            ))
            .into_any_element()
    }

    pub(crate) fn no_graph(&self, theme: &Theme) -> AnyElement {
        div()
            .id("settings-no-graph")
            .p_4()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(t!("settings.no_graph").to_string())
            .into_any_element()
    }

    /// Asks for confirmation before `pending` (used by sections that cannot reach `request`).
    pub(crate) fn ask(
        &mut self,
        pending: Pending,
        window: &mut crate::ui::Window,
        cx: &mut Context<Self>,
    ) {
        self.request(pending, window, cx);
    }
}
