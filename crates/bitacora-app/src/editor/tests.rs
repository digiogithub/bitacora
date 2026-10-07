//! Keystroke-level `#[gpui::test]` suite of the block editor (BIT-US-0030..0037, 0039).
//!
//! Every test opens a real graph in a temp folder with a live runtime session, shows a page in
//! a `PageView` (which makes it editable through core) and drives the editor with simulated
//! keystrokes, then checks the core snapshot and the bytes on disk.

use std::sync::Arc;
use std::time::Duration;

use bitacora_core::editor::BlockId;
use bitacora_core::graph::PageKey;
use bitacora_core::queue::{CommandQueue, Source};
use bitacora_runtime::{RuntimeConfig, Session};

use super::{Caret, OutlineEditor};
use crate::data::{GraphHandle, ViewSettings};
use crate::nav::Route;
use crate::session::SessionLink;
use crate::settings::AppSettings;
use crate::theme;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::text_edit::EntityInputHandler as _;
use crate::ui::{Entity, px, size};
use crate::views::page_view::PageView;

struct Env {
    graph: tempfile::TempDir,
    _data: tempfile::TempDir,
    session: Option<Session>,
    handle: GraphHandle,
    link: SessionLink,
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(s) = self.session.take() {
            let _ = s.shutdown(Duration::from_secs(10));
        }
    }
}

impl Env {
    fn new(files: &[(&str, &str)]) -> Self {
        let graph = tempfile::tempdir().expect("graph");
        let data = tempfile::tempdir().expect("data");
        let root = graph.path();
        std::fs::create_dir_all(root.join("logseq")).expect("logseq");
        std::fs::write(root.join("logseq/config.edn"), "{}").expect("config");
        for (path, content) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
            std::fs::write(file, content).expect("write");
        }
        let mut cfg = RuntimeConfig::new(root);
        cfg.data_dir = Some(data.path().to_path_buf());
        cfg.global_config = Some(data.path().join("no-global.edn"));
        cfg.watch = None;
        cfg.debounce = None;
        let session = Session::open(cfg).expect("session");
        let handle = GraphHandle {
            reader: session.read_api(),
            root: session.root().to_path_buf(),
            settings: Arc::new(ViewSettings::from_config(session.config())),
        };
        let link = SessionLink {
            queue: session.queue().clone(),
            config: Arc::new(session.config().clone()),
            mcp_endpoint: None,
            gate: Arc::default(),
            lookup: session.ref_lookup(),
            hybrid: None,
        };
        Self {
            graph,
            _data: data,
            session: Some(session),
            handle,
            link,
        }
    }

    fn queue(&self) -> &CommandQueue {
        &self.link.queue
    }

    /// Writes dirty pages and reads `rel` from disk.
    fn disk(&self, rel: &str) -> String {
        let _ = self.queue().flush(Source::Ui).expect("flush");
        std::fs::read_to_string(self.graph.path().join(rel)).expect("read page")
    }

    fn snapshot_texts(&self, title: &str) -> Vec<(usize, String)> {
        self.queue()
            .snapshot(&PageKey::from_title(title))
            .map(|s| s.blocks.iter().map(|b| (b.depth, b.text.clone())).collect())
            .unwrap_or_default()
    }
}

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        crate::keymap::load_with_user(cx, None).expect("keymap");
    });
}

fn open_page<'a>(
    cx: &'a mut TestAppContext,
    env: &Env,
    page: &str,
) -> (
    Entity<PageView>,
    Entity<OutlineEditor>,
    &'a mut VisualTestContext,
) {
    setup(cx);
    open_page_without_setup(cx, env, page)
}

fn open_page_without_setup<'a>(
    cx: &'a mut TestAppContext,
    env: &Env,
    page: &str,
) -> (
    Entity<PageView>,
    Entity<OutlineEditor>,
    &'a mut VisualTestContext,
) {
    let (view, cx) = cx.add_window_view(|_, cx| PageView::new(cx));
    cx.simulate_resize(size(px(840.), px(700.)));
    let link = env.link.clone();
    view.update_in(cx, |v, window, cx| {
        v.set_session_link(Some(link), window, cx);
    });
    let (handle, route) = (env.handle.clone(), Route::Page(page.into()));
    view.update(cx, |v, cx| v.show(handle, route, None, cx));
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if view.read_with(cx, |v, _| v.is_live()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(view.read_with(cx, |v, _| v.is_live()), "page is editable");
    let ed = view.read_with(cx, |v, _| v.editor().cloned().expect("editor"));
    (view, ed, cx)
}

fn ids(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) -> Vec<BlockId> {
    ed.read_with(cx, |e, _| e.block_ids().to_vec())
}

fn edit(ed: &Entity<OutlineEditor>, row: usize, caret: Caret, cx: &mut VisualTestContext) {
    let id = ids(ed, cx)[row];
    ed.update_in(cx, |e, window, cx| e.enter(id, caret, window, cx));
    cx.run_until_parked();
}

fn flush(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) {
    ed.update(cx, |e, cx| {
        e.flush(cx);
    });
}

fn buffer(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) -> Option<String> {
    ed.read_with(cx, |e, _| e.buffer_text().map(str::to_owned))
}

fn editing_row(ed: &Entity<OutlineEditor>, cx: &mut VisualTestContext) -> Option<usize> {
    ed.read_with(cx, |e, _| {
        let id = e.editing()?;
        e.block_ids().iter().position(|i| *i == id)
    })
}

/// Keystrokes with the platform "secondary" modifier (Cmd on macOS, Ctrl elsewhere); the keymap
/// binds `secondary-...`.
fn k(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("ctrl-", "cmd-")
    } else {
        keys.to_owned()
    }
}

/// Keystrokes with the word-motion modifier (Alt on macOS, Ctrl elsewhere).
fn kw(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("ctrl-", "alt-")
    } else {
        keys.to_owned()
    }
}

/// Move-block keys (Alt+Shift+Up/Down; Cmd+Shift+Up/Down on macOS).
fn km(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("alt-shift-", "cmd-shift-")
    } else {
        keys.to_owned()
    }
}

/// Zoom keys (Alt+Right/Left; Cmd+. and Cmd+, on macOS where Alt moves by word).
fn kz(keys: &str) -> String {
    if cfg!(target_os = "macos") {
        keys.replace("alt-right", "cmd-.")
            .replace("alt-left", "cmd-,")
    } else {
        keys.to_owned()
    }
}

fn pair(depth: usize, text: &str) -> (usize, String) {
    (depth, text.to_owned())
}

mod ai_tests;
mod dnd_tests;
mod planning_tests;
mod slash_tests;

const HOME: &str = "pages/Home.md";

#[gpui_test]
fn pages_open_editable_and_the_rows_come_from_core(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n\t- child\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    assert_eq!(ed.read_with(cx, |e, _| e.rows().len()), 3);
    assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 3);
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "one"), pair(1, "two"), pair(2, "child")]
    );
    // Viewing writes nothing.
    assert_eq!(env.disk(HOME), "- one\n- two\n\t- child\n");
}

#[gpui_test]
fn typing_commits_after_the_debounce_and_an_untouched_buffer_makes_no_op(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    // Entering and leaving without a change writes nothing and adds no history entry.
    flush(&ed, cx);
    assert_eq!(env.disk(HOME), "- one\n- two\n");
    cx.simulate_input("X");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("oneX"));
    // Not committed yet.
    assert_eq!(env.snapshot_texts("Home")[0].1, "one");
    cx.executor().advance_clock(super::view::FLUSH_DELAY);
    // The commit is submitted to the writer thread without blocking the UI thread.
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if env.snapshot_texts("Home")[0].1 == "oneX" {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(env.snapshot_texts("Home")[0].1, "oneX");
    assert_eq!(env.disk(HOME), "- oneX\n- two\n");
}

#[gpui_test]
fn escape_flushes_and_selects_the_block(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    cx.simulate_keystrokes("escape");
    assert_eq!(env.snapshot_texts("Home")[0].1, "one!");
    assert_eq!(ed.read_with(cx, |e, _| e.editing()), None);
    let first = ids(&ed, cx)[0];
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks()), [first]);
    assert_eq!(
        ed.read_with(cx, |e, _| e.key_context_name()),
        "Outliner BlockSelection"
    );
}

#[gpui_test]
fn hidden_properties_are_not_shown_and_return_byte_exact(cx: &mut TestAppContext) {
    let page = "- title\n  id:: 6f2c1b7a-0000-4000-8000-000000000001\n  collapsed:: true\n\t- child\n- other\n";
    let env = Env::new(&[(HOME, page)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    assert_eq!(buffer(&ed, cx).as_deref(), Some("title"));
    cx.simulate_input("s");
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- titles\n  id:: 6f2c1b7a-0000-4000-8000-000000000001\n  collapsed:: true\n\t- child\n- other\n"
    );
    // A second line is added under the title, before the hidden lines.
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("more");
    flush(&ed, cx);
    assert!(
        env.disk(HOME).starts_with(
            "- titles\n  id:: 6f2c1b7a-0000-4000-8000-000000000001\n  collapsed:: true\n  more\n"
        ),
        "{}",
        env.disk(HOME)
    );
}

#[gpui_test]
fn clicking_text_enters_edit_mode_at_the_mapped_source_offset(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a **bold** z\n- other\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    // "a bo|ld z": display offset 4 maps to source offset 6.
    let src = view.read_with(cx, |v, _| v.rows()[0].block.title.source_offset(4));
    assert_eq!(src, Some(6));
    ed.update_in(cx, |e, window, cx| {
        e.click_row(0, src.unwrap_or(0), false, window, cx);
    });
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 6);
    // A click on the blank area of another row puts the caret at its end.
    ed.update_in(cx, |e, window, cx| {
        e.click_row(1, usize::MAX, false, window, cx);
    });
    assert_eq!(editing_row(&ed, cx), Some(1));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 5);
}

#[gpui_test]
fn a_real_mouse_click_on_a_row_starts_editing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- first block\n- second block\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let pos = view
        .read_with(cx, |v, _| {
            v.items()
                .iter()
                .position(|i| matches!(i, crate::views::page_view::Item::Block(1)))
        })
        .expect("row 1 item");
    let bounds = view
        .read_with(cx, |v, _| v.list_bounds_for_item(pos))
        .expect("row laid out");
    cx.simulate_click(
        crate::ui::point(bounds.right() - px(40.), bounds.top() + px(10.)),
        crate::ui::text_edit::Modifiers::default(),
    );
    assert_eq!(editing_row(&ed, cx), Some(1));
}

#[gpui_test]
fn enter_splits_at_the_caret_and_backspace_at_start_merges(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- abcd\n- tail\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("left left enter");
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "ab"), pair(1, "cd"), pair(1, "tail")]
    );
    assert_eq!(editing_row(&ed, cx), Some(1));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 0);
    cx.simulate_keystrokes("backspace");
    assert_eq!(env.snapshot_texts("Home")[0], pair(1, "abcd"));
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        2,
        "caret at the junction"
    );
    assert_eq!(env.disk(HOME), "- abcd\n- tail\n");
}

#[gpui_test]
fn enter_on_an_empty_last_child_outdents_and_shift_enter_adds_a_line(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- parent\n\t- \n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(env.snapshot_texts("Home"), [pair(1, "parent"), pair(1, "")]);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("next");
    flush(&ed, cx);
    assert_eq!(env.snapshot_texts("Home")[0], pair(1, "parent\nnext"));
    assert!(
        env.disk(HOME).starts_with("- parent\n  next\n"),
        "{}",
        env.disk(HOME)
    );
}

#[gpui_test]
fn enter_inside_a_page_ref_jumps_past_the_brackets_instead_of_splitting(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- see [[Alpha]] now\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::Visible(8), cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(env.snapshot_texts("Home").len(), 1, "no split inside [[ ]]");
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 13);
}

#[gpui_test]
fn delete_at_the_end_pulls_the_next_block_and_refusals_change_nothing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- ab\n- cd\n- ef\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("delete");
    assert_eq!(env.snapshot_texts("Home"), [pair(1, "abcd"), pair(1, "ef")]);
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        2,
        "caret at the junction"
    );
    // Backspace at the start of the first block of the page is refused: nothing changes.
    edit(&ed, 0, Caret::Start, cx);
    cx.simulate_keystrokes("backspace");
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(env.disk(HOME), "- abcd\n- ef\n");
    // Delete in the last block has no next block to pull in.
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("delete");
    assert_eq!(env.snapshot_texts("Home").len(), 2);
}

#[gpui_test]
fn tab_shift_tab_and_alt_shift_arrows_restructure_and_keep_the_caret(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n- b\n- c\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "a"), pair(2, "b"), pair(1, "c")]
    );
    assert_eq!(editing_row(&ed, cx), Some(1));
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(env.snapshot_texts("Home")[1], pair(1, "b"));
    cx.simulate_keystrokes(&km("alt-shift-up"));
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "b"), pair(1, "a"), pair(1, "c")]
    );
    assert_eq!(
        editing_row(&ed, cx),
        Some(0),
        "the caret follows the moved block"
    );
    cx.simulate_keystrokes(&km("alt-shift-down alt-shift-down"));
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "a"), pair(1, "c"), pair(1, "b")]
    );
    // The first block cannot be indented: nothing changes.
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(env.snapshot_texts("Home")[0], pair(1, "a"));
    assert_eq!(env.disk(HOME), "- a\n- c\n- b\n");
}

#[gpui_test]
fn collapse_and_expand_persist_collapsed_true_and_restore_the_bytes(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n\t- a1\n\t- a2\n- b\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-up"));
    assert_eq!(
        env.disk(HOME),
        "- a\n  collapsed:: true\n\t- a1\n\t- a2\n- b\n"
    );
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 2);
    assert_eq!(editing_row(&ed, cx), Some(0));
    cx.simulate_keystrokes(&k("ctrl-down"));
    assert_eq!(env.disk(HOME), "- a\n\t- a1\n\t- a2\n- b\n");
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 4);
    // Leaves are ignored.
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-up"));
    assert_eq!(env.disk(HOME), "- a\n\t- a1\n\t- a2\n- b\n");
}

#[gpui_test]
fn page_level_collapse_acts_when_nothing_is_edited_and_t_o_toggles_all(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n\t- a1\n- b\n\t- b1\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("escape escape");
    assert_eq!(ed.read_with(cx, |e, _| e.key_context_name()), "Outliner");
    cx.simulate_keystrokes("t o");
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 2);
    cx.simulate_keystrokes("t o");
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 4);
    assert_eq!(env.disk(HOME), "- a\n\t- a1\n- b\n\t- b1\n");
}

#[gpui_test]
fn ctrl_enter_cycles_the_task_marker_and_the_checkbox_toggles_done(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- write\n- other\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-enter"));
    assert_eq!(env.snapshot_texts("Home")[0].1, "LATER write");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("LATER write"));
    cx.simulate_keystrokes(&k("ctrl-enter ctrl-enter ctrl-enter"));
    assert_eq!(
        env.snapshot_texts("Home")[0].1,
        "write",
        "cycle ends without a marker"
    );
    ed.update_in(cx, |e, window, cx| e.toggle_done(0, window, cx));
    assert_eq!(env.snapshot_texts("Home")[0].1, "DONE write");
}

#[gpui_test]
fn ctrl_1_2_3_set_the_marker_of_the_editing_block_and_undo_restores_the_bytes(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&[(HOME, "- [#A] write\n  owner:: me\n- other\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-1"));
    assert_eq!(env.disk(HOME), "- TODO [#A] write\n  owner:: me\n- other\n");
    cx.simulate_keystrokes(&k("ctrl-2"));
    assert_eq!(
        env.disk(HOME),
        "- DOING [#A] write\n  owner:: me\n- other\n"
    );
    cx.simulate_keystrokes(&k("ctrl-3"));
    assert_eq!(env.disk(HOME), "- DONE [#A] write\n  owner:: me\n- other\n");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        env.disk(HOME),
        "- DOING [#A] write\n  owner:: me\n- other\n"
    );
    cx.simulate_keystrokes(&k("ctrl-z ctrl-z"));
    assert_eq!(env.disk(HOME), "- [#A] write\n  owner:: me\n- other\n");
}

#[gpui_test]
fn ctrl_1_2_3_apply_to_a_block_selection_as_one_undo_step(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n- DONE b\n- c\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("escape shift-down");
    cx.simulate_keystrokes(&k("ctrl-1"));
    assert_eq!(env.disk(HOME), "- TODO a\n- TODO b\n- c\n");
    cx.simulate_keystrokes(&k("ctrl-3"));
    assert_eq!(env.disk(HOME), "- DONE a\n- DONE b\n- c\n");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.disk(HOME), "- TODO a\n- TODO b\n- c\n");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.disk(HOME), "- a\n- DONE b\n- c\n");
}

#[gpui_test]
fn escape_selects_shift_arrows_extend_and_bulk_operations_use_one_transaction(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&[(HOME, "- a\n- b\n- c\n- d\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("escape shift-down");
    let all = ids(&ed, cx);
    assert_eq!(
        ed.read_with(cx, |e, _| e.selected_blocks()),
        [all[1], all[2]]
    );
    cx.simulate_keystrokes("shift-up shift-up");
    assert_eq!(
        ed.read_with(cx, |e, _| e.selected_blocks()),
        [all[0], all[1]]
    );
    // Indent both under nothing: the first block has no previous sibling -> refused.
    cx.simulate_keystrokes("tab");
    assert_eq!(
        env.snapshot_texts("Home")
            .iter()
            .map(|t| t.0)
            .collect::<Vec<_>>(),
        [1, 1, 1, 1]
    );
    // Select b and c and indent them under a.
    cx.simulate_keystrokes("down");
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks()), [all[1]]);
    cx.simulate_keystrokes("shift-down tab");
    assert_eq!(
        env.snapshot_texts("Home"),
        [pair(1, "a"), pair(2, "b"), pair(2, "c"), pair(1, "d")]
    );
    // Undo reverts both blocks in one step.
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        env.snapshot_texts("Home")
            .iter()
            .map(|t| t.0)
            .collect::<Vec<_>>(),
        [1, 1, 1, 1]
    );
    cx.simulate_keystrokes(&k("ctrl-shift-z"));
    assert_eq!(
        env.snapshot_texts("Home")
            .iter()
            .map(|t| t.0)
            .collect::<Vec<_>>(),
        [1, 2, 2, 1]
    );
}

#[gpui_test]
fn selection_delete_enter_and_select_all(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n- b\n- c\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("escape ctrl-shift-a"));
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks().len()), 3);
    cx.simulate_keystrokes("escape");
    assert!(!ed.read_with(cx, |e, _| e.has_selection()));
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes("escape backspace");
    assert_eq!(env.snapshot_texts("Home"), [pair(1, "a"), pair(1, "c")]);
    // Enter edits a single selected block.
    let first = ids(&ed, cx)[0];
    ed.update_in(cx, |e, window, cx| e.select_only(first, window, cx));
    cx.simulate_keystrokes("enter");
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(env.disk(HOME), "- a\n- c\n");
}

#[gpui_test]
fn ctrl_a_in_selection_mode_selects_the_parent(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- p\n\t- child\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let child = ids(&ed, cx)[1];
    ed.update_in(cx, |e, window, cx| e.select_only(child, window, cx));
    cx.simulate_keystrokes(&k("ctrl-a"));
    let parent = ids(&ed, cx)[0];
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks()), [parent]);
}

#[gpui_test]
fn copy_cut_and_paste_of_block_subtrees(cx: &mut TestAppContext) {
    let env = Env::new(&[(
        HOME,
        "- a\n  id:: 6f2c1b7a-0000-4000-8000-000000000001\n\t- a1\n- b\n",
    )]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("escape ctrl-c"));
    // Plain text: tab-indented Markdown without id::.
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some("- a\n\t- a1\n".to_owned())
    );
    // Paste after block b: a copy gets fresh identities (no id:: line).
    let b = ids(&ed, cx)[2];
    ed.update_in(cx, |e, window, cx| e.select_only(b, window, cx));
    cx.simulate_keystrokes(&k("ctrl-v"));
    let texts = env.snapshot_texts("Home");
    assert_eq!(texts.len(), 5, "{texts:?}");
    assert_eq!(texts[3], pair(1, "a"));
    assert_eq!(texts[4], pair(2, "a1"));
    // Cut removes the subtree and the paste restores it with its id::.
    let a = ids(&ed, cx)[0];
    ed.update_in(cx, |e, window, cx| e.select_only(a, window, cx));
    cx.simulate_keystrokes(&k("ctrl-x"));
    assert_eq!(env.snapshot_texts("Home").len(), 3);
    let last = *ids(&ed, cx).last().expect("last");
    ed.update_in(cx, |e, window, cx| e.select_only(last, window, cx));
    cx.simulate_keystrokes(&k("ctrl-v"));
    assert!(
        env.disk(HOME)
            .contains("id:: 6f2c1b7a-0000-4000-8000-000000000001"),
        "{}",
        env.disk(HOME)
    );
}

#[gpui_test]
fn pasting_a_markdown_list_while_editing_creates_blocks_and_plain_text_goes_inline(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&[(HOME, "- \n- tail\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.write_to_clipboard(crate::ui::text_edit::ClipboardItem::new_string(
        "- one\n  - two\n- three".to_owned(),
    ));
    cx.simulate_keystrokes(&k("ctrl-v"));
    assert_eq!(
        env.snapshot_texts("Home"),
        [
            pair(1, "one"),
            pair(2, "two"),
            pair(1, "three"),
            pair(1, "tail")
        ],
        "pasting onto an empty block replaces it"
    );
    // Inline paste stays in the buffer.
    edit(&ed, 3, Caret::End, cx);
    cx.write_to_clipboard(crate::ui::text_edit::ClipboardItem::new_string(
        "+x".to_owned(),
    ));
    cx.simulate_keystrokes(&k("ctrl-v"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("tail+x"));
    // HTML is converted to Markdown, a list becomes blocks.
    cx.write_to_clipboard(crate::ui::text_edit::ClipboardItem::new_string(
        "<ul><li>h1</li><li>h2</li></ul>".to_owned(),
    ));
    edit(&ed, 3, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-v"));
    let texts = env.snapshot_texts("Home");
    assert!(
        texts.iter().any(|t| t.1 == "h1") && texts.iter().any(|t| t.1 == "h2"),
        "{texts:?}"
    );
}

#[gpui_test]
fn undo_and_redo_restore_the_text_the_caret_and_the_bytes(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- hello\n- other\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::Visible(2), cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(env.snapshot_texts("Home").len(), 3);
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.snapshot_texts("Home").len(), 2);
    assert_eq!(
        env.disk(HOME),
        "- hello\n- other\n",
        "bytes are exact after undo"
    );
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        2,
        "caret restored"
    );
    cx.simulate_keystrokes(&k("ctrl-shift-z"));
    assert_eq!(env.snapshot_texts("Home").len(), 3);
    assert_eq!(
        editing_row(&ed, cx),
        Some(1),
        "redo puts the caret in the new block"
    );
    // Typing is undone in word-sized steps and the buffer follows.
    cx.simulate_input("abc");
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("llo"),
        "typing undone, caret restored"
    );
    cx.simulate_keystrokes(&k("ctrl-y"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("abcllo"));
}

#[gpui_test]
fn zoom_in_re_roots_the_view_with_a_breadcrumb_and_changes_no_file(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- p\n\t- c1\n\t\t- g\n- q\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-."));
    assert_eq!(
        view.read_with(cx, |v, _| v.rows().len()),
        2,
        "c1 and its child"
    );
    assert_eq!(view.read_with(cx, |v, _| v.rows()[0].depth), 0);
    let crumbs: Vec<String> =
        ed.read_with(cx, |e, _| e.crumbs().into_iter().map(|c| c.0).collect());
    assert_eq!(crumbs, ["Home", "p", "c1"]);
    cx.simulate_keystrokes(&k("ctrl-,"));
    let parent = ids(&ed, cx)[0];
    assert_eq!(ed.read_with(cx, |e, _| e.zoom_root()), Some(parent));
    assert_eq!(
        view.read_with(cx, |v, _| v.rows().len()),
        3,
        "zoomed out to the parent"
    );
    cx.simulate_keystrokes(&k("ctrl-,"));
    assert_eq!(ed.read_with(cx, |e, _| e.zoom_root()), None);
    assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 4);
    assert_eq!(env.disk(HOME), "- p\n\t- c1\n\t\t- g\n- q\n");
}

#[gpui_test]
fn arrow_keys_cross_block_boundaries_keeping_the_goal_x(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- abcdef\n- xy\n- 0123456789\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::Start, cx);
    cx.simulate_keystrokes("right right right right down");
    assert_eq!(editing_row(&ed, cx), Some(1));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        2,
        "clamped to the short block"
    );
    cx.simulate_keystrokes("down");
    assert_eq!(editing_row(&ed, cx), Some(2));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        4,
        "goal x remembered"
    );
    cx.simulate_keystrokes("up up");
    assert_eq!(editing_row(&ed, cx), Some(0));
    // Left at 0 and right at the end move to the neighbours.
    cx.simulate_keystrokes("home left");
    assert_eq!(editing_row(&ed, cx), Some(0), "no block above");
    cx.simulate_keystrokes("end right");
    assert_eq!(editing_row(&ed, cx), Some(1));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 0);
    cx.simulate_keystrokes("left");
    assert_eq!(editing_row(&ed, cx), Some(0));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 6);
}

#[gpui_test]
fn autopair_inserts_skips_and_deletes_pairs(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- x\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("[");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("x[]"));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 2);
    cx.simulate_input("a]");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("x[a]"),
        "closer skipped over"
    );
    cx.simulate_keystrokes("left backspace");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("x[]"),
        "only the letter went"
    );
    cx.simulate_keystrokes("backspace");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("x"), "pair deleted");
}

#[gpui_test]
fn ime_composition_keeps_enter_and_tab_away_from_the_outliner(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n- b\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "kan", Some(3..3), window, cx);
    });
    assert_eq!(buffer(&ed, cx).as_deref(), Some("bkan"));
    assert_eq!(ed.read_with(cx, |e, _| e.marked_range()), Some(1..4));
    // The debounce does not commit while the composition is active.
    cx.executor().advance_clock(super::view::FLUSH_DELAY);
    cx.run_until_parked();
    assert_eq!(env.snapshot_texts("Home")[1].1, "b");
    cx.simulate_keystrokes("tab");
    assert_eq!(
        env.snapshot_texts("Home")[1].0,
        1,
        "Tab did not indent during composition"
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        ed.read_with(cx, |e, _| e.marked_range()),
        None,
        "Enter committed the text"
    );
    assert_eq!(env.snapshot_texts("Home").len(), 2, "and did not split");
    // The composed text commits through the input handler.
    ed.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(None, "\u{6f22}", None, window, cx);
        e.replace_text_in_range(None, "\u{6f22}", window, cx);
    });
    assert_eq!(buffer(&ed, cx).as_deref(), Some("bkan\u{6f22}"));
    flush(&ed, cx);
    assert_eq!(env.disk(HOME), "- a\n- bkan\u{6f22}\n");
}

const AC_PAGES: [(&str, &str); 4] = [
    (HOME, "- one\n"),
    ("pages/Alpha.md", "- alpha page\n"),
    ("pages/Project Plan.md", "- plan\n"),
    ("pages/Facts.md", "- the important fact\n- other fact\n"),
];

#[gpui_test]
fn autocomplete_context_wins_over_the_block_editor_context(cx: &mut TestAppContext) {
    let env = Env::new(&AC_PAGES);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    assert_eq!(
        ed.read_with(cx, |e, _| e.key_context_name()),
        "Outliner BlockEditor"
    );
    cx.simulate_input("[[Alp");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one[[Alp]]"));
    assert!(ed.read_with(cx, |e, _| e.completion_open()));
    cx.run_until_parked();
    assert_eq!(
        ed.read_with(cx, |e, _| e.key_context_name()),
        "Outliner BlockEditor Autocomplete"
    );
    // Enter goes to the popup (inserts the page), not to the block.
    cx.simulate_keystrokes("enter");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one[[Alpha]]"));
    assert_eq!(
        env.snapshot_texts("Home").len(),
        1,
        "Enter did not split the block"
    );
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    assert_eq!(
        ed.read_with(cx, |e, _| e.cursor_offset()),
        12,
        "caret after the closing brackets"
    );
    // Escape closes only the popup while it is open.
    cx.simulate_input(" #Pro");
    assert!(ed.read_with(cx, |e, _| e.completion_open()));
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    assert_eq!(editing_row(&ed, cx), Some(0), "still editing");
    assert!(!ed.read_with(cx, |e, _| e.completion_open()));
    // Without the popup the BlockEditor binding applies again.
    cx.simulate_keystrokes("enter");
    assert_eq!(env.snapshot_texts("Home").len(), 2);
    // And the outliner-level binding applies when no block is edited or selected.
    cx.simulate_keystrokes("escape escape");
    assert_eq!(ed.read_with(cx, |e, _| e.key_context_name()), "Outliner");
}

#[gpui_test]
fn page_and_tag_completion_insert_the_right_text_and_navigate_with_the_keyboard(
    cx: &mut TestAppContext,
) {
    let env = Env::new(&AC_PAGES);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    // Tab accepts; a multi-word page after '#' is wrapped in [[ ]].
    cx.simulate_input(" #Proj");
    cx.run_until_parked();
    let first = ed.read_with(cx, |e, _| e.completion().map(|c| c.items.len()));
    assert!(first.is_some_and(|n| n >= 1), "candidates for #Proj");
    cx.simulate_keystrokes("tab");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one #[[Project Plan]]"));
    // A query that matches nothing offers a "New page" entry; Down/Up move, Ctrl+N/P too.
    cx.simulate_input(" [[Brand New");
    cx.run_until_parked();
    let labels: Vec<String> = ed.read_with(cx, |e, _| {
        e.completion()
            .map(|c| c.items.iter().map(super::completion::Item::label).collect())
            .unwrap_or_default()
    });
    assert_eq!(
        labels.last().map(String::as_str),
        Some("New page: Brand New")
    );
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("ctrl-n ctrl-p up");
    assert_eq!(
        ed.read_with(cx, |e, _| e.completion().map(|c| c.selected)),
        Some(0)
    );
    cx.simulate_keystrokes("up");
    let last = labels.len() - 1;
    assert_eq!(
        ed.read_with(cx, |e, _| e.completion().map(|c| c.selected)),
        Some(last)
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("one #[[Project Plan]] [[Brand New]]")
    );
    flush(&ed, cx);
    // Choosing "New page" writes no page file: the page exists only as text until it is edited.
    assert!(!env.graph.path().join("pages/Brand New.md").exists());
    assert_eq!(env.disk(HOME), "- one #[[Project Plan]] [[Brand New]]\n");
}

#[gpui_test]
fn block_reference_completion_writes_the_id_in_the_same_undo_step(cx: &mut TestAppContext) {
    let env = Env::new(&AC_PAGES);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input(" ((important");
    cx.run_until_parked();
    assert!(ed.read_with(cx, |e, _| e.completion_open()));
    cx.simulate_keystrokes("enter");
    let text = env.snapshot_texts("Home")[0].1.clone();
    assert!(text.starts_with("one (("), "{text}");
    let uuid = text
        .trim_start_matches("one ((")
        .trim_end_matches("))")
        .to_owned();
    assert_eq!(uuid.len(), 36, "{text}");
    // The referenced block got its id:: on disk.
    assert!(
        env.disk("pages/Facts.md")
            .contains(&format!("- the important fact\n  id:: {uuid}\n")),
        "{}",
        env.disk("pages/Facts.md")
    );
    assert_eq!(env.disk("pages/Facts.md").matches("id::").count(), 1);
    // One undo removes the reference and the id:: together.
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.snapshot_texts("Home")[0].1, "one ((important))");
    assert_eq!(
        env.disk("pages/Facts.md"),
        "- the important fact\n- other fact\n"
    );
}

#[gpui_test]
fn copy_block_ref_and_embed_persist_an_id_only_for_referenced_blocks(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    // No id:: until a reference is copied.
    assert_eq!(env.disk(HOME), "- one\n- two\n");
    cx.simulate_keystrokes(&k("ctrl-c"));
    let copied = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .expect("clipboard");
    assert!(
        copied.starts_with("((") && copied.ends_with("))"),
        "{copied}"
    );
    let uuid = copied.trim_start_matches("((").trim_end_matches("))");
    assert!(
        env.disk(HOME).contains(&format!("- two\n  id:: {uuid}\n")),
        "{}",
        env.disk(HOME)
    );
    cx.simulate_keystrokes(&k("ctrl-e"));
    let embed = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .expect("clipboard");
    assert_eq!(embed, format!("{{{{embed (({uuid}))}}}}"));
    // Copying with a text selection copies the text, not a reference.
    cx.simulate_keystrokes(&k("ctrl-a ctrl-c"));
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some("two".to_owned())
    );
}

#[gpui_test]
fn very_long_blocks_are_selected_instead_of_edited(cx: &mut TestAppContext) {
    let long = "x".repeat(super::view::MAX_EDIT_LEN + 1);
    let env = Env::new(&[(HOME, &format!("- {long}\n- short\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    assert_eq!(editing_row(&ed, cx), None);
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks().len()), 1);
}

#[gpui_test]
fn the_edited_block_changing_on_disk_offers_keep_mine_or_take_disk(cx: &mut TestAppContext) {
    use bitacora_core::queue::Request;
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    let id = ids(&ed, cx)[0];
    let external = |text: &str| {
        std::fs::write(env.graph.path().join(HOME), format!("- {text}\n- two\n")).expect("write");
        env.queue()
            .execute(
                Source::External,
                Request::ExternalChange {
                    key: PageKey::from_title("Home"),
                    bytes: format!("- {text}\n- two\n").into_bytes(),
                },
            )
            .expect("external change");
    };
    // Another program changed this block: core now holds the disk text.
    external("one (disk)");
    ed.update(cx, |e, cx| {
        e.on_editing_conflict(id, "one".into(), "one (disk)".into(), cx);
    });
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("one!"),
        "the buffer is untouched"
    );
    // Take disk replaces the buffer.
    ed.update_in(cx, |e, window, cx| e.resolve_conflict(false, window, cx));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("one (disk)"));
    // Keep mine: the buffer is written over the disk text on the next flush.
    cx.simulate_input("?");
    external("one (disk)x");
    ed.update(cx, |e, cx| {
        e.on_editing_conflict(id, "x".into(), "one (disk)x".into(), cx);
    });
    ed.update_in(cx, |e, window, cx| e.resolve_conflict(true, window, cx));
    flush(&ed, cx);
    assert_eq!(env.snapshot_texts("Home")[0].1, "one (disk)?");
    assert_eq!(env.disk(HOME), "- one (disk)?\n- two\n");
}

#[gpui_test]
fn page_load_never_rewrites_untouched_blocks(cx: &mut TestAppContext) {
    // Odd indentation, CRLF-free trailing spaces and properties survive an edit of one block.
    let page = "- keep   \n  Keep:: Me\n\t- sub  \n- edit me\n- last\n";
    let env = Env::new(&[(HOME, page)]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 2, Caret::End, cx);
    cx.simulate_input("!");
    flush(&ed, cx);
    assert_eq!(
        env.disk(HOME),
        "- keep   \n  Keep:: Me\n\t- sub  \n- edit me!\n- last\n"
    );
}

#[gpui_test]
fn row_callbacks_zoom_fold_and_toggle_done(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- a\n\t- a1\n- b\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let hooks = |cx: &mut VisualTestContext, row: usize| {
        cx.update(|_, cx| OutlineEditor::row_edit(&ed, row, cx))
            .expect("hooks")
    };
    // Bullet: zoom into the block.
    let h = hooks(cx, 2);
    cx.update(|window, cx| (h.on_bullet)(window, cx));
    let b = ids(&ed, cx)[0];
    assert_eq!(ed.read_with(cx, |e, _| e.zoom_root()), Some(b));
    assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 1);
    ed.update(cx, |e, cx| e.zoom_to(None, cx));
    // Arrow: fold the block (persisted).
    let h = hooks(cx, 0);
    cx.update(|window, cx| (h.on_toggle)(window, cx));
    assert_eq!(env.disk(HOME), "- a\n  collapsed:: true\n\t- a1\n- b\n");
    // Checkbox: toggle done.
    let h = hooks(cx, 2);
    cx.update(|window, cx| (h.on_checkbox)(window, cx));
    assert_eq!(env.snapshot_texts("Home")[2].1, "DONE b");
    // Text click: edit at the offset.
    let h = hooks(cx, 0);
    cx.update(|window, cx| (h.on_text)(1, false, window, cx));
    assert_eq!(editing_row(&ed, cx), Some(0));
}

#[gpui_test]
fn journal_days_are_editable_in_the_feed(cx: &mut TestAppContext) {
    use crate::views::journals::JournalsView;
    use bitacora_core::date::Date;
    let env = Env::new(&[
        ("journals/2025_03_09.md", "- standup notes\n"),
        ("journals/2025_03_08.md", "- yesterday\n"),
    ]);
    setup(cx);
    let day = Date::new(2025, 3, 9).expect("date");
    let (feed, cx) =
        cx.add_window_view(|_, _| JournalsView::with_clock(std::rc::Rc::new(move || Some(day))));
    cx.simulate_resize(size(px(840.), px(700.)));
    let link = env.link.clone();
    feed.update(cx, |v, cx| v.set_session_link(Some(link), cx));
    let handle = env.handle.clone();
    feed.update(cx, |v, cx| v.show(handle, None, cx));
    cx.executor().allow_parking();
    let today = day.journal_day();
    for _ in 0..400 {
        cx.run_until_parked();
        if feed.read_with(cx, |v, _| v.editor_for(today).is_some()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let ed = feed
        .read_with(cx, |v, _| v.editor_for(today).cloned())
        .expect("today is editable");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("!");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("second");
    flush(&ed, cx);
    assert_eq!(
        env.disk("journals/2025_03_09.md"),
        "- standup notes!\n- second\n"
    );
    // The other day is untouched.
    assert_eq!(env.disk("journals/2025_03_08.md"), "- yesterday\n");
}

#[gpui_test]
fn every_default_binding_is_valid_and_user_overrides_are_validated(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        let report = crate::keymap::load_checked(cx, crate::keymap::DEFAULT_KEYMAP).expect("parse");
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(report.bound > 80, "bound {}", report.bound);
    });
    // The editor sections cover the four contexts.
    let sections = crate::keymap::parse(crate::keymap::DEFAULT_KEYMAP).expect("parse");
    for ctx in ["Outliner", "BlockSelection", "BlockEditor", "Autocomplete"] {
        assert!(
            sections
                .iter()
                .any(|s| s.context.as_deref().is_some_and(|c| c.starts_with(ctx))),
            "no section for {ctx}"
        );
    }
    // A user keymap rebinds Mod+Enter and its bad entries are reported and ignored.
    let env = Env::new(&[(HOME, "- a\n")]);
    let user = r#"[{"context": "BlockEditor", "bindings": {
        "ctrl-enter": "outliner::InsertNewline",
        "ctrl-j": "outliner::NoSuchAction"}},
        {"context": "((", "bindings": {"ctrl-k": "outliner::Undo"}}]"#;
    let report = cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        crate::keymap::load_with_user_report(cx, Some(user)).expect("report")
    });
    assert_eq!(report.problems.len(), 2, "{:?}", report.problems);
    let (_view, ed, cx) = open_page_without_setup(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes("ctrl-enter");
    assert_eq!(
        buffer(&ed, cx).as_deref(),
        Some("a\n"),
        "the user binding applies"
    );
    // Malformed JSON is an error that the app reports and ignores.
    cx.update(|_, cx| {
        let report = crate::keymap::load_with_user_report(cx, Some("{")).expect("report");
        assert_eq!(report.problems.len(), 1);
    });
}

#[gpui_test]
fn text_keys_inside_the_edited_block(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- alpha beta gamma\n- next\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 0, Caret::End, cx);
    // Word motion, word selection and word deletion.
    cx.simulate_keystrokes(&kw("ctrl-left"));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 11);
    cx.simulate_keystrokes(&kw("ctrl-shift-left"));
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 6..11);
    cx.simulate_keystrokes(&kw("ctrl-right"));
    assert_eq!(ed.read_with(cx, |e, _| e.cursor_offset()), 10);
    cx.simulate_keystrokes(&kw("ctrl-backspace"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("alpha  gamma"));
    cx.simulate_keystrokes(&kw("home ctrl-delete"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("  gamma"));
    // Select all text, copy, cut, paste and raw paste.
    cx.simulate_keystrokes(&k("ctrl-a ctrl-c"));
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()),
        Some("  gamma".to_owned())
    );
    cx.simulate_keystrokes(&k("ctrl-x"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some(""));
    cx.simulate_keystrokes(&k("ctrl-v ctrl-shift-v"));
    assert_eq!(buffer(&ed, cx).as_deref(), Some("  gamma  gamma"));
    // Shift+Home/End and Shift+Up/Down select inside and across the block edge.
    cx.simulate_keystrokes("home shift-end");
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 0..14);
    cx.simulate_keystrokes("right shift-left shift-left");
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 12..14);
    cx.simulate_keystrokes("shift-up");
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 0..14);
    cx.simulate_keystrokes("end shift-down");
    assert_eq!(ed.read_with(cx, |e, _| e.selection_range()), 14..14);
}

#[gpui_test]
fn alt_arrows_zoom_and_ctrl_semicolon_toggles_all(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- p\n\t- c\n\t\t- g\n- q\n\t- q1\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_keystrokes(&kz("alt-right"));
    assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 2);
    cx.simulate_keystrokes(&kz("alt-left"));
    assert_eq!(
        view.read_with(cx, |v, _| v.rows().len()),
        3,
        "zoomed out to the parent"
    );
    cx.simulate_keystrokes(&kz("alt-left"));
    assert_eq!(view.read_with(cx, |v, _| v.rows().len()), 5);
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&k("ctrl-;"));
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 2);
    cx.simulate_keystrokes(&k("ctrl-;"));
    assert_eq!(view.read_with(cx, |v, _| v.visible_count()), 5);
    assert_eq!(env.disk(HOME), "- p\n\t- c\n\t\t- g\n- q\n\t- q1\n");
}

#[gpui_test]
fn a_placeholder_page_is_editable_and_its_file_appears_with_content(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- see [[Ghost]]\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Ghost");
    assert_eq!(ed.read_with(cx, |e, _| e.rows().len()), 1);
    let file = env.graph.path().join("pages/Ghost.md");
    flush(&ed, cx);
    let _ = env.queue().flush(Source::Ui);
    assert!(!file.exists(), "no file before there is content");
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_input("hello");
    flush(&ed, cx);
    assert_eq!(env.disk("pages/Ghost.md"), "- hello\n");
}

fn row_bounds(
    view: &Entity<PageView>,
    row: usize,
    cx: &mut VisualTestContext,
) -> crate::ui::Bounds<crate::ui::Pixels> {
    let pos = view
        .read_with(cx, |v, _| {
            v.items()
                .iter()
                .position(|i| matches!(i, crate::views::page_view::Item::Block(r) if *r == row))
        })
        .expect("row item");
    view.read_with(cx, |v, _| v.list_bounds_for_item(pos))
        .expect("row laid out")
}

#[gpui_test]
fn dragging_the_mouse_across_blocks_selects_them(cx: &mut TestAppContext) {
    use crate::ui::text_edit::{Modifiers, MouseButton};
    let env = Env::new(&[(HOME, "- one\n- two\n- three\n- four\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let first = row_bounds(&view, 0, cx);
    let third = row_bounds(&view, 2, cx);
    let start = crate::ui::point(first.right() - px(40.), first.top() + px(10.));
    let end = crate::ui::point(third.right() - px(40.), third.top() + px(10.));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    assert_eq!(editing_row(&ed, cx), Some(0));
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    let ids = ids(&ed, cx);
    assert_eq!(editing_row(&ed, cx), None, "the drag left edit mode");
    assert_eq!(
        ed.read_with(cx, |e, _| e.selected_blocks()),
        ids[0..3].to_vec()
    );
    // Dragging does not touch the page.
    assert_eq!(env.disk(HOME), "- one\n- two\n- three\n- four\n");
    // A plain click afterwards edits again and a stray move does not select.
    cx.simulate_mouse_move(start, None, Modifiers::default());
    assert_eq!(ed.read_with(cx, |e, _| e.selected_blocks().len()), 3);
}

#[gpui_test]
fn up_and_down_continue_across_journal_days(cx: &mut TestAppContext) {
    use crate::views::journals::JournalsView;
    use bitacora_core::date::Date;
    let env = Env::new(&[
        ("journals/2025_03_09.md", "- standup notes\n"),
        ("journals/2025_03_08.md", "- yesterday\n"),
    ]);
    setup(cx);
    let day = Date::new(2025, 3, 9).expect("date");
    let (feed, cx) =
        cx.add_window_view(|_, _| JournalsView::with_clock(std::rc::Rc::new(move || Some(day))));
    cx.simulate_resize(size(px(840.), px(700.)));
    let link = env.link.clone();
    feed.update(cx, |v, cx| v.set_session_link(Some(link), cx));
    let handle = env.handle.clone();
    feed.update(cx, |v, cx| v.show(handle, None, cx));
    cx.executor().allow_parking();
    let (d9, d8) = (day.journal_day(), 20_250_308);
    for _ in 0..400 {
        cx.run_until_parked();
        if feed.read_with(cx, |v, _| {
            v.editor_for(d9).is_some() && v.editor_for(d8).is_some()
        }) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let top = feed
        .read_with(cx, |v, _| v.editor_for(d9).cloned())
        .expect("today is editable");
    let older = feed
        .read_with(cx, |v, _| v.editor_for(d8).cloned())
        .expect("older day is editable");
    edit(&top, 0, Caret::End, cx);
    cx.simulate_keystrokes("down");
    assert_eq!(editing_row(&top, cx), None, "left the first day");
    assert_eq!(
        editing_row(&older, cx),
        Some(0),
        "continued in the older day"
    );
    cx.simulate_keystrokes("up");
    assert_eq!(editing_row(&older, cx), None);
    assert_eq!(editing_row(&top, cx), Some(0), "and back");
    // Nothing was written by moving around.
    assert_eq!(env.disk("journals/2025_03_09.md"), "- standup notes\n");
    assert_eq!(env.disk("journals/2025_03_08.md"), "- yesterday\n");
}

#[gpui_test]
fn the_completion_popup_floats_instead_of_pushing_the_rows(cx: &mut TestAppContext) {
    let env = Env::new(&[
        (HOME, "- one\n- below\n"),
        ("pages/Alpha.md", "- alpha page\n"),
    ]);
    let (view, ed, cx) = open_page(cx, &env, "Home");
    let before = row_bounds(&view, 1, cx);
    edit(&ed, 0, Caret::End, cx);
    cx.run_until_parked();
    let editing = row_bounds(&view, 1, cx);
    cx.simulate_input("[[Alp");
    cx.run_until_parked();
    assert!(ed.read_with(cx, |e, _| e.completion_open()));
    let with_popup = row_bounds(&view, 1, cx);
    assert_eq!(
        with_popup.top(),
        editing.top(),
        "the next row did not move down (it was at {:?} before editing)",
        before.top()
    );
}

fn wait_for(cx: &mut VisualTestContext, done: impl Fn() -> bool) {
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition not reached");
}

fn assets_in(env: &Env) -> Vec<String> {
    let _ = env.queue().flush(Source::Ui);
    std::fs::read_dir(env.graph.path().join("assets"))
        .map(|d| {
            d.map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[gpui_test]
fn dropping_files_on_a_block_saves_assets_and_links_them(cx: &mut TestAppContext) {
    let env = Env::new(&[("pages/sub/Home.md", "- one\n- two\n")]);
    let src = tempfile::tempdir().expect("src");
    let doc = src.path().join("report 50%.docx");
    std::fs::write(&doc, [9u8, 9]).expect("doc");
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    ed.update_in(cx, |e, window, cx| {
        e.drop_files(1, std::slice::from_ref(&doc), window, cx)
    });
    wait_for(cx, || env.snapshot_texts("Home")[1].1.contains("assets/"));
    let text = env.snapshot_texts("Home")[1].1.clone();
    let name = assets_in(&env).pop().expect("asset written");
    assert!(
        name.starts_with("report_50_") && name.ends_with("_0.docx"),
        "{name}"
    );
    assert_eq!(text, format!("two[report 50%.docx](../../assets/{name})"));
    assert_eq!(
        std::fs::read(env.graph.path().join("assets").join(&name)).expect("asset"),
        [9, 9]
    );
    assert_eq!(
        env.disk("pages/sub/Home.md"),
        format!("- one\n- two[report 50%.docx](../../assets/{name})\n")
    );
    // One undo takes the link out and recycles the file.
    cx.simulate_keystrokes(&k("ctrl-z"));
    assert_eq!(env.snapshot_texts("Home")[1].1, "two");
    let _ = env.disk("pages/sub/Home.md");
    assert!(assets_in(&env).is_empty());
}

#[gpui_test]
fn pasting_a_clipboard_image_attaches_it(cx: &mut TestAppContext) {
    let env = Env::new(&[("journals/2025_11_14.md", "- note\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Nov 14th, 2025");
    edit(&ed, 0, Caret::End, cx);
    let image = crate::ui::text_edit::Image::from_bytes(
        crate::ui::text_edit::ImageFormat::Png,
        vec![137, 80, 78, 71],
    );
    cx.write_to_clipboard(crate::ui::text_edit::ClipboardItem::new_image(&image));
    cx.simulate_keystrokes(&k("ctrl-v"));
    wait_for(cx, || {
        env.snapshot_texts("Nov 14th, 2025")[0]
            .1
            .contains("assets/")
    });
    let name = assets_in(&env).pop().expect("asset");
    assert!(
        name.starts_with("image_") && name.ends_with("_0.png"),
        "{name}"
    );
    assert_eq!(
        env.snapshot_texts("Nov 14th, 2025")[0].1,
        format!("note![image.png](../assets/{name})")
    );
    assert_eq!(
        editing_row(&ed, cx),
        Some(0),
        "the caret stays in the block"
    );
}

#[gpui_test]
fn delete_asset_asks_the_host_and_keeps_the_text(cx: &mut TestAppContext) {
    let env = Env::new(&[
        (HOME, "- pic ![a](../assets/a.png)\n- other\n"),
        ("assets/a.png", "x"),
    ]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&ed, move |_, event: &super::EditorEvent, _| {
            if let super::EditorEvent::DeleteAsset { link, .. } = event {
                sink.borrow_mut().push(link.clone());
            }
        })
    });
    assert!(ed.read_with(cx, |e, _| e.row_has_asset(0)));
    assert!(!ed.read_with(cx, |e, _| e.row_has_asset(1)));
    edit(&ed, 0, Caret::End, cx);
    cx.simulate_keystrokes(&format!(
        "{}-alt-backspace",
        if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        }
    ));
    assert_eq!(*seen.borrow(), ["assets/a.png"]);
    assert_eq!(env.snapshot_texts("Home")[0].1, "pic ![a](../assets/a.png)");
}

#[gpui_test]
fn the_edited_block_is_reported_busy_to_the_mcp_gate(cx: &mut TestAppContext) {
    use bitacora_mcp::WriteGate as _;
    let uuid = "6f2c1b7a-0000-4000-8000-000000000001";
    let env = Env::new(&[(HOME, &format!("- one\n  id:: {uuid}\n- two\n"))]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    assert_eq!(env.link.gate.busy("Home", uuid), None);
    edit(&ed, 0, Caret::End, cx);
    assert!(
        env.link.gate.busy("Home", uuid).is_some(),
        "busy while editing"
    );
    edit(&ed, 1, Caret::End, cx);
    assert_eq!(
        env.link.gate.busy("Home", uuid),
        None,
        "free once the caret moved"
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(env.link.gate.busy("Home", uuid), None);
}

#[gpui_test]
fn a_block_without_id_is_busy_under_its_index_uuid(cx: &mut TestAppContext) {
    use bitacora_mcp::WriteGate as _;
    let env = Env::new(&[(HOME, "- one\n- two\n")]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    let row = env
        .handle
        .reader
        .page_by_name("Home")
        .expect("page")
        .map(|p| {
            env.handle
                .reader
                .outline(p.id, 0, 10, false)
                .expect("outline")
        })
        .expect("rows");
    let uuid = row[1].uuid.clone();
    edit(&ed, 1, Caret::End, cx);
    wait_for(cx, || env.link.gate.busy("Home", &uuid).is_some());
}

#[gpui_test]
fn the_page_title_turns_into_an_input_and_submits_a_rename(cx: &mut TestAppContext) {
    use crate::views::page_view::PageEvent;
    let env = Env::new(&[(HOME, "- one\n")]);
    let (view, _ed, cx) = open_page(cx, &env, "Home");
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &PageEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
    });
    assert!(view.read_with(cx, |v, _| v.can_rename()));
    // Renaming while a block is being edited leaves edit mode: Enter must not split the block.
    edit(&_ed, 0, Caret::End, cx);
    view.update_in(cx, |v, window, cx| v.start_rename(window, cx));
    assert_eq!(editing_row(&_ed, cx), None);
    assert!(view.read_with(cx, |v, _| v.is_renaming()));
    // Escape leaves everything as it was.
    cx.simulate_keystrokes("escape");
    assert!(!view.read_with(cx, |v, _| v.is_renaming()));
    assert!(seen.borrow().is_empty());
    // Typing a new title and pressing Enter asks the host to rename.
    view.update_in(cx, |v, window, cx| {
        v.start_rename(window, cx);
        v.set_rename_text("  Start  ", window, cx);
    });
    cx.simulate_keystrokes("enter");
    assert_eq!(
        *seen.borrow(),
        [PageEvent::RenamePage {
            from: "Home".into(),
            to: "Start".into()
        }]
    );
    assert!(!view.read_with(cx, |v, _| v.is_renaming()));
    // The same title is not a rename.
    seen.borrow_mut().clear();
    view.update_in(cx, |v, window, cx| {
        v.start_rename(window, cx);
        v.set_rename_text("Home", window, cx);
    });
    cx.simulate_keystrokes("enter");
    assert!(seen.borrow().is_empty());
    // Nothing was written: renaming is the host's job, and no block was split.
    assert_eq!(env.disk(HOME), "- one\n");
}

#[gpui_test]
fn journals_cannot_be_renamed_in_the_header(cx: &mut TestAppContext) {
    let env = Env::new(&[("journals/2025_03_09.md", "- standup\n")]);
    let (view, _ed, cx) = open_page(cx, &env, "Mar 9th, 2025");
    assert!(!view.read_with(cx, |v, _| v.can_rename()));
}

#[gpui_test]
fn a_real_click_on_an_empty_block_starts_editing(cx: &mut TestAppContext) {
    let env = Env::new(&[(HOME, "- see [[Ghost]]\n")]);
    let (view, ed, cx) = open_page(cx, &env, "Ghost");
    let bounds = row_bounds(&view, 0, cx);
    cx.simulate_click(
        crate::ui::point(bounds.left() + px(120.), bounds.top() + px(10.)),
        crate::ui::text_edit::Modifiers::default(),
    );
    assert_eq!(editing_row(&ed, cx), Some(0));
    cx.simulate_input("typed");
    assert_eq!(buffer(&ed, cx).as_deref(), Some("typed"));
}

#[gpui_test]
fn rows_after_a_command_equal_a_full_rebuild(cx: &mut TestAppContext) {
    // Row reuse after local commands (BIT-T-0337) must be invisible: untouched blocks keep the
    // model they had, touched ones are parsed again.
    let env = Env::new(&[
        (HOME, "- one [[Other]]\n- **two**\n  - three #tag\n- four\n"),
        ("pages/Other.md", "- x\n"),
    ]);
    let (_view, ed, cx) = open_page(cx, &env, "Home");
    edit(&ed, 1, Caret::End, cx);
    cx.simulate_input(" more");
    cx.simulate_keystrokes("tab");
    flush(&ed, cx);
    let reused = ed.read_with(cx, |e, _| e.rows().to_vec());
    ed.update(cx, |e, cx| e.rebuild_all_rows_for_test(cx));
    let fresh = ed.read_with(cx, |e, _| e.rows().to_vec());
    assert_eq!(reused.len(), 4);
    assert_eq!(reused, fresh);
    assert_eq!(reused[1].depth, 1, "the second block was indented");
}
