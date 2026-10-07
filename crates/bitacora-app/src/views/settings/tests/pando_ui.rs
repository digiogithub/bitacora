//! Pando settings (BIT-US-0137, BIT-US-0138): persistence, validation, keychain tokens, consent,
//! exclusions and the connection test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use bitacora_config::{PandoFeature, PandoMode};
use bitacora_runtime::{
    KbSharing, PandoCredentials, PandoMemoryBackend, PandoSecretBackend, TokenKind, TokenSource,
};

use super::*;
use crate::views::settings::pando::PandoInputs;

/// Every file under `root` with its bytes.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn ctx_with_pando(env: &Env) -> (SettingsContext, PathBuf) {
    let file = env.data.path().join("cfg").join("pando.json");
    let mut ctx = env.context();
    ctx.pando_file = Some(file.clone());
    (ctx, file)
}

fn memory_credentials() -> PandoCredentials {
    let backend: Arc<dyn PandoSecretBackend> = Arc::new(PandoMemoryBackend::default());
    PandoCredentials::new(Some(backend), |_| None)
}

fn type_into(
    view: &Entity<SettingsView>,
    cx: &mut VisualTestContext,
    pick: impl Fn(&PandoInputs) -> Entity<InputState>,
    text: &str,
) {
    let text = text.to_owned();
    view.update_in(cx, |v, window, cx| {
        let input = pick(&v.pando_inputs);
        input.update(cx, |state, cx| state.set_value(text, window, cx));
    });
}

#[gpui_test]
fn saving_pando_settings_changes_nothing_in_the_graph_folder(cx: &mut TestAppContext) {
    // BIT-SP-0009.R2
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    view.update(cx, |v, cx| {
        v.set_pando_credentials(memory_credentials(), cx)
    });
    let before = tree(&env.root());
    let (seen, _sub) = events(&view, cx);

    view.update(cx, |v, cx| {
        v.set_pando_enabled(true, cx);
        v.set_pando_mode(PandoMode::External, cx);
        v.set_pando_allow_remote(true, cx);
        v.set_pando_feature(PandoFeature::AgentChat, false, cx);
        v.set_pando_agent_writes(true, cx);
    });
    view.update_in(cx, |v, window, cx| {
        v.request_pando_consent(window, cx);
        v.confirm_pending(window, cx);
    });
    type_into(&view, cx, |i| i.exclusion.clone(), "#secret");
    view.update_in(cx, |v, window, cx| v.add_pando_exclusion(window, cx));
    type_into(&view, cx, |i| i.rest_token.clone(), "s3cr3t-token");
    view.update_in(cx, |v, window, cx| {
        v.save_pando_token(TokenKind::Rest, window, cx);
    });

    assert_eq!(tree(&env.root()), before, "the graph folder is untouched");
    let json = std::fs::read_to_string(&file).unwrap();
    assert!(!json.contains("s3cr3t-token"), "{json}");
    assert!(json.contains("#secret") && json.contains("\"granted\": true"));
    assert!(!seen.borrow().is_empty());
}

#[gpui_test]
fn invalid_endpoints_are_refused_and_nothing_is_written(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    let (seen, _sub) = events(&view, cx);
    view.update(cx, |v, _| v.pando.settings.mode = PandoMode::External);
    type_into(&view, cx, |i| i.rest_url.clone(), "http://example.com:8765");
    view.update(cx, |v, cx| v.commit_pando_endpoints(cx));
    assert!(!file.exists());
    assert!(seen.borrow().is_empty());
    let message = view.read_with(cx, |v, _| v.message.clone());
    assert!(matches!(message, Some((Level::Error, _))), "{message:?}");
    // Loopback http is fine.
    type_into(&view, cx, |i| i.rest_url.clone(), "http://127.0.0.1:9000");
    view.update(cx, |v, cx| v.commit_pando_endpoints(cx));
    assert!(file.exists());
    assert_eq!(
        seen.borrow().last(),
        Some(&SettingsEvent::PandoChanged { reopen: true })
    );
}

#[gpui_test]
fn tokens_go_to_the_keychain_and_are_never_shown(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    view.update(cx, |v, cx| {
        v.set_pando_credentials(memory_credentials(), cx)
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.pando_token_source(TokenKind::Rest)),
        None
    );
    type_into(&view, cx, |i| i.rest_token.clone(), "  abc123  ");
    view.update_in(cx, |v, window, cx| {
        v.save_pando_token(TokenKind::Rest, window, cx);
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.pando_token_source(TokenKind::Rest)),
        Some(TokenSource::Keychain)
    );
    // The field is emptied and the file holds no secret.
    let shown = view.read_with(cx, |v, cx| v.text(&v.pando_inputs.rest_token, cx));
    assert!(shown.is_empty());
    assert!(!file.exists() || !std::fs::read_to_string(&file).unwrap().contains("abc123"));
    // An empty token is refused.
    view.update_in(cx, |v, window, cx| {
        v.save_pando_token(TokenKind::Agui, window, cx);
    });
    assert!(matches!(
        view.read_with(cx, |v, _| v.message.clone()),
        Some((Level::Error, _))
    ));
    view.update(cx, |v, cx| v.clear_pando_token(TokenKind::Rest, cx));
    assert_eq!(
        view.read_with(cx, |v, _| v.pando_token_source(TokenKind::Rest)),
        None
    );
}

#[gpui_test]
fn consent_dialog_says_what_is_shared_and_revoke_keeps_exclusions(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, file) = ctx_with_pando(&env);
    let key = env.root().to_string_lossy().into_owned();
    let (view, cx) = view(cx, ctx);
    let (seen, _sub) = events(&view, cx);

    view.update_in(cx, |v, window, cx| v.request_pando_consent(window, cx));
    let pending = view.read_with(cx, |v, _| v.pending().cloned()).unwrap();
    let q = confirmation_for(&pending);
    assert!(
        q.description.contains("shared knowledge base"),
        "{}",
        q.description
    );
    assert!(
        q.description.contains("Any Pando agent"),
        "{}",
        q.description
    );
    assert!(q.description.contains("Bitacora starts"), "managed target");
    // Cancelling records nothing.
    view.update(cx, |v, cx| v.cancel_pending(cx));
    assert!(!file.exists());

    view.update_in(cx, |v, window, cx| {
        v.request_pando_consent(window, cx);
        v.confirm_pending(window, cx);
    });
    let settings = bitacora_runtime::load_pando_settings(&file).unwrap();
    let consent = settings.consent(&key);
    assert!(consent.granted && consent.granted_at.is_some());
    assert!(!consent.agent_writes, "agent writes stay off by default");
    assert_eq!(
        seen.borrow().last(),
        Some(&SettingsEvent::PandoChanged { reopen: true })
    );

    type_into(&view, cx, |i| i.exclusion.clone(), "Journal/Private");
    view.update_in(cx, |v, window, cx| v.add_pando_exclusion(window, cx));
    // Duplicates (any case) and empty entries change nothing.
    type_into(&view, cx, |i| i.exclusion.clone(), "journal/private");
    view.update_in(cx, |v, window, cx| v.add_pando_exclusion(window, cx));
    type_into(&view, cx, |i| i.exclusion.clone(), "  ");
    view.update_in(cx, |v, window, cx| v.add_pando_exclusion(window, cx));
    let consent = bitacora_runtime::load_pando_settings(&file)
        .unwrap()
        .consent(&key);
    assert_eq!(consent.exclusions, vec!["Journal/Private"]);
    assert_eq!(
        seen.borrow().last(),
        Some(&SettingsEvent::PandoChanged { reopen: false }),
        "exclusions apply live, no reopen"
    );

    view.update_in(cx, |v, window, cx| {
        v.request(Pending::RevokePandoConsent { purge: true }, window, cx);
        v.confirm_pending(window, cx);
    });
    let consent = bitacora_runtime::load_pando_settings(&file)
        .unwrap()
        .consent(&key);
    assert!(!consent.granted);
    assert_eq!(consent.exclusions, vec!["Journal/Private"]);
    view.update(cx, |v, cx| v.remove_pando_exclusion("Journal/Private", cx));
    assert!(
        bitacora_runtime::load_pando_settings(&file)
            .unwrap()
            .consent(&key)
            .exclusions
            .is_empty()
    );
}

/// A tiny HTTP server that answers every request with `body`.
fn health_server(body: &'static str) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(4) {
            let Ok(mut stream) = stream else { return };
            let mut buf = [0_u8; 2048];
            let _ = stream.read(&mut buf);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    port
}

fn wait_for_test(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> ConnTest {
    for _ in 0..500 {
        cx.run_until_parked();
        let state = view.read_with(cx, |v, _| v.pando_test().clone());
        if state != ConnTest::Running {
            return state;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("the connection test never finished");
}

#[gpui_test]
fn test_connection_reports_the_version_or_the_failure(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, _file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    view.update(cx, |v, cx| {
        v.set_pando_credentials(memory_credentials(), cx)
    });
    view.update(cx, |v, _| v.pando.settings.mode = PandoMode::External);

    let port = health_server(r#"{"version":"1.2.9","startup_mode":"serve"}"#);
    type_into(
        &view,
        cx,
        |i| i.rest_url.clone(),
        &format!("http://127.0.0.1:{port}"),
    );
    view.update_in(cx, |v, window, cx| v.test_pando(window, cx));
    match wait_for_test(&view, cx) {
        ConnTest::Ok(report) => {
            assert_eq!(report.version, "1.2.9");
            assert!(report.version_ok);
        }
        other => panic!("{other:?}"),
    }

    let port = health_server(r#"{"version":"0.9.0"}"#);
    type_into(
        &view,
        cx,
        |i| i.rest_url.clone(),
        &format!("http://127.0.0.1:{port}"),
    );
    view.update_in(cx, |v, window, cx| v.test_pando(window, cx));
    match wait_for_test(&view, cx) {
        ConnTest::Ok(report) => assert!(!report.version_ok),
        other => panic!("{other:?}"),
    }

    // Nothing listens on a closed port.
    let closed = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    type_into(
        &view,
        cx,
        |i| i.rest_url.clone(),
        &format!("http://127.0.0.1:{closed}"),
    );
    view.update_in(cx, |v, window, cx| v.test_pando(window, cx));
    assert!(matches!(wait_for_test(&view, cx), ConnTest::Failed(_)));
}

#[gpui_test]
fn the_shared_kb_note_follows_the_global_pando_config(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, _file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    let global = env.data.path().join("global.toml");
    view.update(cx, |v, cx| {
        v.set_global_pando_config(Some(global.clone()), cx);
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.pando.sharing.clone()),
        KbSharing::Private
    );
    let abs = env.data.path().join("kb");
    std::fs::write(
        &global,
        format!("[Data]\nDirectory = '{}'\n", abs.display()),
    )
    .unwrap();
    view.update(cx, |v, cx| {
        v.set_global_pando_config(Some(global.clone()), cx);
    });
    assert_eq!(
        view.read_with(cx, |v, _| v.pando.sharing.clone()),
        KbSharing::Shared(abs)
    );
}

#[gpui_test]
fn the_pando_section_renders_in_every_mode_with_and_without_a_graph(cx: &mut TestAppContext) {
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, _file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    view.update(cx, |v, cx| v.select(Section::Pando, cx));
    for mode in [PandoMode::Managed, PandoMode::External, PandoMode::Off] {
        view.update(cx, |v, cx| v.set_pando_mode(mode, cx));
        cx.run_until_parked();
    }
    view.update(cx, |v, cx| v.set_pando_enabled(true, cx));
    view.update_in(cx, |v, window, cx| {
        v.request_pando_consent(window, cx);
        v.confirm_pending(window, cx);
    });
    cx.run_until_parked();
    view.update_in(cx, |v, _, cx| v.set_context(SettingsContext::default(), cx));
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.section()), Section::Pando);
}

#[test]
fn agent_tab_configuration_reads_the_settings_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pando.json");
    let root = dir.path().join("graph");
    assert!(!agent_configured_for(Some(&file), Some(&root)));
    let mut s = bitacora_config::PandoSettings {
        enabled: true,
        ..Default::default()
    };
    s.save(&file).unwrap();
    assert!(
        !agent_configured_for(Some(&file), Some(&root)),
        "no consent"
    );
    s.grant_consent(&root.to_string_lossy(), 1);
    s.save(&file).unwrap();
    assert!(agent_configured_for(Some(&file), Some(&root)));
    assert!(!agent_configured_for(None, Some(&root)));
    assert!(!agent_configured_for(Some(&file), None));
}

#[gpui_test]
fn ai_switches_apply_live_and_are_off_by_default(cx: &mut TestAppContext) {
    // BIT-US-0151, BIT-US-0152: the review and recommendation switches and their automation.
    let env = Env::new(CONFIG);
    setup(cx, None);
    let (ctx, file) = ctx_with_pando(&env);
    let (view, cx) = view(cx, ctx);
    let (seen, _sub) = events(&view, cx);
    view.update(cx, |v, _| {
        let s = v.pando_settings();
        assert!(!s.ai.review_daily && !s.ai.recommend_auto);
        assert!(!s.features.contains_key(&PandoFeature::JournalReview));
    });

    view.update(cx, |v, cx| {
        v.set_pando_ai_auto(|ai| ai.review_daily = true, cx);
        v.set_pando_ai_auto(|ai| ai.review_at_minute = 24 * 60 + 90, cx);
        v.set_pando_ai_auto(|ai| ai.recommend_auto = true, cx);
        v.set_pando_feature(PandoFeature::Recommendations, false, cx);
    });
    let saved = bitacora_config::PandoSettings::load(&file).unwrap();
    assert!(saved.ai.review_daily && saved.ai.recommend_auto);
    assert_eq!(
        saved.ai.review_at_minute,
        23 * 60 + 59,
        "clamped to the day"
    );
    assert_eq!(
        saved.features.get(&PandoFeature::Recommendations),
        Some(&false)
    );
    // None of these restarts the session.
    assert!(
        seen.borrow()
            .iter()
            .all(|e| matches!(e, SettingsEvent::PandoChanged { reopen: false }))
    );
    assert!(!seen.borrow().is_empty());
    // A feature that shapes the session still reopens the graph.
    view.update(cx, |v, cx| {
        v.set_pando_feature(PandoFeature::AgentChat, false, cx)
    });
    assert!(matches!(
        seen.borrow().last(),
        Some(SettingsEvent::PandoChanged { reopen: true })
    ));
}
