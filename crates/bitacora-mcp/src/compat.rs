//! Optional Logseq-compatible `POST /api` endpoint (BIT-SP-0007.R17; design `mcp-server.md`
//! section 2, `05-git-and-apis.md` section 2.2).
//!
//! Off by default (the route does not exist, so the answer is `404`). When enabled it sits behind
//! the same Host / Origin / bearer guard as `/mcp`, applies the same scopes, toggles, rate limit
//! and protected pages, and every call is audited like an MCP tool call (tool name
//! `api:<method>`).
//!
//! Request: `{"method": "logseq.Editor.getBlock", "args": [...]}`. Like Logseq's server, failures
//! are `{"error": "<message>"}` with HTTP 200. Results use camelCase keys.
//!
//! Supported methods: `Editor.getPage|getBlock|getPageBlocksTree|insertBlock|appendBlockInPage|
//! prependBlockInPage|updateBlock|moveBlock|removeBlock|createPage|renamePage|deletePage|
//! getPageLinkedReferences|upsertBlockProperty|removeBlockProperty`, `DB.q`, and
//! `search`/`App.search`. Everything else (UI, `Git.*`, `App.relaunch/quit`, plugin and
//! `DB.datascriptQuery` methods) answers `{"error":"method not supported"}`.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};
use axum::routing::post;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::audit::{CallBase, CallInfo, summarize_args};
use crate::bridge::{Applied, Env, QueueBridge};
use crate::handler::{Services, exec_write, forbidden, report_applied};
use crate::reader::GraphReader;
use crate::render::{Code, ToolError, ToolResult};
use crate::tokens::{Scope, TokenInfo};
use crate::tools::{
    self, BacklinksArgs, GetBlockArgs, GetPageArgs, QueryArgs, SearchArgs, TreeArgs,
};
use crate::write_tools::{
    self, AddBlockArgs, CreatePageArgs, DeletePageArgs, InsertBlockArgs, MoveBlockArgs,
    RemoveBlockArgs, RemovePropertyArgs, RenamePageArgs, SetPropertyArgs, UpdateBlockArgs,
};

/// The `/api` route with its state applied.
pub(crate) fn router(svc: Arc<Services>) -> Router {
    Router::new().route("/api", post(api)).with_state(svc)
}

#[derive(Deserialize)]
struct ApiRequest {
    method: String,
    #[serde(default)]
    args: Vec<Value>,
}

fn error_json(msg: impl Into<String>) -> Value {
    json!({ "error": msg.into() })
}

async fn api(
    State(svc): State<Arc<Services>>,
    Extension(token): Extension<TokenInfo>,
    body: Bytes,
) -> Response {
    let req: ApiRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(error_json(format!("invalid request: {e}"))),
            )
                .into_response();
        }
    };
    let args_json = json!({ "args": req.args });
    let (args_hash, _) = summarize_args(args_json.as_object());
    let base = CallBase {
        token: Some(token.name.clone()),
        client: Some("logseq-api".to_owned()),
        tool: format!("api:{}", req.method),
        args_hash,
        args: format!("{} args", req.args.len()),
    };
    let mut info = CallInfo::default();
    let (value, code) = match dispatch(&svc, &token, &req.method, &req.args, &mut info).await {
        Ok(v) => (v, "ok".to_owned()),
        Err(e) if e.code == Code::NotSupported && e.message == UNSUPPORTED => {
            (error_json(UNSUPPORTED), "NOT_SUPPORTED".to_owned())
        }
        Err(e) => (
            error_json(format!("{}: {}", e.code.as_str(), e.message)),
            e.code.as_str().to_owned(),
        ),
    };
    svc.audit.record_call(base, info, &code);
    axum::Json(value).into_response()
}

const UNSUPPORTED: &str = "method not supported";

fn unsupported() -> ToolError {
    ToolError::new(Code::NotSupported, UNSUPPORTED)
}

// ---------------------------------------------------------------- argument helpers

fn arg_str(args: &[Value], i: usize, what: &str) -> Result<String, ToolError> {
    args.get(i)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ToolError::invalid(format!("argument {} must be {what}", i + 1)))
}

fn opts(args: &[Value], i: usize) -> serde_json::Map<String, Value> {
    args.get(i)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

fn flag(o: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    o.get(key).and_then(Value::as_bool)
}

/// `{"a": 1, "b": "x"}` to `{"a": "1", "b": "x"}`.
fn string_map(v: Option<&Value>) -> Value {
    let Some(Value::Object(o)) = v else {
        return Value::Null;
    };
    Value::Object(
        o.iter()
            .map(|(k, v)| {
                let s = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), Value::String(s))
            })
            .collect(),
    )
}

fn looks_like_uuid(s: &str) -> bool {
    s.len() == 36 && s.bytes().filter(|b| *b == b'-').count() == 4
}

fn parse<A: DeserializeOwned>(v: Value) -> Result<A, ToolError> {
    serde_json::from_value(v).map_err(|e| ToolError::invalid(e.to_string()))
}

// ---------------------------------------------------------------- result shaping

fn camel_key(k: &str) -> String {
    let mut out = String::with_capacity(k.len());
    let mut up = false;
    for c in k.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.extend(c.to_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// snake_case keys to camelCase; `page: "Name"` strings become `{"name": "Name"}` like Logseq's
/// block entities.
fn camel(v: Value) -> Value {
    match v {
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| {
                    let v = if k == "page" {
                        match v {
                            Value::String(s) => json!({ "name": s }),
                            other => camel(other),
                        }
                    } else {
                        camel(v)
                    };
                    (camel_key(&k), v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(camel).collect()),
        other => other,
    }
}

// ---------------------------------------------------------------- execution helpers

async fn read<F>(svc: &Arc<Services>, token: &TokenInfo, f: F) -> Result<Value, ToolError>
where
    F: FnOnce(&dyn GraphReader) -> ToolResult + Send + 'static,
{
    if !token.scopes.contains(&Scope::Read) {
        return Err(forbidden(Scope::Read));
    }
    let reader = Arc::clone(&svc.reader);
    tokio::task::spawn_blocking(move || f(&*reader))
        .await
        .map_err(|e| ToolError::new(Code::Internal, format!("reader task failed: {e}")))?
        .map(|o| o.value)
}

/// A missing entity is `null` in Logseq's API.
fn null_if_missing(r: Result<Value, ToolError>) -> Result<Value, ToolError> {
    match r {
        Err(e) if e.code == Code::NotFound => Ok(Value::Null),
        other => other,
    }
}

async fn write<F>(
    svc: &Arc<Services>,
    token: &TokenInfo,
    delete: bool,
    info: &mut CallInfo,
    f: F,
) -> Result<Applied, ToolError>
where
    F: FnOnce(&QueueBridge, &Env<'_>) -> Result<Applied, ToolError> + Send + 'static,
{
    let applied = exec_write(svc, Some(token), delete, f).await?;
    // Share the audit bookkeeping with the MCP path: move the transactions into `info`.
    let slot = Arc::new(parking_lot::Mutex::new(CallInfo::default()));
    let affected = applied.affected.clone();
    let page = applied.page.clone();
    report_applied(&slot, applied);
    *info = std::mem::take(&mut *slot.lock());
    Ok(Applied {
        affected,
        page,
        ..Applied::default()
    })
}

fn entity(a: &Applied, content: &str) -> Value {
    match a.affected.first() {
        Some(b) => json!({
            "uuid": b.uuid,
            "content": content,
            "page": { "name": b.page },
        }),
        None => Value::Null,
    }
}

async fn dispatch(
    svc: &Arc<Services>,
    token: &TokenInfo,
    method: &str,
    args: &[Value],
    info: &mut CallInfo,
) -> Result<Value, ToolError> {
    let Some(name) = method.strip_prefix("logseq.") else {
        return Err(unsupported());
    };
    match name {
        "Editor.getPage" => {
            let a: GetPageArgs = parse(json!({ "name": arg_str(args, 0, "a page name")? }))?;
            null_if_missing(
                read(svc, token, move |r| tools::get_page(r, a))
                    .await
                    .map(camel),
            )
        }
        "Editor.getBlock" => {
            let o = opts(args, 1);
            let a: GetBlockArgs = parse(json!({
                "uuid": arg_str(args, 0, "a block uuid")?,
                "include_children": flag(&o, "includeChildren"),
            }))?;
            let v = read(svc, token, move |r| tools::get_block(r, a)).await;
            null_if_missing(v.map(|v| camel(v.get("block").cloned().unwrap_or(Value::Null))))
        }
        "Editor.getPageBlocksTree" => {
            let a: TreeArgs = parse(json!({ "name": arg_str(args, 0, "a page name")? }))?;
            let v = read(svc, token, move |r| tools::get_page_blocks_tree(r, a)).await;
            null_if_missing(v.map(|v| camel(v.get("blocks").cloned().unwrap_or(Value::Null))))
        }
        "Editor.getPageLinkedReferences" => {
            let a: BacklinksArgs = parse(json!({ "name": arg_str(args, 0, "a page name")? }))?;
            let v = read(svc, token, move |r| tools::backlinks(r, a)).await?;
            let groups = v
                .get("groups")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            Ok(Value::Array(
                groups
                    .into_iter()
                    .map(|g| {
                        let page = g.get("page").cloned().unwrap_or(Value::Null);
                        let blocks: Vec<Value> = g
                            .get("blocks")
                            .and_then(Value::as_array)
                            .map(|bs| {
                                bs.iter()
                                    .map(|b| camel(b.get("block").cloned().unwrap_or(Value::Null)))
                                    .collect()
                            })
                            .unwrap_or_default();
                        json!([{ "name": page }, blocks])
                    })
                    .collect(),
            ))
        }
        "DB.q" => {
            let a: QueryArgs = parse(json!({ "dsl": arg_str(args, 0, "a query string")? }))?;
            let v = read(svc, token, move |r| tools::query_tool(r, a)).await?;
            Ok(camel(
                v.get("blocks")
                    .cloned()
                    .filter(|b| b.as_array().is_some_and(|a| !a.is_empty()))
                    .or_else(|| v.get("pages").cloned())
                    .unwrap_or(Value::Array(Vec::new())),
            ))
        }
        "search" | "App.search" => {
            let a: SearchArgs = parse(json!({ "query": arg_str(args, 0, "a search string")? }))?;
            let v = read(svc, token, move |r| tools::search(r, a)).await?;
            Ok(camel(v.get("hits").cloned().unwrap_or(Value::Null)))
        }
        "Editor.insertBlock" => {
            let src = arg_str(args, 0, "a block uuid or page name")?;
            let content = arg_str(args, 1, "the block content")?;
            let o = opts(args, 2);
            let properties = string_map(o.get("properties"));
            let before = flag(&o, "before").unwrap_or(false);
            let sibling = flag(&o, "sibling").unwrap_or(true);
            let shown = content.clone();
            let applied = if looks_like_uuid(&src) {
                let position = if before {
                    "before"
                } else if sibling {
                    "after"
                } else {
                    "last_child"
                };
                let a: InsertBlockArgs = parse(json!({
                    "target_uuid": src, "content": content,
                    "properties": properties, "position": position,
                }))?;
                write(svc, token, false, info, move |b, env| {
                    write_tools::insert_block(b, env, a)
                })
                .await?
            } else {
                let a: AddBlockArgs = parse(json!({
                    "page": src, "content": content, "properties": properties,
                }))?;
                write(svc, token, false, info, move |b, env| {
                    write_tools::add_block(b, env, a, before)
                })
                .await?
            };
            Ok(entity(&applied, &shown))
        }
        "Editor.appendBlockInPage" | "Editor.prependBlockInPage" => {
            let page = arg_str(args, 0, "a page name")?;
            let content = arg_str(args, 1, "the block content")?;
            let o = opts(args, 2);
            let a: AddBlockArgs = parse(json!({
                "page": page, "content": content,
                "properties": string_map(o.get("properties")),
            }))?;
            let prepend = name == "Editor.prependBlockInPage";
            let applied = write(svc, token, false, info, move |b, env| {
                write_tools::add_block(b, env, a, prepend)
            })
            .await?;
            Ok(entity(&applied, &content))
        }
        "Editor.updateBlock" => {
            let o = opts(args, 2);
            let a: UpdateBlockArgs = parse(json!({
                "uuid": arg_str(args, 0, "a block uuid")?,
                "content": arg_str(args, 1, "the block content")?,
                "properties": string_map(o.get("properties")),
            }))?;
            write(svc, token, false, info, move |b, env| {
                write_tools::update_block(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.moveBlock" => {
            let o = opts(args, 2);
            let position = if flag(&o, "before").unwrap_or(false) {
                "before"
            } else if flag(&o, "children").unwrap_or(false) {
                "last_child"
            } else {
                "after"
            };
            let a: MoveBlockArgs = parse(json!({
                "uuid": arg_str(args, 0, "a block uuid")?,
                "target_uuid": arg_str(args, 1, "the target block uuid")?,
                "position": position,
            }))?;
            write(svc, token, false, info, move |b, env| {
                write_tools::move_block(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.removeBlock" => {
            let a: RemoveBlockArgs = parse(json!({ "uuid": arg_str(args, 0, "a block uuid")? }))?;
            write(svc, token, true, info, move |b, env| {
                write_tools::remove_block(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.createPage" => {
            let a: CreatePageArgs = parse(json!({
                "name": arg_str(args, 0, "a page name")?,
                "properties": string_map(args.get(1)),
                "if_exists": "return",
            }))?;
            let applied = write(svc, token, false, info, move |b, env| {
                write_tools::create_page(b, env, a)
            })
            .await?;
            Ok(json!({ "name": applied.page, "originalName": applied.page }))
        }
        "Editor.renamePage" => {
            let a: RenamePageArgs = parse(json!({
                "name": arg_str(args, 0, "the page name")?,
                "new_name": arg_str(args, 1, "the new page name")?,
            }))?;
            write(svc, token, true, info, move |b, env| {
                write_tools::rename_page(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.deletePage" => {
            let a: DeletePageArgs = parse(json!({ "name": arg_str(args, 0, "a page name")? }))?;
            write(svc, token, true, info, move |b, env| {
                write_tools::delete_page(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.upsertBlockProperty" => {
            let value = match args.get(2) {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => return Err(ToolError::invalid("argument 3 must be the property value")),
            };
            let a: SetPropertyArgs = parse(json!({
                "uuid": arg_str(args, 0, "a block uuid")?,
                "key": arg_str(args, 1, "the property key")?,
                "value": value,
            }))?;
            write(svc, token, false, info, move |b, env| {
                write_tools::set_property(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        "Editor.removeBlockProperty" => {
            let a: RemovePropertyArgs = parse(json!({
                "uuid": arg_str(args, 0, "a block uuid")?,
                "key": arg_str(args, 1, "the property key")?,
            }))?;
            write(svc, token, false, info, move |b, env| {
                write_tools::remove_property(b, env, a)
            })
            .await?;
            Ok(Value::Null)
        }
        _ => Err(unsupported()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camel_case_keys_and_page_names() {
        let v = camel(json!({
            "original_name": "A",
            "page": "P",
            "children": [{ "journal_day": 1, "page": "P" }]
        }));
        assert_eq!(v["originalName"], "A");
        assert_eq!(v["page"]["name"], "P");
        assert_eq!(v["children"][0]["journalDay"], 1);
        assert_eq!(v["children"][0]["page"]["name"], "P");
    }

    #[test]
    fn uuid_detection() {
        assert!(looks_like_uuid("11111111-1111-4111-8111-111111111111"));
        assert!(!looks_like_uuid("Project X"));
    }
}
