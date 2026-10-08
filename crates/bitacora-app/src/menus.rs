//! Application menus: the app menu and the graph menu with "Open Recent" (BIT-US-0165).

use rust_i18n::t;

use crate::actions::{
    CloseGraph, OpenGraph, OpenRecentGraph1, OpenRecentGraph2, OpenRecentGraph3, OpenRecentGraph4,
    OpenRecentGraph5, OpenRecentGraph6, OpenRecentGraph7, OpenRecentGraph8, OpenRecentGraph9,
    OpenRecentGraph10, OpenSettings, Quit,
};
use crate::recent::RecentGraph;
use crate::ui::{Action, App, MenuEntry, MenuSpec};

/// The action of the `ix`-th (0-based) recent-graph entry, if the menu has a slot for it.
pub fn recent_action(ix: usize) -> Option<Box<dyn Action>> {
    let action: Box<dyn Action> = match ix {
        0 => Box::new(OpenRecentGraph1),
        1 => Box::new(OpenRecentGraph2),
        2 => Box::new(OpenRecentGraph3),
        3 => Box::new(OpenRecentGraph4),
        4 => Box::new(OpenRecentGraph5),
        5 => Box::new(OpenRecentGraph6),
        6 => Box::new(OpenRecentGraph7),
        7 => Box::new(OpenRecentGraph8),
        8 => Box::new(OpenRecentGraph9),
        9 => Box::new(OpenRecentGraph10),
        _ => return None,
    };
    Some(action)
}

/// The menus for the given recent graphs (most recent first).
pub fn build(recents: &[RecentGraph]) -> Vec<MenuSpec> {
    let mut recent_items: Vec<MenuEntry> = recents
        .iter()
        .enumerate()
        .filter_map(|(ix, g)| recent_action(ix).map(|a| MenuEntry::Action(g.name(), a)))
        .collect();
    if recent_items.is_empty() {
        recent_items.push(MenuEntry::Action(
            t!("picker.no_recent").to_string(),
            Box::new(OpenGraph),
        ));
    }
    vec![
        MenuSpec {
            name: t!("app.name").to_string(),
            items: vec![
                MenuEntry::Action(t!("settings.cmd_open").to_string(), Box::new(OpenSettings)),
                MenuEntry::Separator,
                MenuEntry::Action(t!("app.quit").to_string(), Box::new(Quit)),
            ],
        },
        MenuSpec {
            name: t!("menu.graph").to_string(),
            items: vec![
                MenuEntry::Action(t!("menu.open_graph").to_string(), Box::new(OpenGraph)),
                MenuEntry::Submenu(t!("menu.open_recent").to_string(), recent_items),
                MenuEntry::Separator,
                MenuEntry::Action(t!("menu.close_graph").to_string(), Box::new(CloseGraph)),
            ],
        },
    ]
}

/// Installs the menus for `recents`.
pub fn install(cx: &mut App, recents: &[RecentGraph]) {
    crate::ui::set_menus(cx, build(recents));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent(n: usize) -> Vec<RecentGraph> {
        (0..n)
            .map(|i| RecentGraph {
                path: format!("/g/{i}").into(),
            })
            .collect()
    }

    #[test]
    fn graph_menu_lists_open_recent_and_close() {
        let menus = build(&recent(3));
        assert_eq!(menus.len(), 2);
        let graph = &menus[1];
        assert_eq!(graph.items.len(), 4);
        assert!(matches!(&graph.items[1], MenuEntry::Submenu(_, items) if items.len() == 3));
    }

    #[test]
    fn open_settings_is_bound_to_secondary_comma() {
        let sections = crate::keymap::parse(crate::keymap::DEFAULT_KEYMAP).expect("parses");
        assert!(sections.iter().any(|s| {
            s.bindings.iter().any(|(k, a)| {
                a == "bitacora::OpenSettings"
                    && crate::keymap::normalize_keys(k)
                        == crate::keymap::normalize_keys("secondary-,")
            })
        }));
    }

    #[test]
    fn recent_slots_cover_the_recent_cap() {
        assert!((0..crate::recent::MAX_RECENT).all(|i| recent_action(i).is_some()));
        assert!(recent_action(crate::recent::MAX_RECENT).is_none());
    }
}
