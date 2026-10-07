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
}

impl AgentContext {
    pub(crate) fn from_options(p: &PandoOptions) -> Self {
        Self {
            consent: p.settings.consent(&p.graph_key()),
            chat_enabled: p.settings.enabled && p.settings.feature_enabled(PandoFeature::AgentChat),
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
