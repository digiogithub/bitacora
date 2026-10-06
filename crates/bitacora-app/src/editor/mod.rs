//! The block editor (BIT-EP-0007): one block in edit mode inside the page outline.
//!
//! * [`OutlineEditor`] owns the editing state of a page and talks to the core command queue.
//! * [`element`] paints the block in edit mode and implements IME input.
//! * [`actions`] declares the key actions and contexts; bindings are in `default.json`.
//! * [`outline`], [`autopair`], [`html`], [`style`], [`buffer`], [`layout`] and [`text_ops`]
//!   are the pure helpers.

pub mod actions;
pub mod autopair;
pub mod buffer;
pub mod completion;
pub mod element;
pub mod html;
pub mod layout;
pub mod outline;
pub mod row;
pub mod style;
pub mod text_ops;
pub mod view;

#[cfg(test)]
mod tests;

pub use row::RowEdit;
pub use view::{Caret, EditorEvent, OutlineEditor};

use bitacora_config::EffectiveConfig;
use bitacora_core::graph::PageKey;
use bitacora_core::graph_path::GraphPath;
use bitacora_core::queue::{CommandQueue, Request, Response, Source};

use crate::data::GraphHandle;
use crate::ui::App;

/// Makes sure core holds the page titled `title` so it can be edited: a page already loaded
/// (also a virtual one, such as today's journal) is used as is, otherwise its file is read and
/// loaded. A page without a file gets a virtual page in core (`Request::OpenPage`): it is
/// editable at once and the file is only created once it has content.
pub fn ensure_loaded(
    queue: &CommandQueue,
    handle: &GraphHandle,
    config: &EffectiveConfig,
    title: &str,
) -> Option<PageKey> {
    let direct = PageKey::from_title(title);
    if queue.snapshot(&direct).is_some() {
        return Some(direct);
    }
    let page = handle.reader.page_by_name(title).ok().flatten();
    let loaded = page.and_then(|page| {
        let key = PageKey::from_title(&page.original_name);
        if queue.snapshot(&key).is_some() {
            return Some(key);
        }
        let path = GraphPath::new(&page.file_path?).ok()?;
        let bytes = std::fs::read(path.to_fs_path(&handle.root)).ok()?;
        queue
            .execute(
                Source::Ui,
                Request::LoadPage {
                    key: key.clone(),
                    title: page.original_name,
                    path: Some(path),
                    bytes,
                },
            )
            .ok()?;
        Some(key)
    });
    if loaded.is_some() {
        return loaded;
    }
    match queue
        .execute(
            Source::Ui,
            Request::OpenPage {
                title: title.to_owned(),
                cfg: Box::new(config.clone()),
                graph: None,
            },
        )
        .ok()?
    {
        Response::Opened(o) => Some(o.key().clone()),
        _ => None,
    }
}

/// Installs the bindings that depend on the platform (word motion modifiers). The rest of the
/// editor keymap is the `outliner` part of `assets/keymaps/default.json`.
pub fn bind_platform_keys(cx: &mut App) {
    let mut bindings = Vec::new();
    for (keys, action, context) in actions::platform_bindings() {
        let built = match cx.build_action(action, None) {
            Ok(a) => a,
            Err(err) => {
                tracing::warn!(action, "editor keymap: unknown action: {err}");
                continue;
            }
        };
        match crate::ui::key_binding(&keys, built, Some(context)) {
            Ok(b) => bindings.push(b),
            Err(err) => tracing::warn!(keys, "editor keymap: invalid binding: {err}"),
        }
    }
    cx.bind_keys(bindings);
}
