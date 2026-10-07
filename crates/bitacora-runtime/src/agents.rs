//! Thin glue for the AI agents of `bitacora-pando` (BIT-SP-0011): consent context, AG-UI client
//! and the audit adapter. The session accessors that use these live on [`crate::Session`].

use std::sync::Arc;

use bitacora_config::{GraphConsent, PandoFeature};
use bitacora_core::queue::CommandQueue;
use bitacora_mcp::{AgentWrite, McpServer};
use bitacora_pando::agents::{AppliedEdit, AuditSink, ContentGuard, QueueEditApplier};
use bitacora_pando::semantic::{ContentPolicy, SharedPolicy};
use bitacora_pando::{PandoOptions, PandoService, PandoStatus};

/// What the agents need to know about the Pando settings of this graph.
#[derive(Debug, Clone)]
pub(crate) struct AgentContext {
    pub consent: GraphConsent,
    pub chat_enabled: bool,
    /// Graph key and settings file, to persist remembered tool decisions.
    pub graph_key: String,
    pub settings_file: Option<std::path::PathBuf>,
}

impl AgentContext {
    pub(crate) fn from_options(p: &PandoOptions) -> Self {
        Self {
            consent: p.settings.consent(&p.graph_key()),
            chat_enabled: p.settings.enabled && p.settings.feature_enabled(PandoFeature::AgentChat),
            graph_key: p.graph_key(),
            settings_file: p.settings_file.clone(),
        }
    }
}

/// Remembered "allow/deny always" tool decisions of this graph. They are read from the session's
/// live consent record (so revoking consent or forgetting one in Settings applies at once, and
/// nothing is honoured without consent) and persisted to the machine-local `pando.json`.
pub(crate) struct SessionToolMemory {
    pub agent: Arc<parking_lot::RwLock<AgentContext>>,
}

impl bitacora_pando::agents::ToolMemory for SessionToolMemory {
    fn decision(&self, tool: &str) -> Option<bool> {
        let a = self.agent.read();
        if !a.consent.granted {
            return None;
        }
        a.consent.tool_decisions.get(tool).copied()
    }

    fn remember(&self, tool: &str, allow: bool) {
        let (key, file) = {
            let mut a = self.agent.write();
            if !a.consent.granted || tool.trim().is_empty() {
                return;
            }
            a.consent.tool_decisions.insert(tool.to_owned(), allow);
            (a.graph_key.clone(), a.settings_file.clone())
        };
        let Some(file) = file else { return };
        let result = bitacora_config::PandoSettings::load(&file).and_then(|mut s| {
            s.remember_tool_decision(&key, tool, allow);
            s.save(&file)
        });
        if let Err(e) = result {
            tracing::warn!("cannot persist the remembered decision for {tool}: {e}");
        }
    }
}

/// The guard: the semantic worker's live policy when there is one, else the consent record.
pub(crate) fn guard(ctx: Option<&AgentContext>, live: Option<SharedPolicy>) -> ContentGuard {
    let Some(ctx) = ctx else {
        return ContentGuard::new(false, ContentPolicy::default());
    };
    match live {
        Some(policy) => ContentGuard::new(ctx.consent.granted, policy.read().clone()),
        None => ContentGuard::from_consent(&ctx.consent),
    }
}

/// An AG-UI client for the connected service.
pub(crate) fn agui_client(service: Option<&PandoService>) -> Option<pando::agui::AguiClient> {
    let service = service?;
    if !matches!(service.status(), PandoStatus::Connected { .. }) {
        return None;
    }
    let ep = service.endpoints()?;
    let client = pando::PandoClient::new(ep.rest.clone()).ok()?;
    let mut opts = pando::agui::AguiOptions::default().with_base_url(ep.agui_url.clone());
    if let Some(t) = ep.agui_token.clone() {
        opts = opts.with_token(t);
    }
    Some(client.agui_with(opts))
}

/// Records applied edits in the MCP server's audit log so they show up in the agent activity list
/// and can be undone with `Session::undo_agent_entry`.
struct McpAudit {
    log: Arc<bitacora_mcp::AuditLog>,
    queue: CommandQueue,
}

impl AuditSink for McpAudit {
    fn record_edit(&self, summary: &str, edit: &AppliedEdit) -> Option<String> {
        Some(self.log.record_agent_write(
            &self.queue,
            AgentWrite {
                client: "bitacora-chat".to_owned(),
                tool: "propose_edit".to_owned(),
                summary: summary.to_owned(),
                affected: edit.affected.clone(),
                pages: vec![edit.page.clone()],
                txs: edit.txs.clone(),
            },
        ))
    }
}

pub(crate) fn edit_applier(queue: &CommandQueue, mcp: Option<&McpServer>) -> QueueEditApplier {
    let applier = QueueEditApplier::new(queue.clone());
    match mcp {
        Some(m) => applier.with_audit(Arc::new(McpAudit {
            log: Arc::clone(m.audit()),
            queue: queue.clone(),
        })),
        None => applier,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use bitacora_pando::agents::ToolMemory;

    use super::*;

    #[test]
    fn remembered_decisions_persist_and_vanish_with_consent() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pando.json");
        let mut s = bitacora_config::PandoSettings::default();
        s.grant_consent("/g", 1);
        s.save(&file).unwrap();
        let agent = Arc::new(parking_lot::RwLock::new(AgentContext {
            consent: s.consent("/g"),
            chat_enabled: true,
            graph_key: "/g".into(),
            settings_file: Some(file.clone()),
        }));
        let mem = SessionToolMemory {
            agent: Arc::clone(&agent),
        };
        assert_eq!(mem.decision("propose_edit"), None);
        mem.remember("propose_edit", true);
        mem.remember("bash", false);
        assert_eq!(mem.decision("propose_edit"), Some(true));
        assert_eq!(mem.decision("bash"), Some(false));
        let saved = bitacora_config::PandoSettings::load(&file).unwrap();
        assert_eq!(saved.consent("/g").tool_decisions.len(), 2);

        // Revoking consent clears them in the file and nothing is honoured meanwhile.
        let mut revoked = saved;
        revoked.revoke_consent("/g");
        agent.write().consent = revoked.consent("/g");
        assert_eq!(mem.decision("propose_edit"), None);
        assert!(revoked.consent("/g").tool_decisions.is_empty());
        // Without consent nothing is remembered.
        mem.remember("bash", true);
        assert_eq!(mem.decision("bash"), None);
    }
}
