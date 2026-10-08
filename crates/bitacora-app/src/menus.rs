//! Application menus: the app menu and the graph menu with "Open Recent" (BIT-US-0165).

use rust_i18n::t;

use crate::actions::{
    CloseGraph, OpenGraph, OpenRecentGraph1, OpenRecentGraph2, OpenRecentGraph3, OpenRecentGraph4,
    OpenRecentGraph5, OpenRecentGraph6, OpenRecentGraph7, OpenRecentGraph8, OpenRecentGraph9,
    OpenRecentGraph10, OpenSettings, Quit,
};
use crate::recent::RecentGraph;
use crate::ui::{Action, App, MenuEntry, MenuSpec};
use crate::views::settings::Section;

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

/// What a gear-menu row does when clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearCommand {
    /// Opens the settings (on `Some(section)`, or on the last one).
    OpenSettings(Option<Section>),
    /// Shows the native "open graph" dialog.
    OpenGraph,
    /// Opens the `n`-th (0-based) recent graph.
    OpenRecent(usize),
    /// Closes the open graph.
    CloseGraph,
    /// Quits the app.
    Quit,
}

/// A row of the gear (settings) menu in the title bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GearRow {
    /// A clickable row.
    Item {
        /// Localised label.
        label: String,
        /// What it does.
        command: GearCommand,
        /// Whether it is indented under the previous heading.
        nested: bool,
    },
    /// A non-clickable heading.
    Heading(String),
    /// A divider.
    Separator,
}

/// The rows of the gear menu: "Settings..." and one row per settings section, then the entries
/// of the graph menu (open, recent, close) and Quit, so the menu is the complete entry point to
/// the application menu on platforms without a native menu bar (BIT-US-0175).
pub fn gear_rows(recents: &[RecentGraph]) -> Vec<GearRow> {
    let item = |label: String, command, nested| GearRow::Item {
        label,
        command,
        nested,
    };
    let mut rows = vec![item(
        t!("settings.cmd_open").to_string(),
        GearCommand::OpenSettings(None),
        false,
    )];
    rows.extend(
        Section::ALL
            .iter()
            .map(|s| item(s.title(), GearCommand::OpenSettings(Some(*s)), true)),
    );
    rows.push(GearRow::Separator);
    rows.push(item(
        t!("menu.open_graph").to_string(),
        GearCommand::OpenGraph,
        false,
    ));
    rows.push(GearRow::Heading(t!("menu.open_recent").to_string()));
    if recents.is_empty() {
        rows.push(GearRow::Heading(t!("picker.no_recent").to_string()));
    }
    for (ix, graph) in recents.iter().enumerate().take(crate::recent::MAX_RECENT) {
        rows.push(item(graph.name(), GearCommand::OpenRecent(ix), true));
    }
    rows.push(item(
        t!("menu.close_graph").to_string(),
        GearCommand::CloseGraph,
        false,
    ));
    rows.push(GearRow::Separator);
    rows.push(item(t!("app.quit").to_string(), GearCommand::Quit, false));
    rows
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
    fn gear_menu_has_settings_sections_and_the_app_menu() {
        let rows = gear_rows(&recent(2));
        let cmds: Vec<GearCommand> = rows
            .iter()
            .filter_map(|r| match r {
                GearRow::Item { command, .. } => Some(*command),
                _ => None,
            })
            .collect();
        assert_eq!(cmds[0], GearCommand::OpenSettings(None));
        for s in Section::ALL {
            assert!(cmds.contains(&GearCommand::OpenSettings(Some(s))));
        }
        for c in [
            GearCommand::OpenGraph,
            GearCommand::OpenRecent(0),
            GearCommand::OpenRecent(1),
            GearCommand::CloseGraph,
            GearCommand::Quit,
        ] {
            assert!(cmds.contains(&c), "{c:?} missing");
        }
        assert!(!cmds.contains(&GearCommand::OpenRecent(2)));
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
