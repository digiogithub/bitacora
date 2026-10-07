//! Click-to-edit of blocks shown from other pages (BIT-US-0168): query results, backlinks, the
//! Tasks view and the zoomed block of a sidebar item. Linked references are covered in
//! `page_view`'s tests and embeds in `widgets`' tests.

use std::rc::Rc;
use std::time::Duration;

use bitacora_core::queue::Source;

use crate::editor::OutlineEditor;
use crate::nav::{OpenIn, Route};
use crate::render::inline::NavTarget;
use crate::render::query::Scope;
use crate::render::widget::{QueryKind, QueryProps, QuerySpec};
use crate::settings::AppSettings;
use crate::theme;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::ui::{App, AppContext as _, Entity};
use crate::views::block_view::Nav;
use crate::views::page_view::PageView;
use crate::views::right_panel::RightPanel;
use crate::views::right_sidebar::RightSidebar;
use crate::views::tasks::TasksView;
use crate::views::widgets::QueryBlock;
use crate::views::widgets::tests::Env;

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
        crate::keymap::load_with_user(cx, None).expect("keymap");
    });
}

fn nav() -> Nav {
    Rc::new(|_: NavTarget, _: OpenIn, _: &mut App| {})
}

fn settle<V: 'static>(cx: &mut VisualTestContext, view: &Entity<V>, done: impl Fn(&V) -> bool) {
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if view.read_with(cx, |v, _| done(v)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the view did not settle");
}

/// Types `!` into the block in edit mode of `ed` and checks that only `page` changed on disk,
/// that exactly the clicked block got the character and that one undo restores the bytes.
fn type_and_undo(
    cx: &mut VisualTestContext,
    env: &Env,
    ed: &Entity<OutlineEditor>,
    page: &str,
    untouched: &[&str],
    expected: &str,
) {
    let before = env.disk(page);
    let others: Vec<String> = untouched.iter().map(|p| env.disk(p)).collect();
    assert!(ed.read_with(cx, |e, _| e.editing().is_some()), "edit mode");
    cx.simulate_input("!");
    ed.update(cx, |e, cx| {
        e.flush(cx);
    });
    assert_eq!(env.disk(page), expected);
    for (path, bytes) in untouched.iter().zip(&others) {
        assert_eq!(&env.disk(path), bytes, "{path} must not change");
    }
    ed.update(cx, |e, cx| e.exit_edit(cx));
    env.link
        .queue
        .undo(Source::Ui)
        .expect("queue")
        .expect("step");
    assert_eq!(env.disk(page), before, "undo restores the bytes");
}

#[gpui_test]
fn query_results_edit_their_source_block_on_click(cx: &mut TestAppContext) {
    setup(cx);
    let env = Env::new(&[
        ("pages/A.md", "- TODO write docs\n- note\n"),
        ("pages/B.md", "- TODO review\n"),
    ]);
    let (handle, link) = (env.handle.clone(), env.link.clone());
    let spec = QuerySpec {
        kind: QueryKind::Simple,
        source: "(task TODO)".to_owned(),
        props: QueryProps::default(),
    };
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut q = QueryBlock::new(handle, spec, Scope::default(), nav(), None, cx);
        q.set_link(Some(link));
        q
    });
    settle(cx, &view, QueryBlock::is_settled);
    view.update_in(cx, |q, window, cx| q.click_result(0, window, cx));
    let ed = view.read_with(cx, |q, _| q.editor("A").expect("editor"));
    type_and_undo(
        cx,
        &env,
        &ed,
        "pages/A.md",
        &["pages/B.md"],
        "- TODO write docs!\n- note\n",
    );
}

#[gpui_test]
fn tasks_edit_their_block_on_click(cx: &mut TestAppContext) {
    setup(cx);
    let env = Env::new(&[
        ("pages/Work.md", "- TODO first\n- TODO second\n"),
        ("pages/Home.md", "- TODO water plants\n"),
    ]);
    let (handle, link) = (env.handle.clone(), env.link.clone());
    let (view, cx) = cx.add_window_view(|_, cx| TasksView::new(cx));
    view.update(cx, |v, cx| {
        v.set_session_link(Some(link), cx);
        v.show(handle, cx);
    });
    settle(cx, &view, TasksView::is_loaded);
    let uuid = view.read_with(cx, |v, _| {
        v.model()
            .rows
            .iter()
            .find(|r| r.title == "second")
            .expect("second")
            .uuid
            .clone()
    });
    view.update_in(cx, |v, window, cx| v.click_task(&uuid, window, cx));
    let ed = view.read_with(cx, |v, _| v.editor("Work").expect("editor"));
    type_and_undo(
        cx,
        &env,
        &ed,
        "pages/Work.md",
        &["pages/Home.md"],
        "- TODO first\n- TODO second!\n",
    );
}

#[gpui_test]
fn backlinks_edit_their_block_on_click(cx: &mut TestAppContext) {
    setup(cx);
    let env = Env::new(&[
        ("pages/Alpha.md", "- alpha\n"),
        ("pages/Beta.md", "- see [[Alpha]]\n- other\n"),
    ]);
    let (handle, link) = (env.handle.clone(), env.link.clone());
    let (panel, cx) = cx.add_window_view(|_, cx| {
        let stack = cx.new(RightSidebar::new);
        RightPanel::new(stack, cx)
    });
    panel.update(cx, |p, cx| {
        p.set_session_link(Some(link), cx);
        p.set_graph(handle, cx);
        p.set_local_page(Some("Alpha".to_owned()), cx);
    });
    settle(cx, &panel, |p| !p.context().backlinks.is_empty());
    panel.update_in(cx, |p, window, cx| p.click_backlink((0, 0), window, cx));
    let ed = panel.read_with(cx, |p, _| p.editor("Beta").expect("editor"));
    type_and_undo(
        cx,
        &env,
        &ed,
        "pages/Beta.md",
        &["pages/Alpha.md"],
        "- see [[Alpha]]!\n- other\n",
    );
}

#[gpui_test]
fn zoomed_blocks_of_a_read_only_view_edit_in_place(cx: &mut TestAppContext) {
    setup(cx);
    let id = "6500c1a4-0000-4000-8000-0000000000aa";
    let src = format!("- parent\n  id:: {id}\n\t- child\n- other\n");
    let env = Env::new(&[("pages/Source.md", &src)]);
    let (view, cx) = cx.add_window_view(|_, cx| PageView::new(cx));
    view.update(cx, |v, cx| {
        v.set_remote_link(Some(env.link.clone()));
        v.show(env.handle.clone(), Route::Block(id.into()), None, cx);
    });
    settle(cx, &view, |v| v.rows().len() == 2);
    // The child row (index 1) of the zoomed subtree.
    view.update_in(cx, |v, window, cx| v.click_block_row(1, window, cx));
    let ed = view.read_with(cx, |v, _| v.remote_editor("Source").expect("editor"));
    type_and_undo(
        cx,
        &env,
        &ed,
        "pages/Source.md",
        &[],
        &format!("- parent\n  id:: {id}\n\t- child!\n- other\n"),
    );
}
