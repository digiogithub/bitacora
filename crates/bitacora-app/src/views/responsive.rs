//! Responsive behaviour of the shell (BIT-US-0128, BIT-SP-0008.R6).
//!
//! The shell is laid out for wide windows: 252px sidebar + reading column (760px max + padding)
//! and a 360px right panel. Below [`RIGHT_PANEL_INLINE_MIN`] the right panel no longer fits next to
//! the reading column and is closed when the window shrinks past it (reopen with
//! `Ctrl/Cmd+Shift+B`); below [`SIDEBAR_INLINE_MIN`] the sidebar leaves the layout and is shown
//! on demand as an overlay (`Ctrl/Cmd+B`).

/// Smallest window width that shows the right panel inline (252 + 760 + 2*24 padding + 360 ~ 1170).
pub const RIGHT_PANEL_INLINE_MIN: f32 = 1170.0;

/// Smallest window width that shows the left sidebar inline.
pub const SIDEBAR_INLINE_MIN: f32 = 760.0;

/// How the shell arranges its panels at a given window width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Breakpoint {
    /// Sidebar and right panel inline.
    Wide,
    /// Sidebar inline; the right panel does not fit and starts closed.
    Medium,
    /// Only the reading column; the sidebar is an overlay.
    Narrow,
}

impl Breakpoint {
    /// The breakpoint for a window width in logical pixels.
    pub fn for_width(width: f32) -> Self {
        if width >= RIGHT_PANEL_INLINE_MIN {
            Self::Wide
        } else if width >= SIDEBAR_INLINE_MIN {
            Self::Medium
        } else {
            Self::Narrow
        }
    }

    /// Whether the left sidebar takes part in the layout (otherwise it is an overlay).
    pub fn sidebar_inline(self) -> bool {
        self != Self::Narrow
    }

    /// Whether the right panel fits next to the reading column.
    pub fn right_panel_inline(self) -> bool {
        self == Self::Wide
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds() {
        assert_eq!(Breakpoint::for_width(1920.0), Breakpoint::Wide);
        assert_eq!(Breakpoint::for_width(1170.0), Breakpoint::Wide);
        assert_eq!(Breakpoint::for_width(1169.0), Breakpoint::Medium);
        assert_eq!(Breakpoint::for_width(1000.0), Breakpoint::Medium);
        assert_eq!(Breakpoint::for_width(760.0), Breakpoint::Medium);
        assert_eq!(Breakpoint::for_width(640.0), Breakpoint::Narrow);
        assert!(Breakpoint::Medium.sidebar_inline());
        assert!(!Breakpoint::Narrow.sidebar_inline());
        assert!(!Breakpoint::Medium.right_panel_inline());
    }
}
