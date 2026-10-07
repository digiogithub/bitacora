//! Baseline application actions. Bindings live in `assets/keymaps/default.json`.

use crate::ui::actions;

actions!(
    bitacora,
    [
        ToggleLeftSidebar,
        ToggleRightSidebar,
        ToggleTheme,
        GoBack,
        GoForward,
        Quit,
        // Navigation (BIT-US-0078, BIT-US-0079).
        GoJournals,
        GoAllPages,
        // Palettes.
        OpenSearch,
        OpenCommandPalette,
        ClosePalette,
        OpenResultInSidebar,
        CycleSearchScope,
        // All pages table.
        OpenSelectedPage,
        // Right sidebar items (BIT-US-0080).
        FocusRightSidebar,
        SidebarSelectNext,
        SidebarSelectPrevious,
        SidebarToggleItem,
        SidebarCloseItem,
        SidebarOpenItem,
        // Settings (BIT-US-0107).
        OpenSettings,
        // Graph menu (BIT-US-0165).
        OpenGraph,
        CloseGraph,
        OpenRecentGraph1,
        OpenRecentGraph2,
        OpenRecentGraph3,
        OpenRecentGraph4,
        OpenRecentGraph5,
        OpenRecentGraph6,
        OpenRecentGraph7,
        OpenRecentGraph8,
        OpenRecentGraph9,
        OpenRecentGraph10,
    ]
);
