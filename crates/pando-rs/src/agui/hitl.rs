//! Human-in-the-loop interrupts: permission prompts and `AskUserQuestion`.
//!
//! Both reach the client as a pending tool call (the run ends with `RUN_FINISHED{outcome:
//! "interrupt"}`) and are answered with a `tool` message on the same thread. The helpers here only
//! build the answer payload; deliver it with [`crate::agui::Thread::resume`]. Shapes verified
//! against Pando `internal/agui/hitl.go` (`approvalFromMessage`, `answerFromMessage`).
//!
//! Pando fails closed: anything that is not an explicit approval is a denial, and an unanswered
//! question is a cancellation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::thread::PendingToolCall;

/// The synthetic tool a permission prompt arrives as.
pub const PERMISSION_TOOL_NAME: &str = "pando_permission_request";
/// The tool a question prompt arrives as.
pub const QUESTION_TOOL_NAME: &str = "AskUserQuestion";

/// Arguments of a [`PERMISSION_TOOL_NAME`] call.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PermissionRequest {
    /// Tool asking for permission.
    pub tool_name: String,
    /// Action (for example `write`, `execute_unsandboxed`).
    pub action: String,
    /// Human-readable description.
    pub description: String,
    /// Path the action touches.
    pub path: String,
    /// Tool parameters (arbitrary).
    pub params: Value,
    /// The agent's justification (sandbox escalation).
    pub justification: String,
    /// Only an explicit answer approves; render as a warning.
    pub require_explicit_approval: bool,
    /// Auto-approve policies must not answer this one.
    pub never_auto_approve: bool,
}

/// One selectable option of a question.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct QuestionOption {
    /// Label.
    pub label: String,
    /// Description.
    pub description: String,
}

/// One question.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Question {
    /// Question text.
    pub question: String,
    /// Short header.
    pub header: String,
    /// Several options may be selected.
    pub multi_select: bool,
    /// Options.
    pub options: Vec<QuestionOption>,
}

/// Arguments of an [`QUESTION_TOOL_NAME`] call.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct QuestionRequest {
    /// The questions.
    pub questions: Vec<Question>,
}

/// One answered question.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAnswerEntry {
    /// Question identifier (the header or the question text).
    pub question_id: String,
    /// Header shown to the model in place of the id.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub header: String,
    /// Selected option labels.
    pub selected: Vec<String>,
    /// Free text ("other").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub other_text: String,
}

/// Answer to a question prompt.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct QuestionAnswer {
    /// The user did not answer; the model proceeds on its own judgement.
    pub cancelled: bool,
    /// Answers, ignored when `cancelled`.
    pub answers: Vec<QuestionAnswerEntry>,
}

/// What a pending tool call is, from the client's point of view.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Interrupt {
    /// A permission prompt: answer with [`approve`] or [`deny`].
    Permission {
        /// Call id to resume with.
        tool_call_id: String,
        /// The decoded request.
        request: PermissionRequest,
    },
    /// An `AskUserQuestion`: answer with [`answer_question`] or [`cancel_question`].
    Question {
        /// Call id to resume with.
        tool_call_id: String,
        /// The decoded request.
        request: QuestionRequest,
    },
    /// A frontend tool the caller declared: execute it and answer with its output.
    FrontendTool(PendingToolCall),
}

impl Interrupt {
    /// Classifies a pending call. A prompt whose arguments cannot be decoded still classifies as
    /// that prompt (with an empty request) so the caller can deny/cancel it explicitly.
    pub fn classify(call: &PendingToolCall) -> Self {
        match call.name.as_str() {
            PERMISSION_TOOL_NAME => Self::Permission {
                tool_call_id: call.id.clone(),
                request: serde_json::from_str(&call.args_text).unwrap_or_default(),
            },
            QUESTION_TOOL_NAME => Self::Question {
                tool_call_id: call.id.clone(),
                request: serde_json::from_str(&call.args_text).unwrap_or_default(),
            },
            _ => Self::FrontendTool(call.clone()),
        }
    }

    /// The call id to resume with.
    pub fn tool_call_id(&self) -> &str {
        match self {
            Self::Permission { tool_call_id, .. } | Self::Question { tool_call_id, .. } => {
                tool_call_id
            }
            Self::FrontendTool(call) => &call.id,
        }
    }
}

/// The canonical approval payload (`{"approved":true}`).
pub fn approve() -> String {
    r#"{"approved":true}"#.to_owned()
}

/// The canonical denial payload (`{"approved":false}`). Denial is also what the server assumes
/// when no answer arrives.
pub fn deny() -> String {
    r#"{"approved":false}"#.to_owned()
}

/// Serializes a question answer.
pub fn answer_question(answer: &QuestionAnswer) -> String {
    serde_json::to_string(answer).unwrap_or_else(|_| cancel_question())
}

/// The cancellation payload for a question.
pub fn cancel_question() -> String {
    r#"{"cancelled":true,"answers":[]}"#.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_match_the_server_contract() {
        assert_eq!(approve(), r#"{"approved":true}"#);
        assert_eq!(deny(), r#"{"approved":false}"#);
        let a = QuestionAnswer {
            cancelled: false,
            answers: vec![QuestionAnswerEntry {
                question_id: "q1".into(),
                selected: vec!["yes".into()],
                ..Default::default()
            }],
        };
        let v: Value = serde_json::from_str(&answer_question(&a)).unwrap();
        assert_eq!(v["answers"][0]["questionId"], "q1");
        assert!(v["answers"][0].get("header").is_none());
        assert_eq!(v["cancelled"], false);
    }
}
