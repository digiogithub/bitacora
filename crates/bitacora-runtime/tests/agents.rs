//! AI agent accessors of the session (BIT-SP-0011): approved edits go through the session's
//! command queue, are audited with the MCP agent log and undone through it; chat and runs stay
//! unavailable until Pando is connected and the graph consented.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bitacora_config::{PandoMode, PandoSettings};
use bitacora_core::graph::PageKey;
use bitacora_core::queue::Source;
use bitacora_mcp::AuditFilter;
use bitacora_pando::PandoCredentials;
use bitacora_pando::agents::{ChatConfig, EditApplier, NoHost, Proposal};
use bitacora_runtime::{McpOptions, PandoOptions, RuntimeError, Session};
use serde_json::json;

const UUID: &str = "11111111-1111-4111-8111-111111111111";

fn open(dir: &Path, consent: bool) -> Session {
    let graph = common::graph_with(
        dir,
        &[(
            "pages/Notes.md",
            &format!("- alpha\n  id:: {UUID}\n- beta\n"),
        )],
    );
    let mut cfg = common::config(&graph, &dir.join("data"));
    cfg.mcp = Some(McpOptions {
        config: bitacora_mcp::McpConfig {
            port: 0,
            stateful: false,
            ..bitacora_mcp::McpConfig::default()
        },
        token_path: dir.join("tokens.json"),
        secrets: None,
    });
    let canonical = std::fs::canonicalize(&graph).unwrap();
    let mut settings = PandoSettings {
        enabled: true,
        mode: PandoMode::External,
        ..PandoSettings::default()
    };
    if consent {
        settings.grant_consent(&canonical.to_string_lossy(), 1);
    }
    let mut o = PandoOptions::new(settings, canonical);
    o.credentials = PandoCredentials::new(None, |_| None);
    cfg.pando = Some(o);
    Session::open(cfg).unwrap()
}

#[test]
fn approved_edit_is_audited_undoable_and_attributed_to_the_agent() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(dir.path(), true);
    s.open_page("pages/Notes.md").unwrap();
    let key = PageKey::from_title("Notes");
    let text = |s: &Session| -> Vec<String> {
        common::blocks(s, &key)
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    };
    let before = text(&s);

    let applier = s.agent_edit_applier();
    let proposal = Proposal::parse(&json!({
        "title": "Shout",
        "page": "Notes",
        "ops": [{"op": "update_block", "uuid": UUID,
                 "expected_text": format!("alpha\nid:: {UUID}"), "text": "ALPHA"}]
    }))
    .unwrap();
    let applied = applier.apply(&proposal).unwrap();
    let audit_id = applied.audit_id.clone().expect("audited");
    assert!(text(&s)[0].starts_with("ALPHA"));
    assert!(s.queue().audit().iter().any(|a| a.source == Source::Agent));

    let log = s.agent_activity(&AuditFilter::default());
    let rec = log.iter().find(|r| r.id == audit_id).expect("audit record");
    assert_eq!(rec.tool, "propose_edit");
    assert!(rec.write && rec.undoable);
    assert_eq!(rec.affected, [UUID]);
    assert_eq!(rec.pages, ["Notes"]);

    s.undo_agent_entry(&audit_id).unwrap();
    assert_eq!(text(&s), before);
    s.shutdown(Duration::from_secs(5));
}

#[test]
fn chat_and_runs_are_unavailable_without_a_connection() {
    let dir = tempfile::tempdir().unwrap();
    let s = open(dir.path(), true);
    // The guard follows the consent record.
    assert!(s.agent_guard().has_consent());
    assert!(matches!(
        s.start_chat(ChatConfig::default(), Arc::new(NoHost)),
        Err(RuntimeError::Agent(_))
    ));
    assert!(matches!(s.review_deps(), Err(RuntimeError::Agent(_))));
    assert!(matches!(s.recommend_deps(), Err(RuntimeError::Agent(_))));
    assert!(s.agui_client().is_none());
    s.shutdown(Duration::from_secs(5));

    let dir = tempfile::tempdir().unwrap();
    let s = open(dir.path(), false);
    assert!(!s.agent_guard().has_consent(), "no consent, nothing passes");
    assert!(matches!(s.review_deps(), Err(RuntimeError::Agent(m)) if m.contains("consent")));
    s.shutdown(Duration::from_secs(5));
}
