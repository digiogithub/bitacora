//! Settings > Agents: the MCP server status, write toggles, protected pages, rate limit and the
//! token table (BIT-T-0116, BIT-US-0016). Token changes act on the running server's
//! [`TokenStore`], so a revoked token is refused on its next request without a restart.

use crate::views::dims;
use bitacora_mcp::{Scope, TokenStorage, TokenSummary};
use rust_i18n::t;

use super::model::{self, AppKey};
use super::{
    ApplyMode, Field, Pending, RevealedSecret, SettingsEvent, SettingsView, field, mode_badge, row,
};
use crate::ui::button::{Button, ButtonVariants as _};
use crate::ui::switch::Switch;
use crate::ui::text_edit::ClipboardItem;
use crate::ui::theme::Theme;
use crate::ui::{
    AnyElement, ClickEvent, Context, Disableable as _, FluentBuilder as _, IconName,
    InteractiveElement as _, IntoElement, Level, ParentElement as _, Sizable as _, Styled as _,
    Window, div, h_flex, v_flex,
};

/// What "copy" puts on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnippetKind {
    /// `mcpServers` JSON for Claude Desktop and other JSON-configured clients.
    Json,
    /// The `claude mcp add` command for Claude Code.
    Command,
}

/// `YYYY-MM-DD` of unix seconds (UTC).
fn day_of(secs: i64) -> String {
    bitacora_core::date::Date::from_unix_secs(secs, 0).map_or_else(String::new, |d| {
        format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
    })
}

impl SettingsView {
    // ---- toggles and lists ----------------------------------------------------------------

    /// `mcp.allow_writes`: live (the running server's policy), persisted.
    pub fn set_allow_writes(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        crate::theme::edit_settings(cx, Some(window), |s| s.mcp.allow_writes = on);
        if let Some(policy) = &self.ctx.policy {
            policy.set_allow_writes(on);
        }
        // Deletes are meaningless without writes.
        if !on {
            self.set_allow_deletes(false, window, cx);
        }
        cx.notify();
    }

    /// `mcp.allow_deletes`: live, persisted.
    pub fn set_allow_deletes(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        crate::theme::edit_settings(cx, Some(window), |s| s.mcp.allow_deletes = on);
        if let Some(policy) = &self.ctx.policy {
            policy.set_allow_deletes(on);
        }
        cx.notify();
    }

    /// `mcp.enabled`: persisted, applies when the graph is reopened.
    pub fn set_mcp_enabled(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        crate::theme::edit_settings(cx, Some(window), |s| s.mcp.enabled = on);
        self.say(
            Level::Info,
            t!("settings.agents.restart_note").to_string(),
            cx,
        );
    }

    /// `mcp.api_enabled`: persisted, applies when the server restarts.
    pub fn set_api_enabled(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        crate::theme::edit_settings(cx, Some(window), |s| s.mcp.api_enabled = on);
        self.say(
            Level::Info,
            t!("settings.agents.restart_note").to_string(),
            cx,
        );
    }

    pub(crate) fn commit_agents(
        &mut self,
        which: Field,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match which {
            Field::Protected => {
                let list = model::parse_list(&self.text(&self.inputs.protected, cx));
                crate::theme::edit_settings(cx, Some(window), |s| {
                    s.mcp.protected_namespaces.clone_from(&list);
                });
                if let Some(policy) = &self.ctx.policy {
                    policy.set_protected_namespaces(list);
                }
                self.say(Level::Success, t!("settings.saved").to_string(), cx);
            }
            Field::Origins => {
                let list = model::parse_list(&self.text(&self.inputs.origins, cx));
                if let Some(bad) = list.iter().find_map(|o| model::validate_origin(o).err()) {
                    self.say(Level::Error, bad, cx);
                    return;
                }
                crate::theme::edit_settings(cx, Some(window), |s| {
                    s.mcp.allowed_origins.clone_from(&list);
                });
                self.say(
                    Level::Info,
                    t!("settings.agents.restart_note").to_string(),
                    cx,
                );
            }
            Field::Port => match self.text(&self.inputs.port, cx).trim().parse::<u16>() {
                Ok(port) if port >= 1024 => {
                    crate::theme::edit_settings(cx, Some(window), |s| s.mcp.port = port);
                    self.say(
                        Level::Info,
                        t!("settings.agents.restart_note").to_string(),
                        cx,
                    );
                }
                _ => self.say(
                    Level::Error,
                    t!("settings.agents.port_invalid").to_string(),
                    cx,
                ),
            },
            Field::Rate => match model::parse_rate(&self.text(&self.inputs.rate, cx)) {
                Ok(n) => {
                    crate::theme::edit_settings(cx, Some(window), |s| s.mcp.writes_per_minute = n);
                    self.say(
                        Level::Info,
                        t!("settings.agents.restart_note").to_string(),
                        cx,
                    );
                }
                Err(error) => self.say(Level::Error, error, cx),
            },
            Field::TokenName => self.create_token(window, cx),
            _ => {}
        }
    }

    // ---- tokens ---------------------------------------------------------------------------

    /// Creates the token named in the input with the chosen scopes and shows its secret once.
    pub fn create_token(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tokens) = self.ctx.tokens.clone() else {
            self.say(
                Level::Warning,
                t!("settings.agents.not_running").to_string(),
                cx,
            );
            return;
        };
        let name = self.text(&self.inputs.token_name, cx).trim().to_owned();
        if let Err(error) = model::validate_token_name(&name) {
            self.say(Level::Error, error, cx);
            return;
        }
        let scopes = model::scopes_for(self.new_write, self.new_delete);
        match tokens.create(&name, &scopes) {
            Ok(secret) => {
                self.revealed = Some(RevealedSecret { name, secret });
                self.new_write = false;
                self.new_delete = false;
                self.set_input(&self.inputs.token_name.clone(), "", window, cx);
                self.say(
                    Level::Success,
                    t!("settings.agents.token_created").to_string(),
                    cx,
                );
            }
            Err(error) => self.say(Level::Error, error.to_string(), cx),
        }
    }

    /// Revokes a token (it stops working immediately).
    pub fn revoke_token(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(tokens) = self.ctx.tokens.clone() else {
            return;
        };
        match tokens.revoke(name) {
            Ok(true) => {
                if self.revealed.as_ref().is_some_and(|r| r.name == name) {
                    self.revealed = None;
                }
                self.say(
                    Level::Success,
                    t!("settings.agents.token_revoked", name = name).to_string(),
                    cx,
                );
            }
            Ok(false) => self.say(
                Level::Warning,
                t!("settings.agents.token_missing").to_string(),
                cx,
            ),
            Err(error) => self.say(Level::Error, error.to_string(), cx),
        }
    }

    /// Replaces the secret of a token and shows the new one once.
    pub fn rotate_token(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(tokens) = self.ctx.tokens.clone() else {
            return;
        };
        match tokens.rotate(name) {
            Ok(secret) => {
                self.revealed = Some(RevealedSecret {
                    name: name.to_owned(),
                    secret,
                });
                self.say(
                    Level::Success,
                    t!("settings.agents.token_rotated", name = name).to_string(),
                    cx,
                );
            }
            Err(error) => self.say(Level::Error, error.to_string(), cx),
        }
    }

    /// Adds or removes one scope of a token (read cannot be removed).
    pub fn toggle_token_scope(&mut self, name: &str, scope: Scope, cx: &mut Context<Self>) {
        let Some(tokens) = self.ctx.tokens.clone() else {
            return;
        };
        let Some(current) = tokens.list().into_iter().find(|t| t.name == name) else {
            return;
        };
        if scope == Scope::Read {
            return;
        }
        let mut scopes = current.scopes.clone();
        if scopes.contains(&scope) {
            scopes.retain(|s| *s != scope);
        } else {
            scopes.push(scope);
        }
        match tokens.set_scopes(name, &scopes) {
            Ok(()) => cx.notify(),
            Err(error) => self.say(Level::Error, error.to_string(), cx),
        }
    }

    /// The client configuration of token `name`, ready to paste.
    pub fn snippet(&self, name: &str, kind: SnippetKind) -> Option<String> {
        let endpoint = self.mcp.endpoint.as_deref()?;
        let secret = self.ctx.tokens.as_ref()?.secret_of(name)?;
        Some(match kind {
            SnippetKind::Json => model::client_config_json(endpoint, &secret),
            SnippetKind::Command => model::client_config_command(endpoint, &secret),
        })
    }

    fn copy_snippet(&mut self, name: &str, kind: SnippetKind, cx: &mut Context<Self>) {
        match self.snippet(name, kind) {
            Some(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.say(Level::Success, t!("settings.agents.copied").to_string(), cx);
            }
            None if self.mcp.endpoint.is_none() => {
                self.say(
                    Level::Warning,
                    t!("settings.agents.not_running").to_string(),
                    cx,
                );
            }
            None => self.say(
                Level::Warning,
                t!("settings.agents.secret_lost").to_string(),
                cx,
            ),
        }
    }

    fn token_rows(&self) -> Vec<TokenSummary> {
        self.ctx
            .tokens
            .as_ref()
            .map(|t| t.summaries())
            .unwrap_or_default()
    }

    // ---- rendering --------------------------------------------------------------------------

    pub(crate) fn render_agents(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.app(cx);
        let status = match (&self.mcp.endpoint, &self.mcp.unavailable) {
            (Some(endpoint), _) => (
                theme.success,
                t!("settings.agents.running", endpoint = endpoint).to_string(),
            ),
            (None, Some(why)) => (
                theme.danger,
                t!("settings.agents.unavailable", reason = why).to_string(),
            ),
            (None, None) => (
                theme.muted_foreground,
                t!("settings.agents.not_running").to_string(),
            ),
        };
        let revealed = self.revealed.clone();
        let tokens = self.token_rows();
        let can_copy = self.mcp.endpoint.is_some();

        let mut col = v_flex().gap_1();
        col = col.child(row(
            theme,
            t!("settings.agents.status").to_string(),
            Some(t!("settings.agents.status_help").to_string()),
            div()
                .id("settings-mcp-status")
                .text_sm()
                .text_color(status.0)
                .child(status.1),
        ));
        col = col
            .child(row(
                theme,
                t!("settings.agents.enabled").to_string(),
                Some(t!("settings.agents.enabled_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, AppKey::McpEnabled.apply_mode()))
                    .child(
                        Switch::new("settings-mcp-enabled")
                            .checked(app.mcp.enabled)
                            .on_click(cx.listener(|this, on: &bool, window, cx| {
                                this.set_mcp_enabled(*on, window, cx);
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.agents.port").to_string(),
                Some(t!("settings.agents.port_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, AppKey::McpPort.apply_mode()))
                    .child(field(
                        "settings-port",
                        &self.inputs.port,
                        100.,
                        self.commit_listener(Field::Port, cx),
                    )),
            ))
            .child(row(
                theme,
                t!("settings.agents.allow_writes").to_string(),
                Some(t!("settings.agents.allow_writes_help").to_string()),
                Switch::new("settings-allow-writes")
                    .checked(app.mcp.allow_writes)
                    .on_click(cx.listener(|this, on: &bool, window, cx| {
                        this.set_allow_writes(*on, window, cx);
                    })),
            ))
            .child(row(
                theme,
                t!("settings.agents.allow_deletes").to_string(),
                Some(t!("settings.agents.allow_deletes_help").to_string()),
                Switch::new("settings-allow-deletes")
                    .checked(app.mcp.allow_deletes)
                    .disabled(!app.mcp.allow_writes)
                    .on_click(cx.listener(|this, on: &bool, window, cx| {
                        this.set_allow_deletes(*on, window, cx);
                    })),
            ))
            .child(row(
                theme,
                t!("settings.agents.api").to_string(),
                Some(t!("settings.agents.api_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, AppKey::McpApi.apply_mode()))
                    .child(
                        Switch::new("settings-api")
                            .checked(app.mcp.api_enabled)
                            .on_click(cx.listener(|this, on: &bool, window, cx| {
                                this.set_api_enabled(*on, window, cx);
                            })),
                    ),
            ))
            .child(row(
                theme,
                t!("settings.agents.protected").to_string(),
                Some(t!("settings.agents.protected_help").to_string()),
                field(
                    "settings-protected",
                    &self.inputs.protected,
                    240.,
                    self.commit_listener(Field::Protected, cx),
                ),
            ))
            .child(row(
                theme,
                t!("settings.agents.origins").to_string(),
                Some(t!("settings.agents.origins_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, ApplyMode::RestartRequired))
                    .child(field(
                        "settings-origins",
                        &self.inputs.origins,
                        240.,
                        self.commit_listener(Field::Origins, cx),
                    )),
            ))
            .child(row(
                theme,
                t!("settings.agents.rate").to_string(),
                Some(t!("settings.agents.rate_help").to_string()),
                h_flex()
                    .gap_2()
                    .children(mode_badge(theme, AppKey::McpRate.apply_mode()))
                    .child(field(
                        "settings-rate",
                        &self.inputs.rate,
                        100.,
                        self.commit_listener(Field::Rate, cx),
                    )),
            ))
            .child(
                h_flex().py_2().child(
                    Button::new("settings-mcp-restart")
                        .small()
                        .disabled(self.ctx.root.is_none())
                        .label(t!("settings.agents.restart_button").to_string())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.emit(SettingsEvent::ReopenGraph);
                        })),
                ),
            );

        // A freshly created or rotated secret, shown once.
        if let Some(secret) = revealed {
            let copy_secret = secret.secret.clone();
            col = col.child(
                v_flex()
                    .id("settings-secret")
                    .gap_1()
                    .p_3()
                    .rounded(dims::PX_6)
                    .border_1()
                    .border_color(theme.warning)
                    .child(
                        div().text_sm().child(
                            t!("settings.agents.secret_once", name = secret.name).to_string(),
                        ),
                    )
                    .child(
                        div()
                            .id("settings-secret-value")
                            .text_xs()
                            .font_family("monospace")
                            .child(secret.secret),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("settings-secret-copy")
                                    .small()
                                    .icon(IconName::Copy)
                                    .label(t!("settings.agents.copy_token").to_string())
                                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_secret.clone(),
                                        ));
                                        this.say(
                                            Level::Success,
                                            t!("settings.agents.copied").to_string(),
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new("settings-secret-done")
                                    .small()
                                    .label(t!("settings.agents.done").to_string())
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.revealed = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        }

        // Token table.
        col = col.child(
            div()
                .pt_3()
                .text_sm()
                .child(t!("settings.agents.tokens").to_string()),
        );
        if tokens.is_empty() {
            col = col.child(
                div()
                    .id("settings-tokens-empty")
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(t!("settings.agents.tokens_empty").to_string()),
            );
        }
        for (ix, token) in tokens.into_iter().enumerate() {
            let TokenSummary {
                name,
                scopes,
                created_at,
                storage,
                available,
            } = token;
            let meta = format!(
                "{} \u{b7} {}{}",
                match storage {
                    TokenStorage::Keychain => t!("settings.agents.in_keychain").to_string(),
                    TokenStorage::File => t!("settings.agents.in_file").to_string(),
                },
                created_at.map(day_of).unwrap_or_default(),
                if available {
                    String::new()
                } else {
                    format!(" \u{b7} {}", t!("settings.agents.secret_lost"))
                }
            );
            let scope_button = |scope: Scope, name: &str, cx: &mut Context<Self>| {
                let owned = name.to_owned();
                Button::new((
                    match scope {
                        Scope::Read => "settings-scope-read",
                        Scope::Write => "settings-scope-write",
                        Scope::Delete => "settings-scope-delete",
                    },
                    ix,
                ))
                .xsmall()
                .label(model::scope_label(scope))
                .when(scopes.contains(&scope), |b| b.primary())
                .disabled(scope == Scope::Read)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_token_scope(&owned, scope, cx);
                }))
            };
            let (n_json, n_cmd, n_rotate, n_revoke) =
                (name.clone(), name.clone(), name.clone(), name.clone());
            col = col.child(
                h_flex()
                    .id(("settings-token", ix))
                    .gap_2()
                    .py_1()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.5))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_sm().child(name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(meta),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(scope_button(Scope::Read, &name, cx))
                            .child(scope_button(Scope::Write, &name, cx))
                            .child(scope_button(Scope::Delete, &name, cx)),
                    )
                    .child(
                        Button::new(("settings-token-json", ix))
                            .xsmall()
                            .disabled(!can_copy || !available)
                            .label(t!("settings.agents.copy_json").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.copy_snippet(&n_json, SnippetKind::Json, cx);
                            })),
                    )
                    .child(
                        Button::new(("settings-token-cmd", ix))
                            .xsmall()
                            .disabled(!can_copy || !available)
                            .label(t!("settings.agents.copy_command").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.copy_snippet(&n_cmd, SnippetKind::Command, cx);
                            })),
                    )
                    .child(
                        Button::new(("settings-token-rotate", ix))
                            .xsmall()
                            .label(t!("settings.agents.rotate").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.ask(Pending::RotateToken(n_rotate.clone()), window, cx);
                            })),
                    )
                    .child(
                        Button::new(("settings-token-revoke", ix))
                            .xsmall()
                            .danger()
                            .label(t!("settings.agents.revoke").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.ask(Pending::RevokeToken(n_revoke.clone()), window, cx);
                            })),
                    ),
            );
        }

        // New token.
        col = col.child(
            h_flex()
                .gap_2()
                .pt_2()
                .items_center()
                .child(
                    div()
                        .w(dims::PX_200)
                        .child(crate::ui::input::Input::new(&self.inputs.token_name)),
                )
                .child(
                    Button::new("settings-new-write")
                        .xsmall()
                        .label(t!("settings.agents.scope_write").to_string())
                        .when(self.new_write, |b| b.primary())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.new_write = !this.new_write;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-new-delete")
                        .xsmall()
                        .label(t!("settings.agents.scope_delete").to_string())
                        .when(self.new_delete, |b| b.primary())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.new_delete = !this.new_delete;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("settings-token-create")
                        .small()
                        .primary()
                        .icon(IconName::Plus)
                        .disabled(self.ctx.tokens.is_none())
                        .label(t!("settings.agents.new_token").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.create_token(window, cx);
                        })),
                ),
        );
        col.into_any_element()
    }
}
