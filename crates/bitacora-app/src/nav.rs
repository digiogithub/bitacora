//! Navigation model of the main area: routes and the back/forward history (BIT-US-0075).
//!
//! Pure logic with no GPUI types so it is unit-testable; the views feed it scroll positions.

/// What the main area shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// The journals feed.
    Journals,
    /// A page by title.
    Page(String),
    /// A block zoomed in, by UUID.
    Block(String),
    /// The table of all pages.
    AllPages,
    /// The open tasks of the graph (BIT-US-0126).
    Tasks,
}

/// Where a click on a ref, tag, bullet or list entry opens its target (Shift+click opens the
/// right sidebar, BIT-US-0080).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OpenIn {
    /// The main area.
    #[default]
    Main,
    /// The right sidebar stack.
    Sidebar,
}

impl OpenIn {
    /// Sidebar when Shift is held.
    pub fn from_shift(shift: bool) -> Self {
        if shift { Self::Sidebar } else { Self::Main }
    }
}

/// A scroll position of a virtualized list: the first visible item and the pixels scrolled
/// into it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Scroll {
    /// Index of the first visible item.
    pub item_ix: usize,
    /// Pixels scrolled into that item.
    pub offset_px: f32,
}

/// One visited location.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Where.
    pub route: Route,
    /// Where it was scrolled to when left.
    pub scroll: Scroll,
}

/// Maximum entries kept per direction.
const MAX_ENTRIES: usize = 100;

/// Back/forward stacks around the current entry.
#[derive(Debug, Clone, Default)]
pub struct NavHistory {
    back: Vec<Entry>,
    current: Option<Entry>,
    forward: Vec<Entry>,
}

impl NavHistory {
    /// An empty history.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current location.
    pub fn current(&self) -> Option<&Entry> {
        self.current.as_ref()
    }

    /// Whether [`NavHistory::back`] has somewhere to go.
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    /// Whether [`NavHistory::forward`] has somewhere to go.
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Remembers where the current entry was scrolled to.
    pub fn set_scroll(&mut self, scroll: Scroll) {
        if let Some(current) = self.current.as_mut() {
            current.scroll = scroll;
        }
    }

    fn push_back(&mut self, entry: Entry) {
        self.back.push(entry);
        if self.back.len() > MAX_ENTRIES {
            self.back.remove(0);
        }
    }

    /// Visits a new route: the current one goes to the back stack and the forward stack is
    /// cleared. Visiting the route already shown changes nothing.
    pub fn visit(&mut self, route: Route) {
        if self.current.as_ref().is_some_and(|c| c.route == route) {
            return;
        }
        if let Some(prev) = self.current.take() {
            self.push_back(prev);
        }
        self.forward.clear();
        self.current = Some(Entry {
            route,
            scroll: Scroll::default(),
        });
    }

    /// Goes back; returns the entry to show (with its saved scroll).
    pub fn back(&mut self) -> Option<Entry> {
        let target = self.back.pop()?;
        if let Some(cur) = self.current.replace(target.clone()) {
            self.forward.push(cur);
        }
        Some(target)
    }

    /// Goes forward; returns the entry to show (with its saved scroll).
    pub fn forward(&mut self) -> Option<Entry> {
        let target = self.forward.pop()?;
        if let Some(cur) = self.current.replace(target.clone()) {
            self.push_back(cur);
        }
        Some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str) -> Route {
        Route::Page(name.into())
    }

    #[test]
    fn visiting_builds_the_back_stack_and_clears_forward() {
        let mut h = NavHistory::new();
        assert!(!h.can_back() && !h.can_forward());
        h.visit(page("a"));
        h.visit(page("b"));
        h.visit(page("c"));
        assert!(h.can_back());
        assert_eq!(h.back().map(|e| e.route), Some(page("b")));
        assert!(h.can_forward());
        h.visit(page("d"));
        assert!(!h.can_forward());
        assert_eq!(h.back().map(|e| e.route), Some(page("b")));
        assert_eq!(h.back().map(|e| e.route), Some(page("a")));
        assert_eq!(h.back(), None);
        assert_eq!(h.forward().map(|e| e.route), Some(page("b")));
        assert_eq!(h.forward().map(|e| e.route), Some(page("d")));
        assert_eq!(h.forward(), None);
    }

    #[test]
    fn revisiting_the_current_route_is_a_no_op() {
        let mut h = NavHistory::new();
        h.visit(page("a"));
        h.visit(page("a"));
        assert!(!h.can_back());
    }

    #[test]
    fn scroll_positions_are_restored_on_back_and_forward() {
        let mut h = NavHistory::new();
        h.visit(page("a"));
        h.set_scroll(Scroll {
            item_ix: 42,
            offset_px: 7.5,
        });
        h.visit(Route::Journals);
        h.set_scroll(Scroll {
            item_ix: 3,
            offset_px: 0.,
        });
        let back = h.back().expect("back");
        assert_eq!(back.route, page("a"));
        assert_eq!(back.scroll.item_ix, 42);
        let fwd = h.forward().expect("forward");
        assert_eq!(fwd.route, Route::Journals);
        assert_eq!(fwd.scroll.item_ix, 3);
    }

    #[test]
    fn history_is_bounded() {
        let mut h = NavHistory::new();
        for n in 0..(MAX_ENTRIES + 20) {
            h.visit(page(&n.to_string()));
        }
        let mut count = 0;
        while h.back().is_some() {
            count += 1;
        }
        assert_eq!(count, MAX_ENTRIES);
    }
}
