//! Baseline application actions. Bindings live in `assets/keymaps/default.json`.

use crate::ui::actions;

actions!(
    bitacora,
    [ToggleLeftSidebar, ToggleRightSidebar, ToggleTheme, Quit]
);
