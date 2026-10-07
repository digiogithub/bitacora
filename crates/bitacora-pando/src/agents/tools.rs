//! The frontend tools Bitacora declares on a run (BIT-T-0457): `propose_edit`, `open_page` and
//! `get_selection`.
//!
//! They are declared in `RunAgentInput.tools`; the model calls them like any tool, the run
//! interrupts, and the chat session ([`super::chat`]) executes them:
//!
//! - `open_page` and `get_selection` go to the app through [`FrontendHost`] (the trait the app
//!   implements). `get_selection` results pass through the [`ContentGuard`].
//! - `propose_edit` never writes by itself: it becomes an approval card and only an explicit
//!   approval reaches the [`super::edits::EditApplier`].

use pando::agui::Tool;
use serde_json::{Value, json};

use super::guard::{AttachedBlock, ContentGuard};

pub use crate::managed::config::PROPOSE_EDIT_TOOL;

/// Opens a page in the app.
pub const OPEN_PAGE_TOOL: &str = "open_page";
/// Returns the blocks the user currently has selected.
pub const GET_SELECTION_TOOL: &str = "get_selection";

/// What the app implements so agents can drive the UI a little. Called from the chat task, so
/// implementations must be `Send + Sync` and quick (hand the work to the UI thread and return).
pub trait FrontendHost: Send + Sync {
    /// Opens the page named `name`. `Ok` carries a small JSON result for the agent.
    ///
    /// # Errors
    /// A message for the agent when the page cannot be opened.
    fn open_page(&self, name: &str) -> Result<Value, String>;

    /// The blocks the user has selected right now (possibly none).
    ///
    /// # Errors
    /// A message for the agent when there is no editor to ask.
    fn get_selection(&self) -> Result<Vec<AttachedBlock>, String>;
}

/// A host that refuses everything; the default for sessions without UI.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHost;

impl FrontendHost for NoHost {
    fn open_page(&self, _name: &str) -> Result<Value, String> {
        Err("no editor is attached".to_owned())
    }

    fn get_selection(&self) -> Result<Vec<AttachedBlock>, String> {
        Err("no editor is attached".to_owned())
    }
}

/// JSON schema of the `propose_edit` arguments (see [`super::edits::Proposal`]).
#[must_use]
pub fn propose_edit_schema() -> Value {
    let uuid = json!({"type": "string", "description": "Block uuid (the `id::` value)."});
    json!({
        "type": "object",
        "required": ["page", "ops"],
        "additionalProperties": false,
        "properties": {
            "title": {"type": "string", "description": "One-line summary for the approval card."},
            "page": {"type": "string", "description": "Title of the page every op belongs to."},
            "ops": {
                "type": "array",
                "minItems": 1,
                "maxItems": super::edits::MAX_OPS,
                "items": {"oneOf": [
                    {
                        "type": "object",
                        "required": ["op", "text"],
                        "properties": {
                            "op": {"const": "insert_block"},
                            "text": {"type": "string", "description": "Text of ONE block, without the leading `- `."},
                            "parent_uuid": uuid,
                            "after_uuid": uuid
                        }
                    },
                    {
                        "type": "object",
                        "required": ["op", "uuid", "expected_text", "text"],
                        "properties": {
                            "op": {"const": "update_block"},
                            "uuid": uuid,
                            "expected_text": {"type": "string", "description": "The block text you read; a changed block makes the proposal stale."},
                            "text": {"type": "string"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["op", "uuid", "target_uuid", "place"],
                        "properties": {
                            "op": {"const": "move_block"},
                            "uuid": uuid,
                            "target_uuid": uuid,
                            "place": {"enum": ["before", "after", "first_child", "last_child"]}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["op", "uuid"],
                        "properties": {
                            "op": {"const": "delete_block"},
                            "uuid": uuid,
                            "expected_text": {"type": "string"}
                        }
                    },
                    {
                        "type": "object",
                        "required": ["op", "uuid", "key", "value"],
                        "properties": {
                            "op": {"const": "set_property"},
                            "uuid": uuid,
                            "key": {"type": "string"},
                            "value": {"type": "string"}
                        }
                    }
                ]}
            }
        }
    })
}

/// The frontend tools of a session. `propose_edit` only for profiles allowed to propose edits
/// (`bitacora-writer`).
#[must_use]
pub fn tool_set(with_propose_edit: bool) -> Vec<Tool> {
    let mut tools = vec![
        Tool {
            name: OPEN_PAGE_TOOL.to_owned(),
            description: "Open a page of the user's graph in the editor so they can see it."
                .to_owned(),
            parameters: json!({
                "type": "object",
                "required": ["name"],
                "additionalProperties": false,
                "properties": {"name": {"type": "string", "description": "Page title."}}
            }),
        },
        Tool {
            name: GET_SELECTION_TOOL.to_owned(),
            description: "Return the blocks the user currently has selected (empty when none)."
                .to_owned(),
            parameters: json!({"type": "object", "properties": {}, "additionalProperties": false}),
        },
    ];
    if with_propose_edit {
        tools.push(Tool {
            name: PROPOSE_EDIT_TOOL.to_owned(),
            description: "Propose changes to ONE page. Nothing is written until the user approves \
                          the diff; the result tells you whether they did. Address blocks by uuid \
                          and quote the text you read in `expected_text`."
                .to_owned(),
            parameters: propose_edit_schema(),
        });
    }
    tools
}

/// Runs `get_selection` through the guard: only allowed blocks reach the agent.
#[must_use]
pub fn selection_result(guard: &ContentGuard, blocks: &[AttachedBlock]) -> Value {
    let allowed = guard.filter(blocks);
    let withheld = blocks.len().saturating_sub(allowed.len());
    json!({
        "blocks": allowed.iter().map(|b| json!({
            "page": b.page,
            "uuid": b.uuid,
            "text": b.text,
        })).collect::<Vec<_>>(),
        "withheld": withheld,
    })
}

/// Result of a `get_selection` call.
///
/// # Errors
/// The host's error message.
pub fn run_get_selection(host: &dyn FrontendHost, guard: &ContentGuard) -> Result<Value, String> {
    host.get_selection()
        .map(|blocks| selection_result(guard, &blocks))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use bitacora_config::pando::GraphConsent;

    #[test]
    fn tool_set_declares_propose_edit_only_when_asked() {
        let names = |t: Vec<Tool>| t.into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert_eq!(names(tool_set(false)), ["open_page", "get_selection"]);
        assert_eq!(
            names(tool_set(true)),
            ["open_page", "get_selection", "propose_edit"]
        );
        let schema = propose_edit_schema();
        assert_eq!(schema["required"], json!(["page", "ops"]));
        assert_eq!(
            schema["properties"]["ops"]["items"]["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
    }

    #[test]
    fn selection_is_guarded() {
        let guard = ContentGuard::from_consent(&GraphConsent {
            granted: true,
            exclusions: vec!["Secrets".into()],
            ..GraphConsent::default()
        });
        let mk = |page: &str, text: &str| AttachedBlock {
            page: page.into(),
            file_path: format!("pages/{page}.md"),
            tags: Vec::new(),
            uuid: None,
            text: text.into(),
            page_private: false,
        };
        let v = selection_result(&guard, &[mk("Open", "a"), mk("Secrets", "b")]);
        assert_eq!(v["blocks"].as_array().unwrap().len(), 1);
        assert_eq!(v["withheld"], 1);
        assert!(NoHost.get_selection().is_err());
    }
}
