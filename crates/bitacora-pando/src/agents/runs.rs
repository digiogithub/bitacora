//! One-shot structured runs: send a prompt to a profile, collect the answer, never let the agent
//! block on the user (BIT-T-0461, BIT-T-0464).
//!
//! Review and recommendation runs have no UI to answer prompts, so every interrupt is resolved
//! fail-closed: questions are cancelled and permission prompts denied, except the prompts of the
//! read-only Bitacora and knowledge-base tools of the profile allow-list (a real Pando asks before
//! every MCP call, so denying them would blind the agent). Calls Pando lists as pending that are not
//! prompts (its own parked MCP/KB calls) are never answered.

use std::time::Duration;

use pando::agui::hitl::{self, Interrupt, PermissionRequest};
use pando::agui::{AguiClient, ContextEntry, Message, MessageContent, RunOutcome, Thread, role};
use serde_json::Value;

use super::AgentError;
use crate::managed::config::{KB_READ_TOOLS, MCP_READ_TOOLS};

/// Default budget of a one-shot run.
pub const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(300);
/// Most interrupt rounds a one-shot run may need before it is given up.
const MAX_ROUNDS: usize = 40;

/// The answer of a one-shot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunText {
    /// Thread the run used (a fresh one).
    pub thread_id: String,
    /// Text of the last assistant message that had any.
    pub text: String,
}

/// Runs `prompt` on `profile` in a fresh thread and returns the final assistant text.
///
/// `context` is attached as given: callers pass only what already went through the
/// [`super::guard::ContentGuard`]. The run is cancelled on the server when `timeout` passes.
///
/// # Errors
/// [`AgentError::Pando`] for transport and run errors, [`AgentError::Timeout`], and
/// [`AgentError::InvalidOutput`] when the agent never produced text.
pub async fn run_once(
    agui: &AguiClient,
    profile: &str,
    prompt: &str,
    context: Vec<ContextEntry>,
    timeout: Duration,
) -> Result<RunText, AgentError> {
    let mut thread = Thread::new(agui.clone()).with_agent(profile);
    thread.set_context(context);
    let thread_id = thread.thread_id().to_owned();
    let result = tokio::time::timeout(timeout, drive(&mut thread, prompt)).await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            let _ = agui.cancel_run(&thread_id).await;
            return Err(AgentError::Timeout);
        }
    }
    let text = final_text(&thread.messages)
        .ok_or_else(|| AgentError::InvalidOutput("the agent produced no text".into()))?;
    Ok(RunText { thread_id, text })
}

async fn drive(thread: &mut Thread, prompt: &str) -> Result<(), AgentError> {
    let mut outcome = thread.send(prompt).await?.drain().await?;
    let mut rounds = 0;
    while outcome == RunOutcome::Interrupted {
        rounds += 1;
        if rounds > MAX_ROUNDS {
            return Err(AgentError::InvalidOutput(
                "the agent kept asking for permission".into(),
            ));
        }
        let Some(answer) = fail_closed_answer(thread, true) else {
            break;
        };
        outcome = thread.resume_with(answer).await?.drain().await?;
    }
    Ok(())
}

/// Whether a permission prompt asks to run one of the read-only tools Bitacora's profiles allow:
/// the `bitacora_*` read tools (served with the read-only `pando` token, exclusions applied
/// server-side) and Pando's KB read tools. Prompts that demand explicit approval never qualify.
#[must_use]
pub fn is_read_only_tool(req: &PermissionRequest) -> bool {
    !req.require_explicit_approval
        && !req.never_auto_approve
        && (MCP_READ_TOOLS.contains(&req.tool_name.as_str())
            || KB_READ_TOOLS.contains(&req.tool_name.as_str()))
}

/// The answer of a headless run to the first prompt Pando is waiting on: approve an allow-listed
/// read tool (when `approve_reads`), deny any other permission, cancel any question. Pending
/// calls that are not prompts are Pando's own parked tool calls and are never answered.
pub(super) fn fail_closed_answer(thread: &Thread, approve_reads: bool) -> Option<Message> {
    thread.interrupts().into_iter().find_map(|i| match i {
        Interrupt::Permission {
            tool_call_id,
            request,
        } => {
            let ok = approve_reads && is_read_only_tool(&request);
            Some(Message::tool_result(
                &tool_call_id,
                if ok { hitl::approve() } else { hitl::deny() },
            ))
        }
        Interrupt::Question { tool_call_id, .. } => {
            Some(Message::tool_result(&tool_call_id, hitl::cancel_question()))
        }
        _ => None,
    })
}

pub(super) fn final_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .filter(|m| m.role == role::ASSISTANT)
        .find_map(|m| match &m.content {
            MessageContent::Text(t) if !t.trim().is_empty() => Some(t.clone()),
            _ => None,
        })
}

/// Extracts the JSON object an agent wrote, tolerating prose around it and Markdown fences.
///
/// Tries, in order: the whole text, fenced blocks, then every balanced `{...}` span (outermost
/// first). Returns the first that parses to a JSON object.
#[must_use]
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    let mut rest = t;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let body_start = after.find('\n').map_or(0, |i| i + 1);
        let body = &after[body_start..];
        let Some(close) = body.find("```") else { break };
        if let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(body[..close].trim()) {
            return Some(v);
        }
        rest = &body[close + 3..];
    }
    let bytes = t.as_bytes();
    let mut from = 0;
    while let Some(rel) = t[from..].find('{') {
        let start = from + rel;
        if let Some(end) = balanced_end(bytes, start)
            && let Ok(v @ Value::Object(_)) = serde_json::from_str::<Value>(&t[start..=end])
        {
            return Some(v);
        }
        from = start + 1;
    }
    None
}

/// Index of the `}` that closes the `{` at `start`, honouring strings and escapes.
fn balanced_end(b: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for (i, &c) in b.iter().enumerate().skip(start) {
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Strings of a lenient JSON value: a string, an array of strings, or an array of objects whose
/// first present key of `keys` holds the text.
#[must_use]
pub fn lenient_strings(v: Option<&Value>, keys: &[&str]) -> Vec<String> {
    let one = |x: &Value| -> Option<String> {
        match x {
            Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
            Value::Object(o) => keys
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str))
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_owned()),
            _ => None,
        }
    };
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(one).collect(),
        Some(x) => one(x).into_iter().collect(),
        None => Vec::new(),
    }
}

/// The first present key of `keys` in `obj`.
#[must_use]
pub fn pick<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| obj.get(*k))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_json_from_prose_fences_and_braces_in_strings() {
        assert_eq!(extract_json(r#"{"a":1}"#), Some(json!({"a": 1})));
        assert_eq!(
            extract_json("Here you go:\n```json\n{\"a\": {\"b\": 2}}\n```\nDone."),
            Some(json!({"a": {"b": 2}}))
        );
        assert_eq!(
            extract_json("prefix {\"s\": \"a } b\", \"n\": [1]} suffix"),
            Some(json!({"s": "a } b", "n": [1]}))
        );
        // A first fence that is not JSON is skipped.
        assert_eq!(
            extract_json("```text\nnope\n```\n```\n{\"ok\":true}\n```"),
            Some(json!({"ok": true}))
        );
        assert_eq!(extract_json("no json at all"), None);
        assert_eq!(extract_json("[1,2,3]"), None);
        assert_eq!(extract_json("{unclosed"), None);
    }

    #[test]
    fn lenient_strings_accept_strings_arrays_and_objects() {
        assert_eq!(lenient_strings(Some(&json!("a")), &["x"]), ["a"]);
        assert_eq!(
            lenient_strings(
                Some(&json!(["a", {"name": "b"}, {"zzz": 1}, 3, " "])),
                &["name"]
            ),
            ["a", "b"]
        );
        assert!(lenient_strings(None, &["x"]).is_empty());
    }
}
