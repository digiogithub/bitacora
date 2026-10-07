//! The settings view (BIT-US-0107): one overlay with sections for the graph (`config.edn`),
//! the editor, search and index, sync, agents (MCP), appearance and the keymap.
//!
//! * App-level options live in `settings.json` ([`crate::settings::AppSettings`], edited through
//!   [`crate::theme::edit_settings`]).
//! * Graph options are edits of `logseq/config.edn` ([`GraphEdit`]), comment-preserving and
//!   written through the command queue.
//! * Every setting declares an [`ApplyMode`]. Live ones apply at once; the ones that rebuild the
//!   index ask first (a [`Pending`] confirmation) and ones that need a restart say so.
//!
//! The view never talks to the workspace directly: it emits [`SettingsEvent`]s.

mod agents;
mod graph_config;
mod keymap;
mod keymap_model;
mod model;
mod pando;
mod sections;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::Arc;

use bitacora_core::queue::CommandQueue;
use bitacora_mcp::{TokenStore, WritePolicy};
use bitacora_runtime::SyncStatusView;
use rust_i18n::t;

pub use graph_config::{
    ApplyMode, Edited, GraphConfigError, GraphEdit, GraphValues, date_preview, edit_config,
    validate_date_format,
};
pub use keymap_model::{KeymapModel, KeymapRow, save_user_keymap};
pub use model::AppKey;
pub(crate) use pando::agent_configured_for;
pub use pando::{ConnTest, LiveStatus};

use crate::session::SessionHandle;
use crate::settings::AppSettings;
use crate::sync_prefs::SyncPrefs;
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::input::{Input, InputEvent, InputState};
use crate::ui::text_edit::FontWeight;
use crate::ui::theme::{ActiveBitacoraTheme as _, TypeStyleExt as _};
use crate::ui::{
    ActiveTheme as _, AnyElement, App, AppContext as _, ClickEvent, Confirmation, Context, Entity,
    EventEmitter, FluentBuilder as _, FocusHandle, Focusable, IconName, InteractiveElement as _,
    IntoElement, KeyDownEvent, Level, ParentElement as _, Render, Sizable as _,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, h_flex, notify,
    v_flex,
};
use crate::views::kit::{Glyph, Overline, glyph};
use crate::views::modal::{modal, title_bar};

/// A section of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// Graph options and app behaviour.
    General,
    /// Editor behaviour of the graph (workflow, templates).
    Editor,
    /// Search and index.
    Search,
    /// Git sync.
    Sync,
    /// MCP server: tokens, toggles, protected pages.
    Agents,
    /// Pando (AI): connection, features, per-graph consent and exclusions.
    Pando,
    /// Theme and font size.
    Appearance,
    /// Keyboard shortcuts.
    Keymap,
}

impl Section {
    /// Every section in display order.
    pub const ALL: [Self; 8] = [
        Self::General,
        Self::Editor,
        Self::Search,
        Self::Sync,
        Self::Agents,
        Self::Pando,
        Self::Appearance,
        Self::Keymap,
    ];

    /// The navigation groups: a localisation key for the group label and its sections. A new
    /// section is one enum variant plus an entry here and an arm in `render`.
    pub const GROUPS: [(&'static str, &'static [Self]); 2] = [
        (
            "settings.group.graph",
            &[Self::General, Self::Editor, Self::Search, Self::Sync],
        ),
        (
            "settings.group.app",
            &[Self::Agents, Self::Pando, Self::Appearance, Self::Keymap],
        ),
    ];

    /// Localised title.
    pub fn title(self) -> String {
        match self {
            Self::General => t!("settings.section.general"),
            Self::Editor => t!("settings.section.editor"),
            Self::Search => t!("settings.section.search"),
            Self::Sync => t!("settings.section.sync"),
            Self::Agents => t!("settings.section.agents"),
            Self::Pando => t!("settings.section.pando"),
            Self::Appearance => t!("settings.section.appearance"),
            Self::Keymap => t!("settings.section.keymap"),
        }
        .to_string()
    }

    fn icon(self) -> Glyph {
        match self {
            Self::General => Glyph::Settings,
            Self::Editor => Glyph::File,
            Self::Search => Glyph::Search,
            Self::Sync => Glyph::Globe,
            Self::Agents => Glyph::Network,
            Self::Pando => Glyph::Sparkle,
            Self::Appearance => Glyph::Sun,
            Self::Keymap => Glyph::Pin,
        }
    }
}

/// What the settings ask the workspace to do.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEvent {
    /// The view closed.
    Closed,
    /// Rebuild the index and reopen the graph (an index-affecting key changed).
    Reindex,
    /// Reopen the graph so the session picks up changed MCP or sync options.
    ReopenGraph,
    /// `:favorites` changed.
    FavoritesChanged(Vec<String>),
    /// The Pando settings file changed (consent, endpoints, features...). `reopen` asks the
    /// workspace to reopen the graph so the session starts with them.
    PandoChanged {
        /// Reopen the graph.
        reopen: bool,
    },
    /// Commit, fetch, merge and push now.
    SyncNow,
    /// Open the "Enable sync" form.
    EnableSync,
    /// Turn background sync off.
    DisableSync,
    /// Open the sync panel (backend details, conflicts, history).
    OpenSyncPanel,
    /// New sync timing; the workspace saves it and restarts the session.
    SyncTiming {
        /// Seconds without edits before an auto commit.
        idle: u64,
        /// Longest wait before an auto commit.
        max: u64,
        /// Seconds between fetches.
        fetch: u64,
        /// Squash auto commits.
        squash: bool,
    },
}

/// A change waiting for the user's confirmation.
#[derive(Debug, Clone, PartialEq)]
pub enum Pending {
    /// Edits of `config.edn` that rebuild the index.
    Graph(Vec<GraphEdit>),
    /// Turn the trigram block index on or off.
    Substring(bool),
    /// Revoke the token.
    RevokeToken(String),
    /// Replace the secret of the token.
    RotateToken(String),
    /// Consent to send the graph's indexed blocks to Pando's shared KB.
    GrantPandoConsent {
        /// Where the data goes (the URL, or the managed instance).
        target: String,
    },
    /// Withdraw the consent (and optionally delete the graph's documents from Pando).
    RevokePandoConsent {
        /// Also remove the documents already sent.
        purge: bool,
    },
    /// Restore every default shortcut.
    ResetKeymap,
    /// Bind a keystroke that another action in the context already uses.
    KeymapConflict {
        /// Key context.
        context: Option<String>,
        /// Action receiving the keystroke.
        action: String,
        /// The keystroke.
        keys: String,
        /// Actions that lose it.
        conflicts: Vec<String>,
    },
}

/// The MCP server state the Agents section shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpStatus {
    /// The endpoint, when the server runs.
    pub endpoint: Option<String>,
    /// Why it could not start (for example "port 12316 is in use").
    pub unavailable: Option<String>,
}

/// A secret shown once after creating or rotating a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealedSecret {
    /// Token name.
    pub name: String,
    /// The secret.
    pub secret: String,
}

/// Handles of the open graph's session the settings use.
#[derive(Debug, Clone, Default)]
pub struct SettingsContext {
    /// Graph folder.
    pub root: Option<PathBuf>,
    /// The single-writer queue.
    pub queue: Option<CommandQueue>,
    /// Runs closures on the session thread.
    pub session: Option<SessionHandle>,
    /// Global config file (tests point it nowhere).
    pub global_config: Option<PathBuf>,
    /// Where the user keymap is kept (`None`: not persisted).
    pub keymap_file: Option<PathBuf>,
    /// `pando.json` (machine-local Pando settings); `None` keeps them in memory (tests).
    pub pando_file: Option<PathBuf>,
    /// Bearer tokens of the running MCP server.
    pub tokens: Option<Arc<TokenStore>>,
    /// Write policy of the running MCP server (toggles apply live).
    pub policy: Option<Arc<WritePolicy>>,
}

/// Text inputs of the view.
struct Inputs {
    title_format: Entity<InputState>,
    file_format: Entity<InputState>,
    hidden: Entity<InputState>,
    template: Entity<InputState>,
    favorite: Entity<InputState>,
    protected: Entity<InputState>,
    origins: Entity<InputState>,
    rate: Entity<InputState>,
    port: Entity<InputState>,
    token_name: Entity<InputState>,
    keymap_filter: Entity<InputState>,
    sync_idle: Entity<InputState>,
    sync_max: Entity<InputState>,
    sync_fetch: Entity<InputState>,
    font_size: Entity<InputState>,
}

/// A text field that commits on Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    TitleFormat,
    FileFormat,
    Hidden,
    Template,
    Favorite,
    Protected,
    Origins,
    Rate,
    TokenName,
    Port,
    SyncTiming,
    FontSize,
    PandoEndpoints,
    PandoExclusion,
}

/// The overlay.
pub struct SettingsView {
    open: bool,
    section: Section,
    focus: FocusHandle,
    pub(crate) ctx: SettingsContext,
    pub(crate) graph: Option<GraphValues>,
    inputs: Inputs,
    pub(crate) message: Option<(Level, String)>,
    pub(crate) pending: Option<Pending>,
    pub(crate) revealed: Option<RevealedSecret>,
    pub(crate) new_write: bool,
    pub(crate) new_delete: bool,
    pub(crate) mcp: McpStatus,
    pub(crate) keymap: KeymapModel,
    pub(crate) recording: Option<(Option<String>, String)>,
    pub(crate) sync_prefs: SyncPrefs,
    pub(crate) sync_view: Option<SyncStatusView>,
    pub(crate) pando: pando::PandoPanel,
    pando_inputs: pando::PandoInputs,
    squash: bool,
    needs_reload: bool,
    _subscriptions: Vec<Subscription>,
}

impl std::fmt::Debug for SettingsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsView")
            .field("open", &self.open)
            .field("section", &self.section)
            .finish_non_exhaustive()
    }
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl SettingsView {
    /// A closed view.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = |cx: &mut Context<Self>, window: &mut Window, placeholder: String| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let inputs = Inputs {
            title_format: field(cx, window, "MMM do, yyyy".to_owned()),
            file_format: field(cx, window, "yyyy_MM_dd".to_owned()),
            hidden: field(
                cx,
                window,
                t!("settings.general.hidden_placeholder").to_string(),
            ),
            template: field(
                cx,
                window,
                t!("settings.editor.template_placeholder").to_string(),
            ),
            favorite: field(
                cx,
                window,
                t!("settings.general.favorite_placeholder").to_string(),
            ),
            protected: field(
                cx,
                window,
                t!("settings.agents.protected_placeholder").to_string(),
            ),
            origins: field(cx, window, "http://localhost:3000".to_owned()),
            rate: field(cx, window, "60".to_owned()),
            port: field(cx, window, "12316".to_owned()),
            token_name: field(
                cx,
                window,
                t!("settings.agents.token_name_placeholder").to_string(),
            ),
            keymap_filter: field(
                cx,
                window,
                t!("settings.keymap.filter_placeholder").to_string(),
            ),
            sync_idle: field(cx, window, "20".to_owned()),
            sync_max: field(cx, window, "300".to_owned()),
            sync_fetch: field(cx, window, "120".to_owned()),
            font_size: field(cx, window, "16".to_owned()),
        };
        let pando_inputs = pando::PandoInputs {
            rest_url: field(cx, window, "http://127.0.0.1:8765".to_owned()),
            agui_url: field(cx, window, "http://127.0.0.1:8765".to_owned()),
            rest_token: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(t!("settings.pando.token_placeholder").to_string())
            }),
            agui_token: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .placeholder(t!("settings.pando.token_placeholder").to_string())
            }),
            binary: field(cx, window, "pando".to_owned()),
            exclusion: field(
                cx,
                window,
                t!("settings.pando.exclusion_placeholder").to_string(),
            ),
        };
        let mut subscriptions = Vec::new();
        for (input, which) in [
            (&pando_inputs.rest_url, Field::PandoEndpoints),
            (&pando_inputs.agui_url, Field::PandoEndpoints),
            (&pando_inputs.binary, Field::PandoEndpoints),
            (&pando_inputs.exclusion, Field::PandoExclusion),
        ] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.commit(which, window, cx);
                    }
                },
            ));
        }
        for (input, which) in [
            (&inputs.title_format, Field::TitleFormat),
            (&inputs.file_format, Field::FileFormat),
            (&inputs.hidden, Field::Hidden),
            (&inputs.template, Field::Template),
            (&inputs.favorite, Field::Favorite),
            (&inputs.protected, Field::Protected),
            (&inputs.origins, Field::Origins),
            (&inputs.rate, Field::Rate),
            (&inputs.port, Field::Port),
            (&inputs.token_name, Field::TokenName),
            (&inputs.sync_idle, Field::SyncTiming),
            (&inputs.sync_max, Field::SyncTiming),
            (&inputs.sync_fetch, Field::SyncTiming),
            (&inputs.font_size, Field::FontSize),
        ] {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.commit(which, window, cx),
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            ));
        }
        subscriptions.push(cx.observe(&inputs.keymap_filter, |_, _, cx| cx.notify()));
        let keymap = KeymapModel::new(crate::keymap::DEFAULT_KEYMAP, None, &[]);
        Self {
            open: false,
            section: Section::General,
            focus: cx.focus_handle(),
            ctx: SettingsContext::default(),
            graph: None,
            inputs,
            message: None,
            pending: None,
            revealed: None,
            new_write: false,
            new_delete: false,
            mcp: McpStatus::default(),
            keymap,
            recording: None,
            sync_prefs: SyncPrefs::default(),
            sync_view: None,
            pando: pando::PandoPanel::new(),
            pando_inputs,
            squash: true,
            needs_reload: false,
            _subscriptions: subscriptions,
        }
    }

    /// Whether the view is showing.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The section on screen.
    pub fn section(&self) -> Section {
        self.section
    }

    /// Shows the view on `section` (loads the values from disk and the settings file).
    pub fn show(&mut self, section: Option<Section>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(section) = section {
            self.section = section;
        }
        self.open = true;
        self.message = None;
        self.pending = None;
        self.recording = None;
        self.reload(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Hides the view.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.revealed = None;
            self.recording = None;
            cx.emit(SettingsEvent::Closed);
            cx.notify();
        }
    }

    /// Closes the recorder or a revealed secret first (Escape peels one layer at a time).
    /// Returns whether something was closed.
    pub fn escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.recording.take().is_some() {
            cx.notify();
            return true;
        }
        if self.open {
            self.close(cx);
            return true;
        }
        false
    }

    /// Switches section.
    pub fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.message = None;
        self.recording = None;
        cx.notify();
    }

    /// Hands over the handles of the open graph (call when a session goes live or ends).
    pub fn set_context(&mut self, ctx: SettingsContext, cx: &mut Context<Self>) {
        self.ctx = ctx;
        self.needs_reload = self.open;
        cx.notify();
    }

    /// The MCP endpoint, when the server runs.
    pub fn set_mcp_endpoint(&mut self, endpoint: Option<String>, cx: &mut Context<Self>) {
        self.mcp.endpoint = endpoint;
        cx.notify();
    }

    /// Why the MCP server could not start.
    pub fn set_mcp_unavailable(&mut self, reason: Option<String>, cx: &mut Context<Self>) {
        self.mcp.unavailable = reason;
        cx.notify();
    }

    /// Sync preferences and the engine's last status.
    pub fn set_sync(
        &mut self,
        prefs: SyncPrefs,
        view: Option<SyncStatusView>,
        cx: &mut Context<Self>,
    ) {
        let prefs_changed = self.sync_prefs != prefs;
        self.sync_prefs = prefs;
        self.sync_view = view;
        self.needs_reload |= self.open && prefs_changed;
        cx.notify();
    }

    /// Re-reads everything the sections display.
    pub fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.graph = self
            .ctx
            .root
            .as_deref()
            .map(|root| GraphValues::load(root, self.ctx.global_config.as_deref()));
        if let Some(values) = self.graph.clone() {
            self.set_input(
                &self.inputs.title_format.clone(),
                &values.journal_title_format,
                window,
                cx,
            );
            self.set_input(
                &self.inputs.file_format.clone(),
                &values.journal_file_format,
                window,
                cx,
            );
            self.set_input(
                &self.inputs.hidden.clone(),
                &values.hidden.join(", "),
                window,
                cx,
            );
            self.set_input(
                &self.inputs.template.clone(),
                &values.journal_template,
                window,
                cx,
            );
        }
        if let Some(app) = crate::theme::try_settings(cx) {
            let rate = app.mcp.writes_per_minute.to_string();
            self.set_input(
                &self.inputs.protected.clone(),
                &app.mcp.protected_namespaces.join(", "),
                window,
                cx,
            );
            self.set_input(
                &self.inputs.origins.clone(),
                &app.mcp.allowed_origins.join(", "),
                window,
                cx,
            );
            self.set_input(&self.inputs.rate.clone(), &rate, window, cx);
            let port = app.mcp.port.to_string();
            self.set_input(&self.inputs.port.clone(), &port, window, cx);
            if let Some(size) = app.font_size {
                self.set_input(
                    &self.inputs.font_size.clone(),
                    &format!("{size}"),
                    window,
                    cx,
                );
            }
        }
        self.fill_sync_inputs(window, cx);
        self.reload_pando(window, cx);
        self.reload_keymap(cx);
    }

    fn fill_sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.sync_prefs.clone();
        self.squash = p.squash_auto_commits;
        self.set_input(
            &self.inputs.sync_idle.clone(),
            &p.commit_idle_secs.to_string(),
            window,
            cx,
        );
        self.set_input(
            &self.inputs.sync_max.clone(),
            &p.commit_max_secs.to_string(),
            window,
            cx,
        );
        self.set_input(
            &self.inputs.sync_fetch.clone(),
            &p.fetch_interval_secs.to_string(),
            window,
            cx,
        );
    }

    pub(crate) fn set_input(
        &self,
        input: &Entity<InputState>,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        input.update(cx, |state, cx| {
            state.set_value(value.to_owned(), window, cx)
        });
    }

    pub(crate) fn text(&self, field: &Entity<InputState>, cx: &App) -> String {
        field.read(cx).value().to_string()
    }

    /// Shows a line under the section title (and a toast would be redundant).
    pub(crate) fn say(&mut self, level: Level, text: impl Into<String>, cx: &mut Context<Self>) {
        self.message = Some((level, text.into()));
        cx.notify();
    }

    /// Current app settings (defaults before the theme controller exists).
    pub(crate) fn app(&self, cx: &App) -> AppSettings {
        crate::theme::try_settings(cx).unwrap_or_default()
    }

    // ---- commits -----------------------------------------------------------------------

    fn commit(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        match field {
            Field::TitleFormat => {
                let text = self.text(&self.inputs.title_format, cx);
                self.request_graph(
                    vec![GraphEdit::JournalTitleFormat(text.trim().to_owned())],
                    window,
                    cx,
                );
            }
            Field::FileFormat => {
                let text = self.text(&self.inputs.file_format, cx);
                self.request_graph(
                    vec![GraphEdit::JournalFileFormat(text.trim().to_owned())],
                    window,
                    cx,
                );
            }
            Field::Hidden => {
                let items = model::parse_list(&self.text(&self.inputs.hidden, cx));
                self.request_graph(vec![GraphEdit::Hidden(items)], window, cx);
            }
            Field::Template => {
                let text = self.text(&self.inputs.template, cx);
                self.request_graph(vec![GraphEdit::DefaultJournalTemplate(text)], window, cx);
            }
            Field::Favorite => {
                let text = self.text(&self.inputs.favorite, cx);
                self.request_graph(vec![GraphEdit::FavoriteAdd(text)], window, cx);
                self.set_input(&self.inputs.favorite.clone(), "", window, cx);
            }
            Field::Protected | Field::Origins | Field::Rate | Field::Port | Field::TokenName => {
                self.commit_agents(field, window, cx);
            }
            Field::SyncTiming => self.commit_sync_timing(cx),
            Field::FontSize => self.commit_font_size(window, cx),
            Field::PandoEndpoints => self.commit_pando_endpoints(cx),
            Field::PandoExclusion => self.add_pando_exclusion(window, cx),
        }
    }

    /// Applies graph edits: live ones at once, index-affecting ones after the user confirms.
    pub fn request_graph(
        &mut self,
        edits: Vec<GraphEdit>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for edit in &edits {
            if let Err(error) = edit.validate() {
                self.say(Level::Error, error, cx);
                return;
            }
        }
        let needs_reindex = edits
            .iter()
            .any(|e| e.apply_mode() == ApplyMode::ReindexRequired);
        // A value equal to the current one asks nothing.
        if let Some(current) = &self.graph
            && edits.iter().all(|e| unchanged(current, e))
        {
            self.say(Level::Info, t!("settings.nothing_changed").to_string(), cx);
            return;
        }
        if needs_reindex {
            self.request(Pending::Graph(edits), window, cx);
        } else {
            self.apply_graph(&edits, window, cx);
        }
    }

    pub(crate) fn apply_graph(
        &mut self,
        edits: &[GraphEdit],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(queue), Some(root)) = (self.ctx.queue.clone(), self.ctx.root.clone()) else {
            self.say(Level::Warning, t!("settings.no_graph").to_string(), cx);
            return false;
        };
        match edit_config(&queue, &root, edits) {
            Ok(done) => {
                self.reload(window, cx);
                if done.changed {
                    if edits.iter().any(|e| {
                        matches!(e, GraphEdit::FavoriteAdd(_) | GraphEdit::FavoriteRemove(_))
                    }) && let Some(values) = &self.graph
                    {
                        cx.emit(SettingsEvent::FavoritesChanged(values.favorites.clone()));
                    }
                    self.say(Level::Success, t!("settings.saved").to_string(), cx);
                }
                true
            }
            Err(error) => {
                self.say(Level::Error, error.to_string(), cx);
                false
            }
        }
    }

    // ---- confirmations --------------------------------------------------------------------

    /// Asks the user to confirm `pending`; [`confirm_pending`](Self::confirm_pending) runs it.
    /// Without a dialog host (headless tests) the question stays pending.
    pub fn request(&mut self, pending: Pending, window: &mut Window, cx: &mut Context<Self>) {
        let question = confirmation_for(&pending);
        self.pending = Some(pending);
        let (ok, cancel) = (cx.entity(), cx.entity());
        crate::ui::choose(
            window,
            cx,
            question,
            move |window, cx| ok.update(cx, |this, cx| this.confirm_pending(window, cx)),
            move |_, cx| {
                cancel.update(cx, |this, cx| {
                    this.pending = None;
                    cx.notify();
                });
            },
        );
        cx.notify();
    }

    /// The change waiting for confirmation.
    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    /// Runs the confirmed change.
    pub fn confirm_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending {
            Pending::Graph(edits) => {
                if self.apply_graph(&edits, window, cx) {
                    cx.emit(SettingsEvent::Reindex);
                }
            }
            Pending::Substring(on) => self.apply_substring(on, window, cx),
            Pending::RevokeToken(name) => self.revoke_token(&name, cx),
            Pending::RotateToken(name) => self.rotate_token(&name, cx),
            Pending::GrantPandoConsent { .. } => self.grant_pando_consent(cx),
            Pending::RevokePandoConsent { purge } => self.revoke_pando_consent(purge, cx),
            Pending::ResetKeymap => self.reset_keymap(cx),
            Pending::KeymapConflict {
                context,
                action,
                keys,
                ..
            } => self.bind_key(&context, &action, &keys, cx),
        }
        cx.notify();
    }

    /// Drops the pending change.
    pub fn cancel_pending(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        cx.notify();
    }

    // ---- search -----------------------------------------------------------------------------

    /// Turns the trigram block index on or off (asks first: it rebuilds an index).
    pub fn request_substring(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.app(cx).search.substring == on {
            return;
        }
        self.request(Pending::Substring(on), window, cx);
    }

    fn apply_substring(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        crate::theme::edit_settings(cx, Some(window), |s| s.search.substring = on);
        let Some(session) = self.ctx.session.clone() else {
            // Applied the next time a graph opens.
            self.say(Level::Success, t!("settings.saved").to_string(), cx);
            return;
        };
        let done = session.run(move |s| s.set_substring(on).map_err(|e| e.to_string()));
        self.say(Level::Info, t!("settings.search.applying").to_string(), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = done.recv().await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(_)) => {
                    this.say(
                        Level::Success,
                        t!("settings.search.applied").to_string(),
                        cx,
                    );
                    notify(
                        window,
                        cx,
                        Level::Success,
                        t!("settings.search.applied").to_string(),
                    );
                }
                Ok(Err(error)) => this.say(Level::Error, error, cx),
                Err(_) => this.say(Level::Warning, t!("settings.no_graph").to_string(), cx),
            });
        })
        .detach();
    }

    fn commit_font_size(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(&self.inputs.font_size, cx);
        let (min, max) = crate::settings::FONT_SIZE_RANGE;
        match text.trim().parse::<u16>() {
            Ok(size) if (min..=max).contains(&size) => {
                crate::theme::edit_settings(cx, Some(window), |s| s.font_size = Some(size));
                self.say(Level::Success, t!("settings.saved").to_string(), cx);
            }
            _ => self.say(
                Level::Error,
                t!(
                    "settings.appearance.font_size_invalid",
                    min = min,
                    max = max
                )
                .to_string(),
                cx,
            ),
        }
    }

    fn commit_sync_timing(&mut self, cx: &mut Context<Self>) {
        let parse = |input: &Entity<InputState>, range, default, cx: &App| {
            crate::sync_prefs::parse_secs(&input.read(cx).value(), range, default)
        };
        let idle = parse(
            &self.inputs.sync_idle,
            crate::sync_prefs::IDLE_RANGE,
            20,
            cx,
        );
        let max = parse(&self.inputs.sync_max, crate::sync_prefs::MAX_RANGE, 300, cx);
        let fetch = parse(
            &self.inputs.sync_fetch,
            crate::sync_prefs::FETCH_RANGE,
            120,
            cx,
        );
        match (idle, max, fetch) {
            (Ok(idle), Ok(max), Ok(fetch)) => {
                cx.emit(SettingsEvent::SyncTiming {
                    idle,
                    max,
                    fetch,
                    squash: self.squash,
                });
            }
            _ => self.say(
                Level::Error,
                t!("settings.sync.timing_invalid").to_string(),
                cx,
            ),
        }
    }

    pub(crate) fn toggle_squash(&mut self, on: bool, cx: &mut Context<Self>) {
        self.squash = on;
        self.commit_sync_timing(cx);
    }

    pub(crate) fn squash(&self) -> bool {
        self.squash
    }

    // ---- key events -------------------------------------------------------------------------

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.recording.is_some() {
            self.record_key(event, window, cx);
            cx.stop_propagation();
        }
    }
}

/// Whether `edit` would leave `values` as they are.
fn unchanged(values: &GraphValues, edit: &GraphEdit) -> bool {
    match edit {
        GraphEdit::JournalTitleFormat(f) => &values.journal_title_format == f,
        GraphEdit::JournalFileFormat(f) => &values.journal_file_format == f,
        GraphEdit::Hidden(items) => &values.hidden == items,
        GraphEdit::DefaultJournalTemplate(t) => values.journal_template == t.trim(),
        GraphEdit::NameFormat(f) => values.name_format == *f,
        GraphEdit::PreferredWorkflow(w) => values.workflow == *w,
        GraphEdit::FavoriteAdd(p) => values.favorites.iter().any(|f| f == p.trim()),
        GraphEdit::FavoriteRemove(p) => !values.favorites.iter().any(|f| f == p),
        // Owned by the graph view, which compares against its own state.
        GraphEdit::GraphToggle(..) | GraphEdit::GraphForce(..) | GraphEdit::GraphForcesReset => {
            false
        }
    }
}

/// The question shown for a pending change.
fn confirmation_for(pending: &Pending) -> Confirmation {
    let (title, description) = match pending {
        Pending::Graph(_) => (
            t!("settings.confirm.reindex_title").to_string(),
            t!("settings.confirm.reindex_body").to_string(),
        ),
        Pending::Substring(true) => (
            t!("settings.confirm.substring_on_title").to_string(),
            t!("settings.confirm.substring_on_body").to_string(),
        ),
        Pending::Substring(false) => (
            t!("settings.confirm.substring_off_title").to_string(),
            t!("settings.confirm.substring_off_body").to_string(),
        ),
        Pending::RevokeToken(name) => (
            t!("settings.confirm.revoke_title", name = name).to_string(),
            t!("settings.confirm.revoke_body").to_string(),
        ),
        Pending::RotateToken(name) => (
            t!("settings.confirm.rotate_title", name = name).to_string(),
            t!("settings.confirm.rotate_body").to_string(),
        ),
        Pending::GrantPandoConsent { target } => pando::consent_confirmation(target),
        Pending::RevokePandoConsent { purge: false } => (
            t!("settings.pando.revoke_title").to_string(),
            t!("settings.pando.revoke_body").to_string(),
        ),
        Pending::RevokePandoConsent { purge: true } => (
            t!("settings.pando.purge_title").to_string(),
            t!("settings.pando.purge_body").to_string(),
        ),
        Pending::ResetKeymap => (
            t!("settings.confirm.reset_keys_title").to_string(),
            t!("settings.confirm.reset_keys_body").to_string(),
        ),
        Pending::KeymapConflict {
            keys, conflicts, ..
        } => (
            t!("settings.confirm.conflict_title", keys = keys).to_string(),
            t!(
                "settings.confirm.conflict_body",
                actions = conflicts.join(", ")
            )
            .to_string(),
        ),
    };
    let ok_text = match pending {
        Pending::RevokeToken(_) => t!("settings.agents.revoke"),
        Pending::RotateToken(_) => t!("settings.agents.rotate"),
        Pending::ResetKeymap => t!("settings.keymap.reset"),
        Pending::GrantPandoConsent { .. } => t!("settings.pando.grant"),
        Pending::RevokePandoConsent { purge: false } => t!("settings.pando.revoke"),
        Pending::RevokePandoConsent { purge: true } => t!("settings.pando.revoke_purge"),
        _ => t!("settings.confirm.ok"),
    }
    .to_string();
    Confirmation {
        title,
        description,
        ok_text,
        cancel_text: t!("settings.confirm.cancel").to_string(),
    }
}

// ---- shared widgets ------------------------------------------------------------------------

/// A setting row: title and description on the left, the control on the right.
pub(crate) fn row(
    theme: &crate::ui::theme::Theme,
    title: String,
    description: Option<String>,
    control: impl IntoElement,
) -> AnyElement {
    h_flex()
        .gap_4()
        .py_2()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme.border.opacity(0.5))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().child(title))
                .when_some(description, |col, d| {
                    col.child(div().text_xs().text_color(theme.muted_foreground).child(d))
                }),
        )
        .child(div().flex_shrink_0().child(control))
        .into_any_element()
}

/// A badge saying when a setting applies; nothing for live ones.
pub(crate) fn mode_badge(theme: &crate::ui::theme::Theme, mode: ApplyMode) -> Option<AnyElement> {
    let text = match mode {
        ApplyMode::Live => return None,
        ApplyMode::ReindexRequired => t!("settings.mode.reindex").to_string(),
        ApplyMode::RestartRequired => t!("settings.mode.restart").to_string(),
    };
    Some(
        div()
            .px_2()
            .rounded(crate::ui::px(4.))
            .text_xs()
            .bg(theme.warning.opacity(0.18))
            .text_color(theme.warning)
            .child(text)
            .into_any_element(),
    )
}

/// A text input with an Apply button.
pub(crate) fn field(
    id: &'static str,
    input: &Entity<InputState>,
    width: f32,
    on_apply: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .child(div().w(crate::ui::px(width)).child(Input::new(input)))
        .child(
            Button::new(id)
                .small()
                .label(t!("settings.apply").to_string())
                .on_click(on_apply),
        )
        .into_any_element()
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        if std::mem::take(&mut self.needs_reload) {
            self.reload(window, cx);
        }
        let theme = cx.theme().clone();
        let this = cx.entity();
        let dismiss = this.clone();
        let close = this.clone();

        let design = cx.bitacora().clone();
        let mut nav = v_flex()
            .id("settings-nav")
            .h_full()
            .w(design.metrics.sidebar_left)
            .flex_shrink_0()
            .gap(design.metrics.space[1])
            .p(design.metrics.space[3])
            .bg(design.colors.side)
            .border_r_1()
            .border_color(design.colors.line);
        for (group, sections) in Section::GROUPS {
            nav = nav.child(
                div()
                    .px(design.metrics.space[3])
                    .pt(design.metrics.space[3])
                    .pb(design.metrics.space[1])
                    .child(Overline::new(t!(group).to_string())),
            );
            for section in sections.iter().copied() {
                let this = this.clone();
                let selected = self.section == section;
                let fg = if selected {
                    design.colors.text
                } else {
                    design.colors.text_2
                };
                let hover = design.colors.hover;
                nav = nav.child(
                    h_flex()
                        .id(("settings-section", section as usize))
                        .h(design.metrics.nav_item)
                        .px(design.metrics.space[3])
                        .gap(design.metrics.space[3])
                        .items_center()
                        .rounded(design.metrics.radius_control)
                        .cursor_pointer()
                        .type_style(&design.type_scale.ui_small)
                        .text_color(fg)
                        .when(selected, |d| {
                            d.bg(design.colors.hover).font_weight(FontWeight::SEMIBOLD)
                        })
                        .hover(move |d| d.bg(hover))
                        .child(glyph(
                            section.icon(),
                            design.metrics.icon_sm,
                            if section == Section::Pando {
                                design.colors.ai
                            } else {
                                fg
                            },
                            cx,
                        ))
                        .child(section.title())
                        .on_click(move |_, _, cx| this.update(cx, |s, cx| s.select(section, cx))),
                );
            }
        }

        let content = match self.section {
            Section::General => self.render_general(&theme, cx),
            Section::Editor => self.render_editor(&theme, cx),
            Section::Search => self.render_search(&theme, cx),
            Section::Sync => self.render_sync(&theme, cx),
            Section::Agents => self.render_agents(&theme, cx),
            Section::Pando => self.render_pando(&theme, cx),
            Section::Appearance => self.render_appearance(&theme, cx),
            Section::Keymap => self.render_keymap(&theme, cx),
        };
        let message = self.message.clone().map(|(level, text)| {
            let color = match level {
                Level::Error => theme.danger,
                Level::Warning => theme.warning,
                Level::Success => theme.success,
                Level::Info => theme.muted_foreground,
            };
            div()
                .id("settings-message")
                .px_4()
                .py_1()
                .text_xs()
                .text_color(color)
                .child(text)
        });

        let body = v_flex()
            .id("settings-body")
            .flex_1()
            .h_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .px(design.metrics.space[5])
                    .pt(design.metrics.space[4])
                    .type_style(&design.type_scale.h2_block)
                    .child(self.section.title()),
            )
            .children(message)
            .child(
                v_flex()
                    .id(("settings-content", self.section as usize))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .p_4()
                    .child(content),
            );

        let card =
            v_flex()
                .key_context("Settings")
                .track_focus(&self.focus)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.on_key(event, window, cx)
                }))
                .h(crate::ui::px(600.))
                .child(title_bar(
                    &theme,
                    t!("settings.title").to_string(),
                    Button::new("settings-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .on_click(move |_, _, cx| close.update(cx, |s, cx| s.close(cx))),
                ))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .child(nav)
                        .child(body),
                );

        modal(
            "settings",
            &theme,
            900.,
            move |_, cx| dismiss.update(cx, |s, cx| s.close(cx)),
            card,
        )
    }
}
