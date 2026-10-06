//! Write and delete tool implementations: argument/result types and the glue from tool arguments
//! to [`QueueBridge`] calls. Policy checks (toggle, scope, rate limit) live in `handler`.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bridge::{
    Affected, Applied, Env, NewBlock, Position, QueueBridge, RenameArgs, validate_content,
    validate_property,
};
use crate::policy::MAX_BLOCKS_PER_CALL;
use crate::render::{Code, ToolError, ToolResult, output};
use crate::tools::check_graph;

/// Names of the tools that need the `write` scope.
pub(crate) const WRITE_TOOLS: &[&str] = &[
    "create_page",
    "append_block",
    "prepend_block",
    "insert_block",
    "update_block",
    "set_block_property",
    "remove_block_property",
    "move_block",
    "set_task_status",
    "git_sync_now",
];

/// Names of the tools that need the `delete` scope.
pub(crate) const DELETE_TOOLS: &[&str] = &["remove_block", "rename_page", "delete_page"];

/// A block to create, with optional properties and nested children.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct BlockArg {
    /// Logseq Markdown of ONE block: first line plus continuation lines, no leading `- `.
    pub content: String,
    /// Extra `key:: value` properties (`id` and `collapsed` are managed by Bitacora).
    pub properties: Option<BTreeMap<String, String>>,
    /// Nested child blocks.
    pub children: Option<Vec<BlockArg>>,
}

/// Where a block goes relative to the target block.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PositionArg {
    After,
    Before,
    FirstChild,
    LastChild,
}

impl From<PositionArg> for Position {
    fn from(p: PositionArg) -> Self {
        match p {
            PositionArg::After => Self::After,
            PositionArg::Before => Self::Before,
            PositionArg::FirstChild => Self::FirstChild,
            PositionArg::LastChild => Self::LastChild,
        }
    }
}

/// What `create_page` does when the page already exists.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum IfExists {
    #[default]
    Error,
    Return,
}

/// Arguments of `create_page`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct CreatePageArgs {
    /// Page title (a journal title creates the journal page of that day).
    pub name: String,
    /// Page properties (the first, un-bulleted block).
    pub properties: Option<BTreeMap<String, String>>,
    /// Initial blocks (trees allowed).
    pub blocks: Option<Vec<BlockArg>>,
    /// Accepted for Logseq compatibility; journal titles are detected from `name`.
    #[allow(dead_code)]
    pub journal: Option<bool>,
    /// `error` (default) or `return` the existing page.
    pub if_exists: Option<IfExists>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `append_block` and `prepend_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct AddBlockArgs {
    /// Page name, or `today` for today's journal. A missing page is created.
    pub page: String,
    /// Text of one block (use `blocks` for several).
    pub content: Option<String>,
    /// Properties of the `content` block.
    pub properties: Option<BTreeMap<String, String>>,
    /// Children of the `content` block.
    pub children: Option<Vec<BlockArg>>,
    /// Several blocks (trees allowed), inserted in order.
    pub blocks: Option<Vec<BlockArg>>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `insert_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct InsertBlockArgs {
    /// Block the new blocks are placed relative to.
    pub target_uuid: String,
    /// Text of one block (use `blocks` for several).
    pub content: Option<String>,
    /// Properties of the `content` block.
    pub properties: Option<BTreeMap<String, String>>,
    /// Children of the `content` block.
    pub children: Option<Vec<BlockArg>>,
    /// Several blocks (trees allowed), inserted in order.
    pub blocks: Option<Vec<BlockArg>>,
    /// `after` (default), `before`, `first_child` or `last_child`.
    pub position: Option<PositionArg>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `update_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct UpdateBlockArgs {
    /// Block uuid.
    pub uuid: String,
    /// New text of the block (the `id::` and `collapsed::` lines are kept).
    pub content: String,
    /// Properties merged into the block.
    pub properties: Option<BTreeMap<String, String>>,
    /// Version from a previous read; a mismatch gives `CONFLICT` with the current block.
    pub expected_version: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `set_block_property`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct SetPropertyArgs {
    /// Block uuid.
    pub uuid: String,
    /// Property key (`id` and `collapsed` are managed by Bitacora).
    pub key: String,
    /// Property value (one line).
    pub value: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `remove_block_property`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct RemovePropertyArgs {
    /// Block uuid.
    pub uuid: String,
    /// Property key.
    pub key: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `move_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct MoveBlockArgs {
    /// Block to move (with its children).
    pub uuid: String,
    /// Block it is placed relative to.
    pub target_uuid: String,
    /// `after`, `before`, `first_child` or `last_child`.
    pub position: PositionArg,
    /// Version of the moved block from a previous read.
    pub expected_version: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `set_task_status`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct TaskStatusArgs {
    /// Block uuid.
    pub uuid: String,
    /// TODO, DOING, DONE, LATER, NOW, WAITING, CANCELED, CANCELLED, or `none` to clear.
    pub status: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `remove_block`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct RemoveBlockArgs {
    /// Block uuid (its children are removed too).
    pub uuid: String,
    /// Version from a previous read.
    pub expected_version: Option<String>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `rename_page`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct RenamePageArgs {
    /// Current page name or alias.
    pub name: String,
    /// New title.
    pub new_name: String,
    /// Rewrite `[[links]]` and `#tags` in other pages (default true; `false` is not supported).
    pub update_links: Option<bool>,
    /// Merge into the target page when it already exists (default false: `CONFLICT`).
    pub merge: Option<bool>,
    /// With `merge`: carry the source page's `alias::` values over (default false; dropped
    /// aliases are reported in `details.dropped_aliases`).
    pub keep_aliases: Option<bool>,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `delete_page`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub(crate) struct DeletePageArgs {
    /// Page name or alias.
    pub name: String,
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Arguments of `git_sync_now`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub(crate) struct GitSyncArgs {
    /// Graph name or path; default is the active graph.
    pub graph: Option<String>,
}

/// Result of every write tool.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct WriteOut {
    pub ok: bool,
    /// Canonical page name.
    pub page: Option<String>,
    /// Graph-relative file of the page, when it has one.
    pub file: Option<String>,
    /// The call created the page.
    pub created: bool,
    /// Blocks created, changed, moved or removed, with their new versions.
    pub blocks: Vec<Affected>,
    /// Tool specific details (`rename_page`: rewritten pages).
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
    /// Audit entry of this call; undo it from the app's agent activity view.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
}

fn props(p: Option<BTreeMap<String, String>>) -> Result<Vec<(String, String)>, ToolError> {
    let mut out = Vec::new();
    for (k, v) in p.unwrap_or_default() {
        validate_property(&k, &v)?;
        out.push((k, v));
    }
    Ok(out)
}

fn convert(arg: BlockArg, depth: usize) -> Result<NewBlock, ToolError> {
    if depth > 12 {
        return Err(ToolError::new(
            Code::InvalidContent,
            "blocks are nested too deeply",
        ));
    }
    let content = validate_content(&arg.content)?;
    let children = arg
        .children
        .unwrap_or_default()
        .into_iter()
        .map(|c| convert(c, depth + 1))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(NewBlock {
        content,
        properties: props(arg.properties)?,
        children,
    })
}

fn check_total(blocks: &[NewBlock]) -> Result<(), ToolError> {
    let total: usize = blocks.iter().map(NewBlock::count).sum();
    if total > MAX_BLOCKS_PER_CALL {
        return Err(ToolError::new(
            Code::InvalidContent,
            format!("{total} blocks in one call; the limit is {MAX_BLOCKS_PER_CALL}"),
        ));
    }
    Ok(())
}

/// Blocks of a call: the single `content` block followed by `blocks[]`. At most
/// [`MAX_BLOCKS_PER_CALL`] blocks in total.
pub(crate) fn collect_blocks(
    content: Option<String>,
    properties: Option<BTreeMap<String, String>>,
    children: Option<Vec<BlockArg>>,
    blocks: Option<Vec<BlockArg>>,
) -> Result<Vec<NewBlock>, ToolError> {
    let mut out = Vec::new();
    if let Some(c) = content {
        out.push(convert(
            BlockArg {
                content: c,
                properties,
                children,
            },
            0,
        )?);
    } else if properties.is_some() || children.is_some() {
        return Err(ToolError::new(
            Code::InvalidContent,
            "`properties` and `children` need `content`",
        ));
    }
    for b in blocks.unwrap_or_default() {
        out.push(convert(b, 0)?);
    }
    if out.is_empty() {
        return Err(ToolError::new(
            Code::InvalidContent,
            "give `content` or `blocks`",
        ));
    }
    check_total(&out)?;
    Ok(out)
}

/// Encodes an [`Applied`] result.
pub(crate) fn applied_output(a: &Applied, summary: &str) -> ToolResult {
    let mut md = format!("{summary}\n");
    if let Some(p) = &a.page {
        md.push_str(&format!("- page: [[{p}]]\n"));
    }
    for b in &a.affected {
        md.push_str(&format!(
            "- block {} (version {})\n",
            b.uuid,
            b.version.as_deref().unwrap_or("removed")
        ));
    }
    output(
        &WriteOut {
            ok: true,
            page: a.page.clone(),
            file: a.file.clone(),
            created: a.created,
            blocks: a.affected.clone(),
            details: a.extra.clone(),
            audit_id: None,
        },
        md,
    )
}

// ---- tool bodies (run on the blocking pool)

pub(crate) fn create_page(
    b: &QueueBridge,
    env: &Env<'_>,
    a: CreatePageArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    let props = props(a.properties)?;
    let mut blocks = Vec::new();
    for x in a.blocks.unwrap_or_default() {
        blocks.push(convert(x, 0)?);
    }
    check_total(&blocks)?;
    b.create_page(
        env,
        &a.name,
        &props,
        &blocks,
        a.if_exists.unwrap_or_default() == IfExists::Error,
    )
}

pub(crate) fn add_block(
    b: &QueueBridge,
    env: &Env<'_>,
    a: AddBlockArgs,
    prepend: bool,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    let blocks = collect_blocks(a.content, a.properties, a.children, a.blocks)?;
    b.add_to_page(env, &a.page, &blocks, prepend)
}

pub(crate) fn insert_block(
    b: &QueueBridge,
    env: &Env<'_>,
    a: InsertBlockArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    let blocks = collect_blocks(a.content, a.properties, a.children, a.blocks)?;
    b.insert_block(
        env,
        &a.target_uuid,
        a.position.unwrap_or(PositionArg::After).into(),
        &blocks,
    )
}

pub(crate) fn update_block(
    b: &QueueBridge,
    env: &Env<'_>,
    a: UpdateBlockArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    let props = props(a.properties)?;
    b.update_block(
        env,
        &a.uuid,
        &a.content,
        &props,
        a.expected_version.as_deref(),
    )
}

pub(crate) fn set_property(
    b: &QueueBridge,
    env: &Env<'_>,
    a: SetPropertyArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.set_property(env, &a.uuid, &a.key, Some(&a.value))
}

pub(crate) fn remove_property(
    b: &QueueBridge,
    env: &Env<'_>,
    a: RemovePropertyArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.set_property(env, &a.uuid, &a.key, None)
}

pub(crate) fn move_block(
    b: &QueueBridge,
    env: &Env<'_>,
    a: MoveBlockArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.move_block(
        env,
        &a.uuid,
        &a.target_uuid,
        a.position.into(),
        a.expected_version.as_deref(),
    )
}

pub(crate) fn set_task_status(
    b: &QueueBridge,
    env: &Env<'_>,
    a: TaskStatusArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.set_task_status(env, &a.uuid, &a.status)
}

pub(crate) fn remove_block(
    b: &QueueBridge,
    env: &Env<'_>,
    a: RemoveBlockArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.remove_block(env, &a.uuid, a.expected_version.as_deref())
}

pub(crate) fn rename_page(
    b: &QueueBridge,
    env: &Env<'_>,
    a: RenamePageArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.rename_page(
        env,
        &RenameArgs {
            name: &a.name,
            new_name: &a.new_name,
            update_links: a.update_links.unwrap_or(true),
            merge: a.merge.unwrap_or(false),
            keep_aliases: a.keep_aliases.unwrap_or(false),
        },
    )
}

pub(crate) fn delete_page(
    b: &QueueBridge,
    env: &Env<'_>,
    a: DeletePageArgs,
) -> Result<Applied, ToolError> {
    check_graph(env.r, a.graph.as_deref())?;
    b.delete_page(env, &a.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(c: &str) -> BlockArg {
        BlockArg {
            content: c.into(),
            properties: None,
            children: None,
        }
    }

    #[test]
    fn block_limit_counts_children() {
        let kids: Vec<BlockArg> = (0..MAX_BLOCKS_PER_CALL)
            .map(|i| arg(&format!("c{i}")))
            .collect();
        let r = collect_blocks(Some("root".into()), None, Some(kids), None);
        assert!(matches!(r, Err(e) if e.code == Code::InvalidContent));
        let ok = collect_blocks(None, None, None, Some(vec![arg("a"), arg("b")]));
        assert_eq!(ok.expect("ok").len(), 2);
    }

    #[test]
    fn needs_some_content() {
        assert!(collect_blocks(None, None, None, None).is_err());
        assert!(collect_blocks(None, Some(BTreeMap::new()), None, None).is_err());
    }
}
