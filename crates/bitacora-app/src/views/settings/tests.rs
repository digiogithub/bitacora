//! View-logic tests of the settings (BIT-T-0331, BIT-T-0332, BIT-T-0116): live and
//! confirm-first settings, the agents page against a real MCP server, persistence and the live
//! keymap.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::RefCell;
use std::io::{Read as _, Write as _};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use bitacora_mcp::{McpConfig, MemoryBackend, SecretBackend};
use bitacora_runtime::{McpOptions, RuntimeConfig, Session};

use super::*;
use crate::settings::{AppSettings, ThemePreference};
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::{keymap, theme};

struct Env {
    graph: tempfile::TempDir,
    data: tempfile::TempDir,
    session: Option<Session>,
    backend: Arc<MemoryBackend>,
}

impl Env {
    fn new(config: &str) -> Self {
        let graph = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let root = graph.path();
        std::fs::create_dir_all(root.join("logseq")).unwrap();
        std::fs::write(root.join("logseq/config.edn"), config).unwrap();
        std::fs::create_dir_all(root.join("pages")).unwrap();
        std::fs::write(root.join("pages/p.md"), "- hello\n").unwrap();
        let backend = Arc::new(MemoryBackend::new());
        let secrets: Arc<dyn SecretBackend> = backend.clone();
        let mut cfg = RuntimeConfig::new(root);
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        cfg.mcp = Some(McpOptions {
            config: McpConfig {
                port: 0,
                ..McpConfig::default()
            },
            token_path: data.path().join("tokens.json"),
            secrets: Some(secrets),
        });
        let session = Session::open(cfg).unwrap();
        Self {
            graph,
            data,
            session: Some(session),
            backend,
        }
    }

    fn root(&self) -> PathBuf {
        self.graph.path().canonicalize().unwrap()
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.root().join("logseq/config.edn")).unwrap()
    }

    fn session(&self) -> &Session {
        self.session.as_ref().unwrap()
    }

    fn context(&self) -> SettingsContext {
        SettingsContext {
            root: Some(self.root()),
            queue: Some(self.session().queue().clone()),
            session: None,
            global_config: Some(self.data.path().join("no-global.edn")),
            keymap_file: Some(self.data.path().join("keymap.json")),
            tokens: self.session().mcp_tokens(),
            policy: self.session().mcp_policy(),
        }
    }

    fn port(&self) -> u16 {
        let endpoint = self.session().mcp_endpoint().unwrap();
        endpoint
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap()
            .rsplit(':')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    /// Status line of `POST /mcp` with `token` (a refused token is `401`).
    fn mcp_status(&self, token: &str) -> u16 {
        let port = self.port();
        let body = "{}";
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut text = String::new();
        let _ = stream.read_to_string(&mut text);
        text.split_whitespace().nth(1).unwrap().parse().unwrap()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.shutdown(Duration::from_secs(10));
        }
    }
}

fn setup(cx: &mut TestAppContext, settings_file: Option<PathBuf>) {
    cx.update(|cx| {
        crate::ui::init(cx);
        crate::views::panels::register_panels(cx);
        theme::install(cx, AppSettings::default(), settings_file);
        keymap::load_with_user(cx, None).expect("default keymap");
    });
}

fn view(
    cx: &mut TestAppContext,
    ctx: SettingsContext,
) -> (Entity<SettingsView>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(SettingsView::new);
    view.update_in(cx, |v, window, cx| {
        v.set_context(ctx, cx);
        v.show(None, window, cx);
    });
    (view, cx)
}

fn events(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
) -> (Rc<RefCell<Vec<SettingsEvent>>>, crate::ui::Subscription) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let sub = cx.update(|_, cx| {
        cx.subscribe(view, move |_, event: &SettingsEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
    });
    (seen, sub)
}

fn set_text(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    pick: impl Fn(&Inputs) -> Entity<InputState>,
    text: &str,
) {
    let text = text.to_owned();
    view.update_in(cx, |v, window, cx| {
        let input = pick(&v.inputs);
        input.update(cx, |state, cx| state.set_value(text, window, cx));
    });
}

const CONFIG: &str = "{;; my graph\n :meta/version 1 ;; keep\n :favorites [\"A\"]}\n";

#[gpui_test]
fn every_section_renders_with_and_without_a_graph(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    for section in Section::ALL {
        view.update(cx, |v, cx| v.select(section, cx));
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |v, _| v.section()), section);
    }
    // No graph open: sections that need one say so instead of panicking.
    view.update_in(cx, |v, _, cx| v.set_context(SettingsContext::default(), cx));
    for section in Section::ALL {
        view.update(cx, |v, cx| v.select(section, cx));
        cx.run_until_parked();
    }
    view.update(cx, |v, cx| v.close(cx));
    assert!(!view.read_with(cx, |v, _| v.is_open()));
}

#[gpui_test]
fn changing_the_journal_title_format_asks_to_reindex_before_touching_the_file(
    cx: &mut TestAppContext,
) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    let (seen, _sub) = events(&view, cx);

    view.update_in(cx, |v, window, cx| {
        v.request_graph(
            vec![GraphEdit::JournalTitleFormat("yyyy-MM-dd".into())],
            window,
            cx,
        );
    });
    assert!(matches!(
        view.read_with(cx, |v, _| v.pending().cloned()),
        Some(Pending::Graph(_))
    ));
    assert_eq!(
        env.config_text(),
        CONFIG,
        "nothing is written before confirming"
    );

    view.update_in(cx, |v, window, cx| v.confirm_pending(window, cx));
    let text = env.config_text();
    assert!(
        text.contains("journal/page-title-format") && text.contains("yyyy-MM-dd"),
        "{text}"
    );
    assert!(
        text.contains(";; my graph") && text.contains(":meta/version 1 ;; keep"),
        "{text}"
    );
    assert_eq!(seen.borrow().as_slice(), &[SettingsEvent::Reindex]);
    assert_eq!(
        view.read_with(cx, |v, _| v.graph.clone().unwrap().journal_title_format),
        "yyyy-MM-dd"
    );
}

#[gpui_test]
fn cancelling_leaves_the_config_untouched_and_invalid_formats_never_ask(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    view.update_in(cx, |v, window, cx| {
        v.request_graph(
            vec![GraphEdit::JournalFileFormat("dd-MM-yyyy".into())],
            window,
            cx,
        );
    });
    assert!(view.read_with(cx, |v, _| v.pending().is_some()));
    view.update(cx, |v, cx| v.cancel_pending(cx));
    assert!(view.read_with(cx, |v, _| v.pending().is_none()));
    assert_eq!(env.config_text(), CONFIG);

    // A format that cannot identify a day is refused up front.
    view.update_in(cx, |v, window, cx| {
        v.request_graph(
            vec![GraphEdit::JournalTitleFormat("yyyy-MM".into())],
            window,
            cx,
        );
    });
    assert!(view.read_with(cx, |v, _| v.pending().is_none()));
    assert!(matches!(
        view.read_with(cx, |v, _| v.message.clone()),
        Some((Level::Error, _))
    ));
    assert_eq!(env.config_text(), CONFIG);
}

#[gpui_test]
fn live_graph_settings_apply_at_once_and_favorites_notify(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    let (seen, _sub) = events(&view, cx);

    view.update_in(cx, |v, window, cx| {
        v.request_graph(
            vec![GraphEdit::PreferredWorkflow(
                bitacora_config::PreferredWorkflow::Todo,
            )],
            window,
            cx,
        );
    });
    assert!(
        view.read_with(cx, |v, _| v.pending().is_none()),
        "live: no question"
    );
    assert!(env.config_text().contains(":preferred-workflow :todo"));

    set_text(&view, cx, |i| i.favorite.clone(), "Projects");
    view.update_in(cx, |v, window, cx| v.commit(Field::Favorite, window, cx));
    assert!(env.config_text().contains("\"Projects\""));
    assert_eq!(
        seen.borrow().last(),
        Some(&SettingsEvent::FavoritesChanged(vec![
            "A".into(),
            "Projects".into()
        ]))
    );
    assert!(env.config_text().contains(";; my graph"));
}

#[gpui_test]
fn search_substring_asks_first_and_persists_across_a_restart(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("settings.json");
    setup(cx, Some(file.clone()));
    let (view, cx) = view(cx, SettingsContext::default());
    view.update_in(cx, |v, window, cx| v.request_substring(false, window, cx));
    assert_eq!(
        view.read_with(cx, |v, _| v.pending().cloned()),
        Some(Pending::Substring(false))
    );
    assert!(
        AppSettings::load(&file).search.substring,
        "not saved before confirming"
    );
    view.update_in(cx, |v, window, cx| v.confirm_pending(window, cx));
    assert!(!AppSettings::load(&file).search.substring);
    // Turning it on again asks again.
    view.update_in(cx, |v, window, cx| v.request_substring(true, window, cx));
    assert_eq!(
        view.read_with(cx, |v, _| v.pending().cloned()),
        Some(Pending::Substring(true))
    );
}

#[gpui_test]
fn appearance_applies_live_and_persists(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("settings.json");
    setup(cx, Some(file.clone()));
    let (view, cx) = view(cx, SettingsContext::default());
    cx.update(|window, cx| theme::set_preference(cx, Some(window), ThemePreference::Dark));
    cx.update(|_, cx| {
        assert!(crate::ui::theme::Theme::global(cx).is_dark());
    });
    assert_eq!(AppSettings::load(&file).mode, ThemePreference::Dark);

    set_text(&view, cx, |i| i.font_size.clone(), "18");
    view.update_in(cx, |v, window, cx| v.commit(Field::FontSize, window, cx));
    assert_eq!(AppSettings::load(&file).font_size, Some(18));
    cx.update(|_, cx| {
        assert_eq!(
            f32::from(crate::ui::theme::Theme::global(cx).font_size),
            18.0
        );
    });
    // Out of range is rejected and nothing changes.
    set_text(&view, cx, |i| i.font_size.clone(), "90");
    view.update_in(cx, |v, window, cx| v.commit(Field::FontSize, window, cx));
    assert_eq!(AppSettings::load(&file).font_size, Some(18));
}

#[gpui_test]
fn agents_tokens_are_created_shown_once_and_revoking_refuses_the_next_request(
    cx: &mut TestAppContext,
) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    view.update(cx, |v, cx| {
        v.set_mcp_endpoint(Some("http://127.0.0.1:12316/mcp".into()), cx);
        v.select(Section::Agents, cx);
    });

    set_text(&view, cx, |i| i.token_name.clone(), "Claude Desktop");
    view.update(cx, |v, _| v.new_write = true);
    view.update_in(cx, |v, window, cx| v.create_token(window, cx));
    let revealed = view
        .read_with(cx, |v, _| v.revealed.clone())
        .expect("secret shown once");
    assert_eq!(revealed.name, "Claude Desktop");
    assert!(revealed.secret.starts_with("bit_"));
    let tokens = env.session().mcp_tokens().unwrap();
    let info = tokens
        .list()
        .into_iter()
        .find(|t| t.name == "Claude Desktop")
        .unwrap();
    assert_eq!(
        info.scopes,
        vec![bitacora_mcp::Scope::Read, bitacora_mcp::Scope::Write]
    );
    // The secret lives in the keychain backend, not in the token file.
    assert!(env.backend.contains("Claude Desktop"));
    let file = std::fs::read_to_string(env.data.path().join("tokens.json")).unwrap();
    assert!(!file.contains(&revealed.secret));

    // The server accepts it...
    assert_ne!(env.mcp_status(&revealed.secret), 401);
    assert_eq!(env.mcp_status("bit_not_a_token"), 401);
    // ...until it is revoked, with no restart.
    view.update_in(cx, |v, window, cx| {
        v.request(Pending::RevokeToken("Claude Desktop".into()), window, cx);
    });
    assert!(view.read_with(cx, |v, _| v.pending().is_some()));
    assert_ne!(
        env.mcp_status(&revealed.secret),
        401,
        "still valid until confirmed"
    );
    view.update_in(cx, |v, window, cx| v.confirm_pending(window, cx));
    assert_eq!(env.mcp_status(&revealed.secret), 401);
    assert!(!env.backend.contains("Claude Desktop"));

    // Names are validated and unique.
    set_text(&view, cx, |i| i.token_name.clone(), "bad/name");
    view.update_in(cx, |v, window, cx| v.create_token(window, cx));
    assert!(matches!(
        view.read_with(cx, |v, _| v.message.clone()),
        Some((Level::Error, _))
    ));
}

#[gpui_test]
fn rotating_a_token_replaces_its_secret(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    let tokens = env.session().mcp_tokens().unwrap();
    let old = tokens.secret_of("default").unwrap();
    assert_ne!(env.mcp_status(&old), 401);
    view.update_in(cx, |v, window, cx| {
        v.request(Pending::RotateToken("default".into()), window, cx);
        v.confirm_pending(window, cx);
    });
    let fresh = view
        .read_with(cx, |v, _| v.revealed.clone())
        .unwrap()
        .secret;
    assert_ne!(fresh, old);
    assert_eq!(env.mcp_status(&old), 401);
    assert_ne!(env.mcp_status(&fresh), 401);
}

#[gpui_test]
fn scopes_can_be_toggled_but_read_stays(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    let tokens = env.session().mcp_tokens().unwrap();
    view.update(cx, |v, cx| {
        v.toggle_token_scope("default", bitacora_mcp::Scope::Write, cx)
    });
    let scopes = |tokens: &bitacora_mcp::TokenStore| tokens.list()[0].scopes.clone();
    assert!(scopes(&tokens).contains(&bitacora_mcp::Scope::Write));
    view.update(cx, |v, cx| {
        v.toggle_token_scope("default", bitacora_mcp::Scope::Write, cx)
    });
    view.update(cx, |v, cx| {
        v.toggle_token_scope("default", bitacora_mcp::Scope::Read, cx)
    });
    assert_eq!(scopes(&tokens), vec![bitacora_mcp::Scope::Read]);
}

#[gpui_test]
fn write_toggles_apply_to_the_running_server_and_are_saved(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("settings.json");
    setup(cx, Some(file.clone()));
    let (view, cx) = view(cx, env.context());
    let policy = env.session().mcp_policy().unwrap();
    assert!(
        !policy.allow_writes() && !policy.allow_deletes(),
        "off by default"
    );
    view.update_in(cx, |v, window, cx| v.set_allow_writes(true, window, cx));
    view.update_in(cx, |v, window, cx| v.set_allow_deletes(true, window, cx));
    assert!(policy.allow_writes() && policy.allow_deletes());
    let saved = AppSettings::load(&file).mcp;
    assert!(saved.allow_writes && saved.allow_deletes);
    // Turning writes off also turns deletes off.
    view.update_in(cx, |v, window, cx| v.set_allow_writes(false, window, cx));
    assert!(!policy.allow_writes() && !policy.allow_deletes());

    // Protected namespaces apply live; origins are validated and need a restart.
    set_text(&view, cx, |i| i.protected.clone(), "private, secrets");
    view.update_in(cx, |v, window, cx| v.commit(Field::Protected, window, cx));
    assert!(policy.in_protected_namespace("private/diary"));
    set_text(&view, cx, |i| i.origins.clone(), "not-an-origin");
    view.update_in(cx, |v, window, cx| v.commit(Field::Origins, window, cx));
    assert!(AppSettings::load(&file).mcp.allowed_origins.is_empty());
    set_text(&view, cx, |i| i.origins.clone(), "http://localhost:3000");
    view.update_in(cx, |v, window, cx| v.commit(Field::Origins, window, cx));
    assert_eq!(
        AppSettings::load(&file).mcp.allowed_origins,
        vec!["http://localhost:3000".to_owned()]
    );
    set_text(&view, cx, |i| i.rate.clone(), "30");
    view.update_in(cx, |v, window, cx| v.commit(Field::Rate, window, cx));
    assert_eq!(AppSettings::load(&file).mcp.writes_per_minute, 30);
    set_text(&view, cx, |i| i.rate.clone(), "0");
    view.update_in(cx, |v, window, cx| v.commit(Field::Rate, window, cx));
    assert_eq!(AppSettings::load(&file).mcp.writes_per_minute, 30);
}

#[gpui_test]
fn the_client_snippet_carries_the_endpoint_and_the_secret_from_the_keychain(
    cx: &mut TestAppContext,
) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    assert!(
        view.read_with(cx, |v, _| v.snippet("default", agents::SnippetKind::Json))
            .is_none(),
        "no endpoint, no snippet"
    );
    let endpoint = env.session().mcp_endpoint().unwrap();
    view.update(cx, |v, cx| v.set_mcp_endpoint(Some(endpoint.clone()), cx));
    let secret = env
        .session()
        .mcp_tokens()
        .unwrap()
        .secret_of("default")
        .unwrap();
    assert!(
        env.backend.contains("default"),
        "the secret is in the keychain backend"
    );
    let json = view
        .read_with(cx, |v, _| v.snippet("default", agents::SnippetKind::Json))
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["mcpServers"]["bitacora"]["url"], endpoint.as_str());
    assert_eq!(
        value["mcpServers"]["bitacora"]["headers"]["Authorization"],
        format!("Bearer {secret}")
    );
    let command = view
        .read_with(cx, |v, _| {
            v.snippet("default", agents::SnippetKind::Command)
        })
        .unwrap();
    assert!(command.starts_with("claude mcp add --transport http bitacora "));
}

#[gpui_test]
fn sync_timing_is_validated_and_forwarded(cx: &mut TestAppContext) {
    setup(cx, None);
    let (view, cx) = view(cx, SettingsContext::default());
    let (seen, _sub) = events(&view, cx);
    set_text(&view, cx, |i| i.sync_idle.clone(), "30");
    set_text(&view, cx, |i| i.sync_max.clone(), "600");
    set_text(&view, cx, |i| i.sync_fetch.clone(), "240");
    view.update_in(cx, |v, window, cx| v.commit(Field::SyncTiming, window, cx));
    assert_eq!(
        seen.borrow().as_slice(),
        &[SettingsEvent::SyncTiming {
            idle: 30,
            max: 600,
            fetch: 240,
            squash: true
        }]
    );
    // Not a number: refused. Out of range: clamped, like the sync form.
    set_text(&view, cx, |i| i.sync_idle.clone(), "soon");
    view.update_in(cx, |v, window, cx| v.commit(Field::SyncTiming, window, cx));
    assert_eq!(seen.borrow().len(), 1, "a non-number is not forwarded");
    set_text(&view, cx, |i| i.sync_idle.clone(), "1");
    view.update_in(cx, |v, window, cx| v.commit(Field::SyncTiming, window, cx));
    assert!(matches!(
        seen.borrow().last(),
        Some(SettingsEvent::SyncTiming { idle: 5, .. })
    ));
}

#[gpui_test]
fn rebinding_an_action_takes_effect_immediately_and_conflicts_wait_for_confirmation(
    cx: &mut TestAppContext,
) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (view, cx) = view(cx, env.context());
    let keymap_file = env.data.path().join("keymap.json");
    let toggle = Some("Workspace".to_owned());
    let installed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| cx.global::<keymap::InstalledKeymap>().0.clone())
    };

    // Free keystroke: bound and saved at once; the old one is switched off.
    view.update_in(cx, |v, window, cx| {
        v.propose_binding(
            toggle.clone(),
            "bitacora::ToggleLeftSidebar".into(),
            "alt-j".into(),
            window,
            cx,
        );
    });
    assert!(view.read_with(cx, |v, _| v.pending().is_none()));
    let eff = installed(cx);
    assert_eq!(
        eff.get(&(toggle.clone(), keymap::normalize_keys("alt-j")))
            .map(String::as_str),
        Some("bitacora::ToggleLeftSidebar")
    );
    assert!(!eff.contains_key(&(toggle.clone(), keymap::normalize_keys("secondary-b"))));
    let saved = std::fs::read_to_string(&keymap_file).unwrap();
    assert!(
        saved.contains("alt-j") && saved.contains("unbind"),
        "{saved}"
    );

    // A keystroke another action uses: a warning, nothing saved until confirmed.
    let before = saved.clone();
    view.update_in(cx, |v, window, cx| {
        v.propose_binding(
            toggle.clone(),
            "bitacora::GoBack".into(),
            "secondary-k".into(),
            window,
            cx,
        );
    });
    assert!(matches!(
        view.read_with(cx, |v, _| v.pending().cloned()),
        Some(Pending::KeymapConflict { .. })
    ));
    assert_eq!(std::fs::read_to_string(&keymap_file).unwrap(), before);
    view.update_in(cx, |v, window, cx| v.confirm_pending(window, cx));
    let eff = installed(cx);
    assert_eq!(
        eff.get(&(toggle.clone(), keymap::normalize_keys("secondary-k")))
            .map(String::as_str),
        Some("bitacora::GoBack")
    );

    // Reset restores every default and removes the file.
    view.update_in(cx, |v, window, cx| {
        v.request(Pending::ResetKeymap, window, cx);
        v.confirm_pending(window, cx);
    });
    assert!(!keymap_file.exists());
    let eff = installed(cx);
    assert_eq!(
        eff.get(&(toggle.clone(), keymap::normalize_keys("secondary-b")))
            .map(String::as_str),
        Some("bitacora::ToggleLeftSidebar")
    );
}

#[gpui_test]
fn a_rebound_shortcut_drives_the_workspace_and_the_old_one_goes_quiet(cx: &mut TestAppContext) {
    setup(cx, None);
    let tmp = tempfile::tempdir().unwrap();
    let keymap_file = tmp.path().join("keymap.json");
    let kf = keymap_file.clone();
    let (ws, cx) = cx.add_window_view(move |window, cx| {
        crate::views::workspace::Workspace::new(
            crate::views::workspace::WorkspaceConfig {
                keymap_file: Some(kf.clone()),
                ..Default::default()
            },
            window,
            cx,
        )
    });
    let sidebar = ws.read_with(cx, |w, _| w.sidebar().clone());
    let settings = ws.read_with(cx, |w, _| w.settings().clone());
    assert!(sidebar.read_with(cx, |s, _| s.is_visible()));
    settings.update_in(cx, |s, window, cx| {
        s.reload(window, cx);
        s.propose_binding(
            Some("Workspace".into()),
            "bitacora::ToggleLeftSidebar".into(),
            "alt-j".into(),
            window,
            cx,
        );
    });
    cx.simulate_keystrokes("alt-j");
    assert!(
        !sidebar.read_with(cx, |s, _| s.is_visible()),
        "the new shortcut works"
    );
    cx.simulate_keystrokes("secondary-b");
    assert!(
        !sidebar.read_with(cx, |s, _| s.is_visible()),
        "the old one does nothing"
    );
    cx.simulate_keystrokes("alt-j");
    assert!(sidebar.read_with(cx, |s, _| s.is_visible()));
}

#[gpui_test]
fn secondary_comma_opens_the_settings_and_escape_closes_them(cx: &mut TestAppContext) {
    setup(cx, None);
    let (ws, cx) = cx.add_window_view(|window, cx| {
        crate::views::workspace::Workspace::new(Default::default(), window, cx)
    });
    let settings = ws.read_with(cx, |w, _| w.settings().clone());
    assert!(!settings.read_with(cx, |s, _| s.is_open()));
    cx.simulate_keystrokes("secondary-,");
    assert!(settings.read_with(cx, |s, _| s.is_open()));
    cx.simulate_keystrokes("escape");
    assert!(!settings.read_with(cx, |s, _| s.is_open()));
    // The palette command opens it too.
    ws.update_in(cx, |w, window, cx| {
        w.run_command(
            crate::views::palette::PaletteCommand::OpenSettings,
            window,
            cx,
        );
    });
    assert!(settings.read_with(cx, |s, _| s.is_open()));
}

#[gpui_test]
fn the_session_applies_search_substring_and_mcp_settings_from_the_app_settings(
    cx: &mut TestAppContext,
) {
    // Session options built from the app settings: a disabled trigram index and MCP knobs.
    setup(cx, None);
    let graph = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(graph.path().join("pages")).unwrap();
    std::fs::write(graph.path().join("pages/p.md"), "- hello world\n").unwrap();
    let mut options = crate::session::SessionOptions {
        data_dir: Some(data.path().to_path_buf()),
        global_config: Some(data.path().join("no-global.edn")),
        mcp_token_path: Some(data.path().join("tokens.json")),
        disable_substring: true,
        ..Default::default()
    };
    options.mcp.port = 0;
    options.mcp.allow_writes = true;
    let (session, events) =
        crate::session::GraphSession::start(graph.path().to_path_buf(), options).unwrap();
    // Wait for the session to be live.
    let mut live = false;
    for _ in 0..500 {
        while let Ok(event) = events.try_recv() {
            if matches!(event, crate::session::SessionEvent::Ready(_)) {
                live = true;
            }
        }
        if live {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(live, "session ready");
    let handle = session.handle().unwrap();
    // The index was already switched off on open: turning it off again changes nothing, and the
    // write policy follows the saved MCP settings.
    let changed = handle
        .run(|s| s.set_substring(false).unwrap())
        .recv_blocking()
        .unwrap();
    assert!(!changed);
    let writes = handle
        .run(|s| s.mcp_policy().map(|p| p.allow_writes()))
        .recv_blocking()
        .unwrap();
    assert_eq!(writes, Some(true));
    assert!(
        session
            .shutdown_with_report(Duration::from_secs(10))
            .is_some()
    );
}

#[test]
fn navigation_groups_cover_every_section_once_with_a_pando_slot() {
    let grouped: Vec<Section> = Section::GROUPS
        .iter()
        .flat_map(|(_, sections)| sections.iter().copied())
        .collect();
    assert_eq!(grouped.len(), Section::ALL.len());
    for section in Section::ALL {
        assert_eq!(grouped.iter().filter(|s| **s == section).count(), 1);
    }
    assert!(grouped.contains(&Section::Pando));
}
