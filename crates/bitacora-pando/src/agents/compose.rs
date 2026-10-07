//! Compose and continue runs of the editor (BIT-US-0153, BIT-SP-0011.R6): the "Compose with AI"
//! box and the inline ghost text.
//!
//! Both are one-shot runs of the `bitacora-writer` profile with no frontend tools: the answer is
//! plain text the user accepts or discards, never an edit the agent applies. Nothing is sent for
//! a page the [`ContentGuard`] hides (no consent, excluded name, path or tag, `private:: true`):
//! the run is refused before any request is made. The preview streams: `on_preview` receives
//! the whole text so far each time it grows.

use std::sync::Arc;
use std::time::Duration;

use bitacora_index::IndexReader;
use pando::agui::{AguiClient, Event, RunOutcome, Thread};

use super::AgentError;
use super::chat::WRITER_PROFILE;
use super::guard::{AttachedBlock, ContentGuard};
use super::runs::{fail_closed_answer, final_text};

/// Budget of a compose run.
pub const COMPOSE_TIMEOUT: Duration = Duration::from_secs(90);
/// Budget of a ghost-text continuation (it is worthless when late).
pub const CONTINUE_TIMEOUT: Duration = Duration::from_secs(20);
/// Most interrupt rounds a run may need before it is given up.
const MAX_ROUNDS: usize = 4;
/// Longest continuation kept, in characters.
pub const MAX_CONTINUATION_CHARS: usize = 600;

/// What the run is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeMode {
    /// The user typed an instruction in the compose box.
    Compose,
    /// Continue the block text after the caret (ghost text).
    Continue,
}

/// One compose or continue request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeRequest {
    /// What to do.
    pub mode: ComposeMode,
    /// Title of the page holding the block.
    pub page: String,
    /// The instruction (compose); ignored for [`ComposeMode::Continue`].
    pub instruction: String,
    /// Text of the block (for a continuation: the text before the caret).
    pub block_text: String,
    /// Send the block text as context (the "this block" chip). A continuation always does.
    pub include_block: bool,
}

/// What the gate needs to know about a page: where it lives and how it is marked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageMeta {
    /// Graph-relative path of the page's file.
    pub file_path: String,
    /// Tags of the page.
    pub tags: Vec<String>,
    /// The page carries `private:: true`.
    pub private: bool,
}

/// Reads of the page a request is about.
pub trait PageLookup: Send + Sync {
    /// Metadata of the page titled `title`; `None` when it is not indexed (a page that has no
    /// file yet cannot carry exclusions by path or tag, only by name).
    fn page(&self, title: &str) -> Option<PageMeta>;
}

impl PageLookup for IndexReader {
    fn page(&self, title: &str) -> Option<PageMeta> {
        let row = self.page_by_name(title).ok().flatten()?;
        let file_path = row.file_path?;
        let blocks = self.semantic_blocks(&file_path).unwrap_or_default();
        let mut tags: Vec<String> = Vec::new();
        let mut private = false;
        for b in &blocks {
            for t in &b.tags {
                if !tags.contains(t) {
                    tags.push(t.clone());
                }
            }
            private |= b.page_properties.iter().any(|(k, v)| {
                k.trim().eq_ignore_ascii_case("private") && v.trim().eq_ignore_ascii_case("true")
            });
        }
        Some(PageMeta {
            file_path,
            tags,
            private,
        })
    }
}

/// Everything a compose run needs.
#[derive(Clone)]
pub struct ComposeDeps {
    /// AG-UI client of the connected Pando.
    pub agui: AguiClient,
    /// Consent and exclusions.
    pub guard: ContentGuard,
    /// Page metadata for the guard.
    pub pages: Arc<dyn PageLookup>,
}

impl std::fmt::Debug for ComposeDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComposeDeps").finish_non_exhaustive()
    }
}

impl ComposeDeps {
    /// Dependencies from their parts.
    #[must_use]
    pub fn new(agui: AguiClient, guard: ContentGuard, pages: Arc<dyn PageLookup>) -> Self {
        Self { agui, guard, pages }
    }
}

/// Whether a request about `req.page` may be sent at all; returns the block as the guard saw it.
///
/// # Errors
/// [`AgentError::Unavailable`] when there is no consent or the page or block is hidden.
pub fn check_allowed(
    deps: &ComposeDeps,
    req: &ComposeRequest,
) -> Result<AttachedBlock, AgentError> {
    if !deps.guard.has_consent() {
        return Err(AgentError::Unavailable(
            "the graph has not consented to sharing content with Pando agents".into(),
        ));
    }
    let meta = deps.pages.page(&req.page).unwrap_or_default();
    let block = AttachedBlock {
        page: req.page.clone(),
        file_path: meta.file_path,
        tags: meta.tags,
        uuid: None,
        text: req.block_text.clone(),
        page_private: meta.private,
    };
    if !deps.guard.allows(&block) {
        return Err(AgentError::Unavailable(
            "this page or block is excluded from AI features".into(),
        ));
    }
    Ok(block)
}

/// The prompt of a request. The block text travels as context, not in the prompt.
#[must_use]
pub fn prompt_for(req: &ComposeRequest) -> String {
    const FORMAT: &str = "Reply with only the text to write, as Logseq Markdown (no code fences, \
        no quotes around it, no preamble, no explanation). Do not use tools.";
    match req.mode {
        ComposeMode::Compose => {
            let ctx = if req.include_block {
                " The current block is attached as context."
            } else {
                ""
            };
            format!(
                "Write text for a note in an outliner.{ctx} Task: {}\n{FORMAT}",
                req.instruction.trim()
            )
        }
        ComposeMode::Continue => format!(
            "Continue the attached text exactly where it stops, in the same language and tone. \
             Write at most two sentences and do not repeat the text.\n{FORMAT}"
        ),
    }
}

/// Cleans an answer for insertion: strips a wrapping fence or quotes and trims; a continuation
/// keeps the leading space the agent wrote (it is glued to the text before it) and is capped.
#[must_use]
pub fn clean_answer(mode: ComposeMode, raw: &str) -> String {
    let mut t = raw.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let body = rest.split_once('\n').map_or("", |(_, b)| b).trim_end();
        t = body.strip_suffix("```").unwrap_or(body).trim();
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') && !t[1..t.len() - 1].contains('"') {
        t = &t[1..t.len() - 1];
    }
    let mut out = t.to_owned();
    if mode == ComposeMode::Continue {
        out = out.chars().take(MAX_CONTINUATION_CHARS).collect();
        if raw.starts_with(' ') && !out.is_empty() {
            out.insert(0, ' ');
        }
    }
    out
}

/// Cancels the run on the server when the future is dropped before it finished.
struct CancelOnDrop {
    agui: AguiClient,
    thread_id: String,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let agui = self.agui.clone();
            let id = std::mem::take(&mut self.thread_id);
            rt.spawn(async move {
                let _ = agui.cancel_run(&id).await;
            });
        }
    }
}

/// Runs a compose or continue request and returns the cleaned answer. Dropping the future
/// (aborting the task) cancels the run on the server.
///
/// `on_preview` receives the answer so far every time it grows.
///
/// # Errors
/// [`AgentError::Unavailable`] when the guard refuses (nothing is sent), [`AgentError::Pando`],
/// [`AgentError::Timeout`], and [`AgentError::InvalidOutput`] for an empty answer.
pub async fn run_compose(
    deps: &ComposeDeps,
    req: &ComposeRequest,
    mut on_preview: impl FnMut(&str) + Send,
) -> Result<String, AgentError> {
    let block = check_allowed(deps, req)?;
    let context = if req.include_block || req.mode == ComposeMode::Continue {
        deps.guard.context_entries(&[block])
    } else {
        Vec::new()
    };
    let mut thread = Thread::new(deps.agui.clone()).with_agent(WRITER_PROFILE);
    thread.set_context(context);
    let mut cancel = CancelOnDrop {
        agui: deps.agui.clone(),
        thread_id: thread.thread_id().to_owned(),
        armed: true,
    };
    let budget = match req.mode {
        ComposeMode::Compose => COMPOSE_TIMEOUT,
        ComposeMode::Continue => CONTINUE_TIMEOUT,
    };
    let prompt = prompt_for(req);
    let result = tokio::time::timeout(budget, drive(&mut thread, &prompt, &mut on_preview)).await;
    match result {
        Ok(Ok(())) => cancel.armed = false,
        Ok(Err(e)) => {
            cancel.armed = false;
            return Err(e);
        }
        // `cancel` stays armed: the run is cancelled on the server when it drops.
        Err(_) => return Err(AgentError::Timeout),
    }
    let raw = final_text(&thread.messages)
        .ok_or_else(|| AgentError::InvalidOutput("the agent produced no text".into()))?;
    let text = clean_answer(req.mode, &raw);
    if text.trim().is_empty() {
        return Err(AgentError::InvalidOutput(
            "the agent answered nothing".into(),
        ));
    }
    Ok(text)
}

async fn drive(
    thread: &mut Thread,
    prompt: &str,
    on_preview: &mut (impl FnMut(&str) + Send),
) -> Result<(), AgentError> {
    let mut preview = String::new();
    let mut outcome = stream(thread.send(prompt).await?, &mut preview, on_preview).await?;
    let mut rounds = 0;
    while outcome == RunOutcome::Interrupted {
        rounds += 1;
        if rounds > MAX_ROUNDS {
            return Err(AgentError::InvalidOutput(
                "the agent kept asking for permission".into(),
            ));
        }
        // Compose reads nothing beyond its prompt: every permission is denied.
        let Some(answer) = fail_closed_answer(thread, false) else {
            break;
        };
        outcome = stream(thread.resume_with(answer).await?, &mut preview, on_preview).await?;
    }
    Ok(())
}

async fn stream(
    mut run: pando::agui::ThreadRun<'_>,
    preview: &mut String,
    on_preview: &mut (impl FnMut(&str) + Send),
) -> Result<RunOutcome, AgentError> {
    let mut outcome = None;
    while let Some(event) = run.next().await {
        match event? {
            Event::TextMessageStart { .. } => preview.clear(),
            Event::TextMessageContent { delta, .. } if !delta.is_empty() => {
                preview.push_str(&delta);
                on_preview(preview);
            }
            Event::RunFinished { outcome: o, .. } => {
                outcome = Some(if o.as_deref() == Some(pando::agui::OUTCOME_INTERRUPT) {
                    RunOutcome::Interrupted
                } else {
                    RunOutcome::Finished
                });
            }
            Event::RunError { message, code } => {
                return Err(pando::Error::Run { code, message }.into());
            }
            _ => {}
        }
    }
    outcome.ok_or_else(|| pando::Error::Protocol("stream ended before RUN_FINISHED".into()).into())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn req(mode: ComposeMode) -> ComposeRequest {
        ComposeRequest {
            mode,
            page: "Notes".into(),
            instruction: "  summarise  ".into(),
            block_text: "hello".into(),
            include_block: true,
        }
    }

    #[test]
    fn prompts_name_the_task_and_never_embed_the_block() {
        let p = prompt_for(&req(ComposeMode::Compose));
        assert!(p.contains("Task: summarise"));
        assert!(p.contains("attached as context"));
        assert!(!p.contains("hello"));
        let c = prompt_for(&req(ComposeMode::Continue));
        assert!(c.contains("Continue the attached text"));
        assert!(!c.contains("Task:"));
    }

    #[test]
    fn answers_are_cleaned() {
        assert_eq!(
            clean_answer(ComposeMode::Compose, "```md\nA\nB\n```"),
            "A\nB"
        );
        assert_eq!(clean_answer(ComposeMode::Compose, "\"quoted\""), "quoted");
        assert_eq!(
            clean_answer(ComposeMode::Compose, "say \"a\" and \"b\""),
            "say \"a\" and \"b\""
        );
        assert_eq!(
            clean_answer(ComposeMode::Continue, " and more"),
            " and more"
        );
        let long = "x".repeat(MAX_CONTINUATION_CHARS + 10);
        assert_eq!(
            clean_answer(ComposeMode::Continue, &long).chars().count(),
            MAX_CONTINUATION_CHARS
        );
    }
}
