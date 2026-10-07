//! Settings > Pando: connection (mode, endpoints, keychain tokens, test), managed instance
//! status, feature switches, and per graph the consent, agent writes and content exclusions
//! (BIT-US-0137, BIT-US-0138, BIT-SP-0009).
//!
//! The settings live in `pando.json` of the platform config directory (never in the graph, never
//! in git: saving here cannot touch the graph folder). Tokens go to the OS keychain and are never
//! shown. A change that alters how the session starts (enable, mode, endpoints, tokens, consent,
//! agent writes, features) reopens the graph; exclusions and revoked consent apply to the running
//! session at once (`Session::apply_pando_consent`).

use crate::views::dims;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bitacora_config::{PandoFeature, PandoMode, PandoSettings};
use bitacora_runtime::{
    ConnectionReport, KbSharing, ManagedState, ManagedStatus, PandoCredentials, PandoStatus,
    TokenKind, TokenSource,
};
use rust_i18n::t;

use super::{Pending, SettingsEvent, SettingsView, mode_badge, row};
use crate::ui::button::Button;
use crate::ui::input::{Input, InputState};
use crate::ui::switch::Switch;
use crate::ui::theme::Theme;
use crate::ui::{
    AnyElement, AppContext as _, ClickEvent, Context, Disableable as _, Entity,
    InteractiveElement as _, IntoElement, Level, ParentElement as _, Sizable as _,
    StatefulInteractiveElement as _, Styled as _, Window, div, h_flex, v_flex,
};
use crate::views::kit::{Chip, ChipTone, Glyph, IconButton, Overline, Segmented};

/// How long "Test connection" waits for the server.
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Outcome of the last "Test connection".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnTest {
    /// Not tried yet.
    #[default]
    Idle,
    /// Waiting for the server.
    Running,
    /// The server answered.
    Ok(ConnectionReport),
    /// Why it did not.
    Failed(String),
}

/// What the running session reports about Pando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveStatus {
    /// Connection state of the service.
    pub status: PandoStatus,
    /// Managed instance state, in managed mode.
    pub managed: Option<ManagedStatus>,
    /// Log of the managed instance.
    pub log: Option<PathBuf>,
    /// `(synced, pending)` documents of the semantic sync.
    pub semantic: Option<(u64, u64)>,
    /// Latest failed send of the semantic sync still waiting for a retry.
    pub semantic_error: Option<String>,
}

/// Everything the Pando section keeps between renders.
pub(crate) struct PandoPanel {
    /// The saved settings (every change is saved at once).
    pub settings: PandoSettings,
    pub test: ConnTest,
    pub live: Option<LiveStatus>,
    pub credentials: PandoCredentials,
    pub sharing: KbSharing,
    /// Global Pando config used to tell whether the managed KB is shared (tests point it away).
    pub global_pando_config: Option<PathBuf>,
}

impl PandoPanel {
    pub(crate) fn new() -> Self {
        Self {
            settings: PandoSettings::default(),
            test: ConnTest::Idle,
            live: None,
            credentials: PandoCredentials::system(),
            sharing: KbSharing::Private,
            global_pando_config: None,
        }
    }
}

/// Text inputs of the section.
pub(crate) struct PandoInputs {
    pub rest_url: Entity<InputState>,
    pub agui_url: Entity<InputState>,
    pub rest_token: Entity<InputState>,
    pub agui_token: Entity<InputState>,
    pub binary: Entity<InputState>,
    pub exclusion: Entity<InputState>,
}

// ---- pure helpers (unit tested) --------------------------------------------------------------

/// Validates and normalises one exclusion entry: a page name, a namespace, a graph-relative path
/// prefix (contains `/`) or a `#tag`.
///
/// # Errors
/// A message key suffix for an empty entry or a bare `#`.
pub(crate) fn normalize_exclusion(raw: &str) -> Result<String, &'static str> {
    let e = raw.trim();
    if e.is_empty() || e == "#" {
        return Err("exclusion_empty");
    }
    Ok(e.to_owned())
}

/// Adds `entry` unless an equal one (ignoring case) exists. Returns whether it was added.
pub(crate) fn add_exclusion(list: &mut Vec<String>, entry: &str) -> bool {
    if list.iter().any(|e| e.eq_ignore_ascii_case(entry)) {
        return false;
    }
    list.push(entry.to_owned());
    true
}

/// Whether the Agent tab can offer the assistant: the integration is on, the chat feature is on
/// and `graph_key` has consent.
pub(crate) fn agent_configured(settings: &PandoSettings, graph_key: &str) -> bool {
    settings.feature_enabled(PandoFeature::AgentChat) && settings.has_consent(graph_key)
}

/// Loads `pando.json` and tells whether the Agent tab of the graph at `root` is configured.
pub(crate) fn agent_configured_for(file: Option<&Path>, root: Option<&Path>) -> bool {
    let (Some(file), Some(root)) = (file, root) else {
        return false;
    };
    bitacora_runtime::load_pando_settings(file)
        .is_ok_and(|s| agent_configured(&s, &root.to_string_lossy()))
}

/// The label key and tone of the status chip.
pub(crate) fn status_chip(
    settings: &PandoSettings,
    consented: bool,
    live: Option<&PandoStatus>,
) -> (&'static str, ChipTone) {
    if !settings.is_active() {
        return ("status_off", ChipTone::Neutral);
    }
    if !consented {
        return ("status_consent", ChipTone::Outline);
    }
    match live {
        Some(PandoStatus::Connected { .. }) => ("status_connected", ChipTone::Accent),
        Some(PandoStatus::Unavailable { .. }) => ("status_unavailable", ChipTone::Outline),
        Some(PandoStatus::Unauthorized) => ("status_unauthorized", ChipTone::Outline),
        Some(PandoStatus::TooOld { .. }) => ("status_too_old", ChipTone::Outline),
        Some(PandoStatus::Starting) => ("status_starting", ChipTone::Ai),
        Some(PandoStatus::ConsentRequired) => ("status_consent", ChipTone::Outline),
        Some(PandoStatus::Off) | None => ("status_pending", ChipTone::Neutral),
    }
}

/// Translates a key built at run time.
fn tr(key: &str) -> String {
    t!(key).to_string()
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

fn day_text(secs: i64) -> String {
    bitacora_core::date::Date::from_unix_secs(secs, 0).map_or_else(String::new, |d| {
        format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
    })
}

fn feature_key(feature: PandoFeature) -> &'static str {
    match feature {
        PandoFeature::SemanticSearch => "semantic",
        PandoFeature::AgentChat => "chat",
        PandoFeature::McpBridge => "bridge",
        PandoFeature::JournalReview => "review",
        PandoFeature::Recommendations => "recommend",
    }
}

fn mode_key(mode: PandoMode) -> &'static str {
    match mode {
        PandoMode::Managed => "managed",
        PandoMode::External => "external",
        PandoMode::Off => "off",
    }
}

fn mode_from_key(key: &str) -> PandoMode {
    match key {
        "external" => PandoMode::External,
        "off" => PandoMode::Off,
        _ => PandoMode::Managed,
    }
}

// ---- state and actions -------------------------------------------------------------------------

impl SettingsView {
    /// The consent key of the open graph.
    pub(crate) fn pando_graph_key(&self) -> Option<String> {
        self.ctx
            .root
            .as_deref()
            .map(|r| r.to_string_lossy().into_owned())
    }

    /// Replaces the credential store (tests use an in-memory one).
    pub fn set_pando_credentials(&mut self, credentials: PandoCredentials, cx: &mut Context<Self>) {
        self.pando.credentials = credentials;
        cx.notify();
    }

    /// Points the shared-KB check at another global Pando config (tests).
    pub fn set_global_pando_config(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        self.pando.global_pando_config = path;
        self.pando.sharing =
            bitacora_runtime::kb_sharing(self.pando.global_pando_config.as_deref());
        cx.notify();
    }

    /// The Pando settings as shown.
    pub fn pando_settings(&self) -> &PandoSettings {
        &self.pando.settings
    }

    /// The last connection test.
    pub fn pando_test(&self) -> &ConnTest {
        &self.pando.test
    }

    /// Re-reads `pando.json`, the input fields and the session's Pando state.
    pub(crate) fn reload_pando(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pando.settings = match self.ctx.pando_file.as_deref() {
            Some(file) => match bitacora_runtime::load_pando_settings(file) {
                Ok(s) => s,
                Err(e) => {
                    self.say(Level::Error, e.to_string(), cx);
                    PandoSettings::default()
                }
            },
            None => PandoSettings::default(),
        };
        let s = self.pando.settings.clone();
        self.set_input(&self.pando_inputs.rest_url.clone(), &s.rest_url, window, cx);
        self.set_input(&self.pando_inputs.agui_url.clone(), &s.agui_url, window, cx);
        self.set_input(
            &self.pando_inputs.binary.clone(),
            s.binary.as_deref().unwrap_or(""),
            window,
            cx,
        );
        self.pando.sharing =
            bitacora_runtime::kb_sharing(self.pando.global_pando_config.as_deref());
        self.refresh_pando_live(cx);
    }

    /// Asks the session for the Pando status (managed instance, sync counts).
    pub fn refresh_pando_live(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.ctx.session.clone() else {
            self.pando.live = None;
            return;
        };
        let rx = session.run(|s| LiveStatus {
            status: s.pando_status(),
            managed: s.pando().and_then(|p| p.managed_status()),
            log: s.pando().and_then(|p| p.managed_log_path()),
            semantic: s.semantic_status().map(|st| (st.synced, st.pending)),
            semantic_error: s.semantic_status().and_then(|st| st.last_error),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(live) = rx.recv().await {
                let _ = this.update(cx, |this, cx| {
                    this.pando.live = Some(live);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Sends the whole graph's documents again now (re-diff plus retry of failed sends).
    pub(crate) fn resync_semantic(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.ctx.session.clone() {
            let _ = session.run(|s| {
                if let Some(worker) = s.semantic() {
                    worker.reconcile();
                    worker.retry_now();
                }
            });
        }
        self.refresh_pando_live(cx);
    }

    /// Validates and saves `settings`, then tells the workspace. Nothing is written on a
    /// validation error. `reopen` asks for the graph to be reopened so the session starts with
    /// the new settings.
    pub(crate) fn save_pando(
        &mut self,
        settings: PandoSettings,
        reopen: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Err(e) = settings.validate() {
            self.say(Level::Error, e.to_string(), cx);
            return false;
        }
        if let Some(file) = self.ctx.pando_file.clone()
            && let Err(e) = settings.save(&file)
        {
            self.say(Level::Error, e.to_string(), cx);
            return false;
        }
        self.pando.settings = settings;
        self.say(Level::Success, t!("settings.saved").to_string(), cx);
        cx.emit(SettingsEvent::PandoChanged { reopen });
        cx.notify();
        true
    }

    /// Master switch.
    pub fn set_pando_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.enabled = on;
        self.save_pando(s, true, cx);
    }

    /// Managed, external or off.
    pub fn set_pando_mode(&mut self, mode: PandoMode, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.mode = mode;
        self.pando.test = ConnTest::Idle;
        self.save_pando(s, true, cx);
    }

    /// Allow non-loopback endpoints (they then must be https).
    pub fn set_pando_allow_remote(&mut self, on: bool, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.allow_remote = on;
        self.save_pando(s, true, cx);
    }

    /// Switches one feature on or off. The review and recommendation features are read at each
    /// request, so they apply without reopening the graph.
    pub fn set_pando_feature(&mut self, feature: PandoFeature, on: bool, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.features.insert(feature, on);
        let reopen = !matches!(
            feature,
            PandoFeature::JournalReview | PandoFeature::Recommendations
        );
        self.save_pando(s, reopen, cx);
    }

    /// Changes the automatic behaviour of the AI features (daily review, auto recommendations).
    /// Applies without reopening the graph.
    pub fn set_pando_ai_auto(
        &mut self,
        update: impl FnOnce(&mut bitacora_config::AiAuto),
        cx: &mut Context<Self>,
    ) {
        let mut s = self.pando.settings.clone();
        update(&mut s.ai);
        s.ai.review_at_minute = s.ai.review_at_minute.min(23 * 60 + 59);
        self.save_pando(s, false, cx);
    }

    /// Agent writes for the open graph (the `pando` MCP token gets the Write scope).
    pub fn set_pando_agent_writes(&mut self, on: bool, cx: &mut Context<Self>) {
        let Some(key) = self.pando_graph_key() else {
            self.say(Level::Warning, t!("settings.no_graph").to_string(), cx);
            return;
        };
        let mut s = self.pando.settings.clone();
        s.graphs.entry(key).or_default().agent_writes = on;
        self.save_pando(s, true, cx);
    }

    /// Commits the URLs and the binary path from their inputs.
    pub(crate) fn commit_pando_endpoints(&mut self, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.rest_url = self.text(&self.pando_inputs.rest_url, cx).trim().to_owned();
        s.agui_url = self.text(&self.pando_inputs.agui_url, cx).trim().to_owned();
        let binary = self.text(&self.pando_inputs.binary, cx).trim().to_owned();
        s.binary = (!binary.is_empty()).then_some(binary);
        self.pando.test = ConnTest::Idle;
        self.save_pando(s, true, cx);
    }

    /// Which token source is active (the token itself is never read into the UI).
    pub fn pando_token_source(&self, kind: TokenKind) -> Option<TokenSource> {
        self.pando.credentials.resolve(kind).map(|(_, src)| src)
    }

    /// Stores the token typed in the input in the keychain and clears the input.
    pub(crate) fn save_pando_token(
        &mut self,
        kind: TokenKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = match kind {
            TokenKind::Rest => self.pando_inputs.rest_token.clone(),
            TokenKind::Agui => self.pando_inputs.agui_token.clone(),
        };
        let text = self.text(&input, cx);
        match self.pando.credentials.store(kind, &text) {
            Ok(()) => {
                self.set_input(&input, "", window, cx);
                self.say(
                    Level::Success,
                    t!("settings.pando.token_saved").to_string(),
                    cx,
                );
                cx.emit(SettingsEvent::PandoChanged { reopen: true });
            }
            Err(e) => self.say(Level::Error, e.to_string(), cx),
        }
    }

    /// Removes the stored token.
    pub(crate) fn clear_pando_token(&mut self, kind: TokenKind, cx: &mut Context<Self>) {
        match self.pando.credentials.clear(kind) {
            Ok(()) => {
                self.say(
                    Level::Success,
                    t!("settings.pando.token_cleared").to_string(),
                    cx,
                );
                cx.emit(SettingsEvent::PandoChanged { reopen: true });
            }
            Err(e) => self.say(Level::Error, e.to_string(), cx),
        }
    }

    /// Probes the configured REST endpoint on a background thread.
    pub fn test_pando(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut s = self.pando.settings.clone();
        s.rest_url = self.text(&self.pando_inputs.rest_url, cx).trim().to_owned();
        let credentials = self.pando.credentials.clone();
        self.pando.test = ConnTest::Running;
        cx.notify();
        let task = cx.background_spawn(async move {
            bitacora_runtime::test_connection(&s, &credentials, TEST_TIMEOUT)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.pando.test = match result {
                    Ok(report) => ConnTest::Ok(report),
                    Err(e) => ConnTest::Failed(e),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Restarts the managed instance.
    pub fn restart_managed_pando(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.ctx.session.clone() {
            let _ = session.run(|s| {
                if let Some(p) = s.pando() {
                    p.restart_managed();
                }
            });
            self.say(Level::Info, t!("settings.pando.restarting").to_string(), cx);
            self.refresh_pando_live(cx);
        }
    }

    // ---- consent and exclusions ---------------------------------------------------------------

    /// Asks for consent: shows the dialog that says what leaves the machine.
    pub fn request_pando_consent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pando_graph_key().is_none() {
            self.say(Level::Warning, t!("settings.no_graph").to_string(), cx);
            return;
        }
        let target = match self.pando.settings.mode {
            PandoMode::Managed => t!("settings.pando.target_managed").to_string(),
            _ => self.pando.settings.rest_url.clone(),
        };
        self.request(Pending::GrantPandoConsent { target }, window, cx);
    }

    /// Records the consent (after the dialog) and reopens the graph so the sync can start.
    pub(crate) fn grant_pando_consent(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.pando_graph_key() else {
            return;
        };
        let mut s = self.pando.settings.clone();
        s.grant_consent(&key, now_unix());
        self.save_pando(s, true, cx);
    }

    /// Revokes the consent after the user confirmed and removes the graph's documents from
    /// Pando. The running session stops sending (and honouring remembered tool decisions) at once.
    pub(crate) fn revoke_pando_consent(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.pando_graph_key() else {
            return;
        };
        let mut s = self.pando.settings.clone();
        s.revoke_consent(&key);
        if self.save_pando(s, false, cx) {
            self.apply_pando_live(true);
        }
    }

    /// Forgets the remembered "always allow/deny" decision for `tool`.
    pub fn forget_pando_tool_decision(&mut self, tool: &str, cx: &mut Context<Self>) {
        let Some(key) = self.pando_graph_key() else {
            return;
        };
        let mut s = self.pando.settings.clone();
        if s.forget_tool_decision(&key, tool) && self.save_pando(s, false, cx) {
            self.apply_pando_live(false);
        }
    }

    /// Adds the entry typed in the exclusions input.
    pub(crate) fn add_pando_exclusion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.text(&self.pando_inputs.exclusion, cx);
        let entry = match normalize_exclusion(&raw) {
            Ok(e) => e,
            Err(key) => {
                self.say(Level::Error, tr(&format!("settings.pando.{key}")), cx);
                return;
            }
        };
        let Some(key) = self.pando_graph_key() else {
            self.say(Level::Warning, t!("settings.no_graph").to_string(), cx);
            return;
        };
        let mut s = self.pando.settings.clone();
        let consent = s.graphs.entry(key).or_default();
        if !add_exclusion(&mut consent.exclusions, &entry) {
            self.say(Level::Info, t!("settings.nothing_changed").to_string(), cx);
            return;
        }
        if self.save_pando(s, false, cx) {
            self.set_input(&self.pando_inputs.exclusion.clone(), "", window, cx);
            self.apply_pando_live(false);
        }
    }

    /// Removes one exclusion.
    pub fn remove_pando_exclusion(&mut self, entry: &str, cx: &mut Context<Self>) {
        let Some(key) = self.pando_graph_key() else {
            return;
        };
        let mut s = self.pando.settings.clone();
        s.graphs
            .entry(key)
            .or_default()
            .exclusions
            .retain(|e| e != entry);
        if self.save_pando(s, false, cx) {
            self.apply_pando_live(false);
        }
    }

    /// Pushes the saved consent and exclusions into the running session.
    fn apply_pando_live(&self, purge: bool) {
        if let Some(session) = self.ctx.session.clone() {
            let settings = self.pando.settings.clone();
            let _ = session.run(move |s| s.apply_pando_consent(&settings, purge));
        }
    }

    // ---- rendering -------------------------------------------------------------------------------

    fn pando_heading(&self, key: &str) -> AnyElement {
        div()
            .pt_4()
            .pb_1()
            .child(Overline::new(t!(key).to_string()))
            .into_any_element()
    }

    pub(crate) fn render_pando(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let s = self.pando.settings.clone();
        let key = self.pando_graph_key();
        let consent = key.as_deref().map(|k| s.consent(k)).unwrap_or_default();
        let (chip_key, tone) = status_chip(
            &s,
            consent.granted,
            self.pando.live.as_ref().map(|l| &l.status),
        );
        let reason = match self.pando.live.as_ref().map(|l| &l.status) {
            Some(PandoStatus::Unavailable { reason }) => Some(reason.clone()),
            _ => None,
        };
        let mut col = v_flex().id("settings-pando").gap_1();

        col = col.child(row(
            theme,
            t!("settings.pando.status").to_string(),
            reason,
            Chip::new(tr(&format!("settings.pando.{chip_key}"))).tone(tone),
        ));

        // ---- Connection ----
        col = col
            .child(self.pando_heading("settings.pando.connection"))
            .child(row(
                theme,
                t!("settings.pando.enabled").to_string(),
                Some(t!("settings.pando.enabled_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, super::ApplyMode::RestartRequired))
                    .child(
                        Switch::new("settings-pando-enabled")
                            .checked(s.enabled)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.set_pando_enabled(*on, cx);
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.pando.mode").to_string(),
                Some(t!("settings.pando.mode_help").to_string()),
                Segmented::new("settings-pando-mode")
                    .option("managed", t!("settings.pando.mode_managed").to_string())
                    .option("external", t!("settings.pando.mode_external").to_string())
                    .option("off", t!("settings.pando.mode_off").to_string())
                    .selected(mode_key(s.mode))
                    .on_change({
                        let this = cx.entity();
                        move |k, _, cx| {
                            let mode = mode_from_key(k);
                            this.update(cx, |this, cx| this.set_pando_mode(mode, cx));
                        }
                    }),
            ));
        if s.mode == PandoMode::External {
            col = col
                .child(self.url_row(
                    theme,
                    "settings.pando.rest_url",
                    &self.pando_inputs.rest_url,
                ))
                .child(self.url_row(
                    theme,
                    "settings.pando.agui_url",
                    &self.pando_inputs.agui_url,
                ))
                .child(row(
                    theme,
                    t!("settings.pando.endpoints_apply").to_string(),
                    Some(t!("settings.pando.endpoints_help").to_string()),
                    Button::new("settings-pando-endpoints-apply")
                        .small()
                        .label(t!("settings.apply").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.commit_pando_endpoints(cx);
                        })),
                ))
                .child(row(
                    theme,
                    t!("settings.pando.allow_remote").to_string(),
                    Some(t!("settings.pando.allow_remote_help").to_string()),
                    Switch::new("settings-pando-remote")
                        .checked(s.allow_remote)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.set_pando_allow_remote(*on, cx);
                        })),
                ))
                .child(self.token_row(theme, TokenKind::Rest, cx))
                .child(self.token_row(theme, TokenKind::Agui, cx))
                .child(row(
                    theme,
                    t!("settings.pando.test").to_string(),
                    Some(self.test_text()),
                    Button::new("settings-pando-test")
                        .small()
                        .label(t!("settings.pando.test_button").to_string())
                        .disabled(self.pando.test == ConnTest::Running)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.test_pando(window, cx);
                        })),
                ));
        }
        if s.mode == PandoMode::Managed {
            col = col.child(self.managed_rows(theme, cx));
        }

        // ---- Features ----
        col = col.child(self.pando_heading("settings.pando.features"));
        for feature in PandoFeature::ALL {
            let k = feature_key(feature);
            col = col.child(row(
                theme,
                tr(&format!("settings.pando.feature_{k}")),
                Some(tr(&format!("settings.pando.feature_{k}_help"))),
                Switch::new(("settings-pando-feature", feature as usize))
                    .checked(s.features.get(&feature).copied().unwrap_or(true))
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.set_pando_feature(feature, *on, cx);
                    })),
            ));
        }

        col = col.child(self.ai_auto_rows(theme, cx));

        // ---- Graph ----
        col = col.child(self.pando_heading("settings.pando.graph"));
        col = col.child(self.graph_rows(theme, &consent, key.is_some(), cx));

        // ---- Activity ----
        col = col.child(self.pando_heading("settings.pando.activity"));
        col = col.child(self.activity_rows(theme, cx));
        col.into_any_element()
    }

    /// Daily review and auto recommendations: off by default, shown under the features.
    fn ai_auto_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let ai = self.pando.settings.ai.clone();
        let at = ai.review_at_minute;
        let time = format!("{:02}:{:02}", at / 60, at % 60);
        let step = |id: &'static str, glyph: Glyph, delta: i32, cx: &mut Context<Self>| {
            IconButton::new(id, glyph)
                .small()
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_pando_ai_auto(
                        |ai| {
                            let minute = i64::from(ai.review_at_minute) + i64::from(delta);
                            ai.review_at_minute =
                                u32::try_from(minute.rem_euclid(24 * 60)).unwrap_or(0);
                        },
                        cx,
                    );
                }))
        };
        v_flex()
            .child(row(
                theme,
                tr("settings.pando.review_daily"),
                Some(tr("settings.pando.review_daily_help")),
                Switch::new("settings-pando-review-daily")
                    .checked(ai.review_daily)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_pando_ai_auto(|ai| ai.review_daily = on, cx);
                    })),
            ))
            .child(row(
                theme,
                tr("settings.pando.review_at"),
                Some(tr("settings.pando.review_at_help")),
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(step(
                        "settings-pando-review-earlier",
                        Glyph::ChevronLeft,
                        -30,
                        cx,
                    ))
                    .child(
                        div()
                            .debug_selector(|| "settings-pando-review-time".to_string())
                            .child(time),
                    )
                    .child(step(
                        "settings-pando-review-later",
                        Glyph::ChevronRight,
                        30,
                        cx,
                    )),
            ))
            .child(row(
                theme,
                tr("settings.pando.recommend_auto"),
                Some(tr("settings.pando.recommend_auto_help")),
                Switch::new("settings-pando-recommend-auto")
                    .checked(ai.recommend_auto)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_pando_ai_auto(|ai| ai.recommend_auto = on, cx);
                    })),
            ))
            .into_any_element()
    }

    fn url_row(&self, theme: &Theme, key: &str, input: &Entity<InputState>) -> AnyElement {
        row(
            theme,
            t!(key).to_string(),
            None,
            div().w(dims::PX_300).child(Input::new(input)),
        )
    }

    fn token_row(&self, theme: &Theme, kind: TokenKind, cx: &mut Context<Self>) -> AnyElement {
        let (title, input, id_save, id_clear) = match kind {
            TokenKind::Rest => (
                "settings.pando.rest_token",
                &self.pando_inputs.rest_token,
                "settings-pando-rest-token-save",
                "settings-pando-rest-token-clear",
            ),
            TokenKind::Agui => (
                "settings.pando.agui_token",
                &self.pando_inputs.agui_token,
                "settings-pando-agui-token-save",
                "settings-pando-agui-token-clear",
            ),
        };
        let source = match self.pando_token_source(kind) {
            Some(TokenSource::Keychain) => t!("settings.pando.token_keychain").to_string(),
            Some(TokenSource::Env) => {
                t!("settings.pando.token_env", var = kind.env_var()).to_string()
            }
            None => t!("settings.pando.token_none").to_string(),
        };
        row(
            theme,
            t!(title).to_string(),
            Some(source),
            h_flex()
                .gap_2()
                .child(div().w(dims::PX_200).child(Input::new(input)))
                .child(
                    Button::new(id_save)
                        .small()
                        .label(t!("settings.pando.token_save").to_string())
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.save_pando_token(kind, window, cx);
                        })),
                )
                .child(
                    Button::new(id_clear)
                        .small()
                        .label(t!("settings.pando.token_clear").to_string())
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.clear_pando_token(kind, cx);
                        })),
                ),
        )
    }

    fn test_text(&self) -> String {
        match &self.pando.test {
            ConnTest::Idle => t!("settings.pando.test_help").to_string(),
            ConnTest::Running => t!("settings.pando.testing").to_string(),
            ConnTest::Ok(r) if r.version_ok => {
                t!("settings.pando.test_ok", version = r.version).to_string()
            }
            ConnTest::Ok(r) => t!(
                "settings.pando.test_old",
                version = r.version,
                min = r.min_version
            )
            .to_string(),
            ConnTest::Failed(e) => t!("settings.pando.test_failed", error = e).to_string(),
        }
    }

    fn managed_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let live = self.pando.live.as_ref();
        let managed = live.and_then(|l| l.managed.as_ref());
        let state = managed.map_or_else(
            || t!("settings.pando.managed_idle").to_string(),
            |m| {
                let key = match m.state {
                    ManagedState::Stopped => "stopped",
                    ManagedState::Starting => "starting",
                    ManagedState::Ready => "ready",
                    ManagedState::Restarting => "restarting",
                    ManagedState::Failed => "failed",
                };
                let base = tr(&format!("settings.pando.managed_{key}"));
                match &m.last_error {
                    Some(e) if m.state == ManagedState::Failed => format!("{base}: {e}"),
                    _ => base,
                }
            },
        );
        let sharing = match &self.pando.sharing {
            KbSharing::Shared(dir) => {
                t!("settings.pando.kb_shared", dir = dir.display().to_string()).to_string()
            }
            KbSharing::Private => t!("settings.pando.kb_private").to_string(),
        };
        let log = live.and_then(|l| l.log.clone());
        v_flex()
            .child(row(
                theme,
                t!("settings.pando.managed").to_string(),
                Some(state),
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("settings-pando-restart")
                            .small()
                            .label(t!("settings.pando.restart").to_string())
                            .disabled(self.ctx.session.is_none())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.restart_managed_pando(cx);
                            })),
                    )
                    .child(
                        Button::new("settings-pando-log")
                            .small()
                            .label(t!("settings.pando.open_log").to_string())
                            .disabled(log.is_none())
                            .on_click(move |_, _, cx| {
                                if let Some(path) = &log {
                                    cx.open_with_system(path);
                                }
                            }),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.pando.binary").to_string(),
                Some(t!("settings.pando.binary_help").to_string()),
                v_flex().child(super::field(
                    "settings-pando-binary-apply",
                    &self.pando_inputs.binary,
                    240.,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.commit_pando_endpoints(cx)),
                )),
            ))
            .child(row(
                theme,
                t!("settings.pando.kb").to_string(),
                Some(sharing),
                div(),
            ))
            .into_any_element()
    }

    fn graph_rows(
        &self,
        theme: &Theme,
        consent: &bitacora_config::GraphConsent,
        has_graph: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_graph {
            return div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(t!("settings.no_graph").to_string())
                .into_any_element();
        }
        let consent_text = match (consent.granted, consent.granted_at) {
            (true, Some(at)) => t!("settings.pando.consent_given", date = day_text(at)).to_string(),
            (true, None) => t!("settings.pando.consent_given_undated").to_string(),
            (false, _) => t!("settings.pando.consent_missing").to_string(),
        };
        let mut col = v_flex().gap_1();
        col = col.child(row(
            theme,
            t!("settings.pando.consent").to_string(),
            Some(consent_text),
            if consent.granted {
                h_flex().gap_2().child(
                    Button::new("settings-pando-purge")
                        .small()
                        .label(t!("settings.pando.revoke_purge").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.request(Pending::RevokePandoConsent, window, cx);
                        })),
                )
            } else {
                h_flex().gap_2().child(
                    Button::new("settings-pando-grant")
                        .small()
                        .label(t!("settings.pando.grant").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.request_pando_consent(window, cx);
                        })),
                )
            },
        ));
        col = col.child(row(
            theme,
            t!("settings.pando.agent_writes").to_string(),
            Some(t!("settings.pando.agent_writes_help").to_string()),
            Switch::new("settings-pando-agent-writes")
                .checked(consent.agent_writes)
                .on_click(cx.listener(|this, on: &bool, _, cx| {
                    this.set_pando_agent_writes(*on, cx);
                })),
        ));
        let mut chips = h_flex().gap_1().flex_wrap();
        for (ix, entry) in consent.exclusions.iter().enumerate() {
            let entry_text = entry.clone();
            chips = chips.child(
                h_flex()
                    .id(("settings-pando-exclusion", ix))
                    .gap_1()
                    .items_center()
                    .cursor_pointer()
                    .child(Chip::new(entry.clone()).tone(ChipTone::Outline).mono(true))
                    .child(
                        div()
                            .text_xs()
                            .child(t!("settings.pando.exclusion_remove").to_string()),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.remove_pando_exclusion(&entry_text, cx);
                    })),
            );
        }
        col = col.child(row(
            theme,
            t!("settings.pando.exclusions").to_string(),
            Some(t!("settings.pando.exclusions_help").to_string()),
            super::field(
                "settings-pando-exclusion-add",
                &self.pando_inputs.exclusion,
                240.,
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.add_pando_exclusion(window, cx);
                }),
            ),
        ));
        if !consent.exclusions.is_empty() {
            col = col.child(div().py_1().child(chips));
        }
        col = col.child(row(
            theme,
            t!("settings.pando.remembered").to_string(),
            Some(if consent.tool_decisions.is_empty() {
                t!("settings.pando.remembered_none").to_string()
            } else {
                t!("settings.pando.remembered_help").to_string()
            }),
            div(),
        ));
        for (ix, (tool, allow)) in consent.tool_decisions.iter().enumerate() {
            let tool_name = tool.clone();
            col = col.child(
                h_flex()
                    .id(("settings-pando-decision", ix))
                    .gap_2()
                    .items_center()
                    .child(Chip::new(tool.clone()).tone(ChipTone::Outline).mono(true))
                    .child(div().text_xs().child(if *allow {
                        t!("settings.pando.remembered_allow").to_string()
                    } else {
                        t!("settings.pando.remembered_deny").to_string()
                    }))
                    .child(
                        Button::new(("settings-pando-forget", ix))
                            .small()
                            .label(t!("settings.pando.forget").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.forget_pando_tool_decision(&tool_name, cx);
                            })),
                    ),
            );
        }
        col.into_any_element()
    }

    fn activity_rows(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let live = self.pando.live.as_ref();
        let sync = match live.and_then(|l| l.semantic) {
            Some((synced, pending)) => t!(
                "settings.pando.sync_counts",
                synced = synced,
                pending = pending
            )
            .to_string(),
            None => t!("settings.pando.sync_none").to_string(),
        };
        let log = live.and_then(|l| l.log.clone());
        let running = live.is_some_and(|l| l.semantic.is_some());
        let sync = match live.and_then(|l| l.semantic_error.as_deref()) {
            Some(error) => format!(
                "{sync} - {}",
                t!("settings.pando.sync_error", error = error)
            ),
            None => sync,
        };
        row(
            theme,
            t!("settings.pando.sync").to_string(),
            Some(sync),
            h_flex()
                .gap_2()
                .child(
                    Button::new("settings-pando-resync")
                        .small()
                        .label(t!("settings.pando.resync").to_string())
                        .disabled(!running)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.resync_semantic(cx);
                        })),
                )
                .child(
                    Button::new("settings-pando-refresh")
                        .small()
                        .label(t!("settings.pando.refresh").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.refresh_pando_live(cx);
                        })),
                )
                .child(
                    Button::new("settings-pando-activity-log")
                        .small()
                        .label(t!("settings.pando.open_log").to_string())
                        .disabled(log.is_none())
                        .on_click(move |_, _, cx| {
                            if let Some(path) = &log {
                                cx.open_with_system(path);
                            }
                        }),
                ),
        )
    }
}

/// Text of the consent dialog (what is sent, where, to whom it is visible).
pub(crate) fn consent_confirmation(target: &str) -> (String, String) {
    (
        t!("settings.pando.consent_title").to_string(),
        t!("settings.pando.consent_body", target = target).to_string(),
    )
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn exclusions_are_trimmed_deduplicated_and_never_empty() {
        assert_eq!(normalize_exclusion("  #secret "), Ok("#secret".to_owned()));
        assert!(normalize_exclusion("   ").is_err());
        assert!(normalize_exclusion("#").is_err());
        let mut list = vec!["Journal/Private".to_owned()];
        assert!(!add_exclusion(&mut list, "journal/private"));
        assert!(add_exclusion(&mut list, "#tag"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn agent_needs_consent_chat_and_activation() {
        let mut s = PandoSettings::default();
        assert!(!agent_configured(&s, "/g"));
        s.enabled = true;
        assert!(!agent_configured(&s, "/g"), "no consent");
        s.grant_consent("/g", 1);
        assert!(agent_configured(&s, "/g"));
        s.features.insert(PandoFeature::AgentChat, false);
        assert!(!agent_configured(&s, "/g"));
    }

    #[test]
    fn chip_follows_activation_consent_and_status() {
        let mut s = PandoSettings::default();
        assert_eq!(status_chip(&s, false, None).0, "status_off");
        s.enabled = true;
        assert_eq!(status_chip(&s, false, None).0, "status_consent");
        let up = PandoStatus::Connected {
            version: "1.2.3".into(),
        };
        assert_eq!(status_chip(&s, true, Some(&up)).0, "status_connected");
        let down = PandoStatus::Unavailable { reason: "x".into() };
        assert_eq!(status_chip(&s, true, Some(&down)).0, "status_unavailable");
        assert_eq!(status_chip(&s, true, None).0, "status_pending");
    }

    #[test]
    fn mode_keys_round_trip() {
        for m in [PandoMode::Managed, PandoMode::External, PandoMode::Off] {
            assert_eq!(mode_from_key(mode_key(m)), m);
        }
    }
}
