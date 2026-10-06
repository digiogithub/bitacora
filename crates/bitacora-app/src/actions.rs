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
    ]
);
