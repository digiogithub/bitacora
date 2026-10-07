//! View-logic tests of the graph view (BIT-US-0157, BIT-US-0159).

use std::time::Duration;

use bitacora_index::{GraphDataEdge, GraphDataNode, GraphFilter};

use crate::nav::OpenIn;
use crate::render::inline::NavTarget;
use crate::testing::TestGraph;
use crate::ui::Entity;
use crate::ui::testing::{TestAppContext, VisualTestContext, gpui_test};
use crate::views::graph_view::{GraphMode, GraphView};
use crate::views::page_view::PageEvent;
use crate::{settings::AppSettings, theme};

fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::ui::init(cx);
        theme::install(cx, AppSettings::default(), None);
    });
}

fn graph() -> TestGraph {
    TestGraph::new(&[
        ("pages/Alpha.md", "- see [[Beta]]\n"),
        ("pages/Beta.md", "- b\n"),
        ("pages/Gamma.md", "- [[Beta]] and [[Alpha]]\n"),
        ("pages/Delta.md", "- [[Gamma]]\n"),
        ("journals/2024_01_01.md", "- day [[Beta]]\n"),
    ])
}

fn wait_loaded(cx: &mut VisualTestContext, view: &Entity<GraphView>) {
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.run_until_parked();
        if view.read_with(cx, |v, _| v.is_loaded()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("graph did not load");
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    mode: GraphMode,
    g: &TestGraph,
) -> (Entity<GraphView>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|_, _| GraphView::new(mode));
    view.update(cx, |v, cx| v.show(g.handle.clone(), cx));
    wait_loaded(cx, &view);
    (view, cx)
}

fn settle(cx: &mut VisualTestContext, view: &Entity<GraphView>) {
    for _ in 0..1000 {
        let done = view.update(cx, |v, _| {
            v.sync_snapshot();
            v.is_settled() && !v.wants_frames()
        });
        if done {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("layout did not settle");
}

fn names(view: &Entity<GraphView>, cx: &VisualTestContext) -> Vec<String> {
    let mut found = view.read_with(cx, |v, _| {
        v.model()
            .nodes()
            .iter()
            .map(|n| n.name.clone())
            .collect::<Vec<_>>()
    });
    found.sort();
    found
}

fn world_of(view: &Entity<GraphView>, cx: &VisualTestContext, name: &str) -> [f32; 2] {
    view.read_with(cx, |v, _| {
        let ix = v
            .model()
            .nodes()
            .iter()
            .position(|n| n.name == name)
            .expect("node");
        v.positions()[ix]
    })
}

fn screen_of(view: &Entity<GraphView>, cx: &VisualTestContext, name: &str) -> [f32; 2] {
    view.read_with(cx, |v, _| {
        let ix = v
            .model()
            .nodes()
            .iter()
            .position(|n| n.name == name)
            .expect("node");
        v.viewport().to_screen(v.positions()[ix])
    })
}

#[gpui_test]
fn global_graph_lists_pages_without_journals(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    assert_eq!(names(&view, cx), ["Alpha", "Beta", "Delta", "Gamma"]);
    assert_eq!(view.read_with(cx, |v, _| v.model().links().len()), 4);
}

#[gpui_test]
fn layout_settles_and_stops_requesting_frames(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    assert!(!view.read_with(cx, |v, _| v.wants_frames()));
}

#[gpui_test]
fn clicking_a_node_opens_its_page_and_shift_opens_the_sidebar(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &PageEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
    });
    let at = screen_of(&view, cx, "Beta");
    view.update(cx, |v, cx| {
        v.pointer_down(at, false, cx);
        v.pointer_up(false, cx);
        v.pointer_down(at, false, cx);
        v.pointer_up(true, cx);
    });
    assert_eq!(
        *seen.borrow(),
        vec![
            PageEvent::open(NavTarget::Page("Beta".into()), OpenIn::Main),
            PageEvent::open(NavTarget::Page("Beta".into()), OpenIn::Sidebar),
        ]
    );
}

#[gpui_test]
fn dragging_a_node_moves_it_and_does_not_open_the_page(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(0));
    let sink = seen.clone();
    let _sub =
        cx.update(|_, cx| cx.subscribe(&view, move |_, _: &PageEvent, _| *sink.borrow_mut() += 1));
    let at = screen_of(&view, cx, "Gamma");
    let to = [at[0] + 120.0, at[1] + 80.0];
    view.update(cx, |v, cx| {
        v.pointer_down(at, false, cx);
        v.pointer_move(to, cx);
    });
    assert!(view.read_with(cx, |v, _| v.wants_frames()));
    let moved = screen_of(&view, cx, "Gamma");
    assert!((moved[0] - to[0]).abs() < 1.0 && (moved[1] - to[1]).abs() < 1.0);
    view.update(cx, |v, cx| v.pointer_up(false, cx));
    assert_eq!(*seen.borrow(), 0);
    // Releasing wakes the simulation to cool again, then frames stop.
    settle(cx, &view);
}

#[gpui_test]
fn dragging_empty_space_pans_and_the_wheel_zooms_at_the_cursor(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    let before = view.read_with(cx, |v, _| v.viewport());
    view.update(cx, |v, cx| {
        v.pointer_down([4000.0, 4000.0], false, cx);
        v.pointer_move([4030.0, 4010.0], cx);
        v.pointer_up(false, cx);
    });
    let panned = view.read_with(cx, |v, _| v.viewport());
    assert!((panned.offset[0] - before.offset[0] - 30.0).abs() < 1e-3);
    assert!((panned.offset[1] - before.offset[1] - 10.0).abs() < 1e-3);
    let cursor = [10.0, 20.0];
    let world = panned.to_world(cursor);
    view.update(cx, |v, cx| v.scroll(cursor, 200.0, cx));
    let zoomed = view.read_with(cx, |v, _| v.viewport());
    assert!(zoomed.zoom > panned.zoom);
    let after = zoomed.to_world(cursor);
    assert!((world[0] - after[0]).abs() < 1e-3 && (world[1] - after[1]).abs() < 1e-3);
}

#[gpui_test]
fn hovering_a_node_is_tracked(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    let at = screen_of(&view, cx, "Alpha");
    view.update(cx, |v, cx| v.pointer_move(at, cx));
    assert!(view.read_with(cx, |v, _| v.hover().is_some()));
    view.update(cx, |v, cx| v.pointer_move([5000.0, 5000.0], cx));
    assert_eq!(view.read_with(cx, |v, _| v.hover()), None);
}

#[gpui_test]
fn focus_limits_the_graph_to_n_hops_and_resets(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    let delta = view
        .read_with(cx, |v, _| {
            v.model()
                .nodes()
                .iter()
                .find(|n| n.name == "Delta")
                .map(|n| n.id)
        })
        .expect("Delta");
    // Secondary-click on the node toggles the focus.
    settle(cx, &view);
    let at = screen_of(&view, cx, "Delta");
    view.update(cx, |v, cx| {
        v.pointer_down(at, true, cx);
        v.pointer_up(false, cx);
    });
    assert_eq!(view.read_with(cx, |v, _| v.focus().to_vec()), [delta]);
    assert_eq!(names(&view, cx), ["Delta", "Gamma"]);
    view.update(cx, |v, cx| v.set_hops(2, cx));
    assert_eq!(names(&view, cx), ["Alpha", "Beta", "Delta", "Gamma"]);
    view.update(cx, |v, cx| v.set_hops(99, cx));
    assert_eq!(view.read_with(cx, |v, _| v.hops()), 6);
    view.update(cx, |v, cx| v.reset_focus(cx));
    assert_eq!(names(&view, cx).len(), 4);
    assert!(view.read_with(cx, |v, _| v.focus().is_empty()));
}

#[gpui_test]
fn refresh_keeps_surviving_positions_and_seeds_new_nodes_near_a_neighbour(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = open(cx, GraphMode::Global, &g);
    settle(cx, &view);
    let before = world_of(&view, cx, "Beta");
    let mut data = g
        .handle
        .reader
        .graph_data(&GraphFilter::default())
        .expect("data");
    let beta = data
        .nodes
        .iter()
        .find(|n| n.name == "Beta")
        .map(|n| n.id)
        .expect("Beta");
    data.nodes.push(GraphDataNode {
        id: 99_999,
        name: "Fresh".into(),
        is_journal: false,
        is_tag: false,
        is_namespace_parent: false,
        degree: 1,
    });
    data.edges.push(GraphDataEdge {
        src: 99_999,
        dst: beta,
    });
    // Read inside the same update: a later render would already pull a ticked snapshot.
    let (kept, fresh) = view.update(cx, |v, cx| {
        v.apply_data(&data, None, cx);
        let at = |name: &str| {
            let ix = v.model().nodes().iter().position(|n| n.name == name);
            v.positions()[ix.expect("node")]
        };
        (at("Beta"), at("Fresh"))
    });
    assert_eq!(kept, before);
    let d = (fresh[0] - before[0]).hypot(fresh[1] - before[1]);
    assert!((d - 30.0).abs() < 0.01, "distance {d}");
    // The refresh reheats the layout, which then settles again.
    settle(cx, &view);
    // Identical data is a no-op: the layout is not restarted.
    let ptr = view.read_with(cx, |v, _| v.positions().as_ptr());
    view.update(cx, |v, cx| v.apply_data(&data, None, cx));
    assert_eq!(view.read_with(cx, |v, _| v.positions().as_ptr()), ptr);
}

#[gpui_test]
fn local_graph_shows_the_page_and_its_neighbours(cx: &mut TestAppContext) {
    setup(cx);
    let g = graph();
    let (view, cx) = cx.add_window_view(|_, _| GraphView::new(GraphMode::Local));
    view.update(cx, |v, cx| {
        v.set_page(Some("Delta".into()), cx);
        v.show(g.handle.clone(), cx);
    });
    wait_loaded(cx, &view);
    assert_eq!(names(&view, cx), ["Delta", "Gamma"]);
}

#[gpui_test]
fn right_sidebar_local_graph_follows_the_page_and_click_navigates(cx: &mut TestAppContext) {
    use crate::ui::AppContext as _;
    use crate::views::right_panel::RightPanel;
    use crate::views::right_sidebar::{RightSidebar, StackEvent};
    setup(cx);
    let g = graph();
    let (panel, cx) = cx.add_window_view(|_, cx| {
        let stack = cx.new(RightSidebar::new);
        RightPanel::new(stack, cx)
    });
    let stack = panel.read_with(cx, |p, _| p.stack().clone());
    panel.update(cx, |s, cx| {
        s.set_graph(g.handle.clone(), cx);
        s.set_local_page(Some("Gamma".into()), cx);
    });
    let local = panel.read_with(cx, |s, _| s.local_graph().clone());
    for _ in 0..400 {
        cx.run_until_parked();
        if names(&local, cx).len() == 4 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(names(&local, cx), ["Alpha", "Beta", "Delta", "Gamma"]);
    panel.update(cx, |s, cx| s.set_local_page(Some("Delta".into()), cx));
    for _ in 0..400 {
        cx.run_until_parked();
        if names(&local, cx) == ["Delta", "Gamma"] {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(names(&local, cx), ["Delta", "Gamma"]);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _sub = cx.update(|_, cx| {
        cx.subscribe(&stack, move |_, event: &StackEvent, _| {
            sink.borrow_mut().push(event.clone());
        })
    });
    settle(cx, &local);
    let at = screen_of(&local, cx, "Gamma");
    local.update(cx, |v, cx| {
        v.pointer_down(at, false, cx);
        v.pointer_up(false, cx);
    });
    assert_eq!(
        *seen.borrow(),
        vec![StackEvent::Navigate(NavTarget::Page("Gamma".into()))]
    );
}
