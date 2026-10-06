//! GPUI actions of the block editor (BIT-US-0031). Bindings live in the `outliner` sections of
//! `assets/keymaps/default.json`; the four key contexts are:
//!
//! | Context | Active when | Examples |
//! |---|---|---|
//! | `Outliner` | the page outline has focus | `t o`, undo/redo |
//! | `BlockSelection` | blocks are selected (no text caret) | Shift+Up/Down, Backspace |
//! | `BlockEditor` | one block is in edit mode | Enter, Tab, arrows |
//! | `Autocomplete` | a completion popup is open | Enter, Up/Down, Esc |
//!
//! Precedence is `Autocomplete` > `BlockEditor` > `BlockSelection` > `Outliner`: a focused outline
//! sets the context to `Outliner` plus the mode it is in, and the keymap lists the more specific
//! sections last, so the later binding wins.

use crate::ui::actions;

actions!(
    outliner,
    [
        // Text caret (BlockEditor).
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        SelectAllText,
        DeleteBackward,
        DeleteForward,
        DeleteWordBackward,
        DeleteWordForward,
        NewBlock,
        InsertNewline,
        Copy,
        CopyEmbed,
        Cut,
        Paste,
        PasteRaw,
        ExitEdit,
        ShowCharacterPalette,
        // Structure (BlockEditor and BlockSelection).
        Indent,
        Outdent,
        MoveBlockUp,
        MoveBlockDown,
        CollapseBlock,
        ExpandBlock,
        CycleMarker,
        ZoomIn,
        ZoomOut,
        Undo,
        Redo,
        ToggleCollapseAll,
        // Block selection.
        SelectionUp,
        SelectionDown,
        ExtendSelectionUp,
        ExtendSelectionDown,
        SelectAllBlocks,
        SelectParent,
        EditSelected,
        ClearSelection,
        DeleteSelected,
        // Autocomplete popup.
        AcceptCompletion,
        CompletionNext,
        CompletionPrevious,
        DismissCompletion,
    ]
);

/// Key context names.
pub mod context {
    /// The page outline has focus.
    pub const OUTLINER: &str = "Outliner";
    /// One block is in edit mode.
    pub const BLOCK_EDITOR: &str = "BlockEditor";
    /// Blocks are selected.
    pub const BLOCK_SELECTION: &str = "BlockSelection";
    /// A completion popup is open.
    pub const AUTOCOMPLETE: &str = "Autocomplete";
}

/// Platform word-motion modifier (Alt on macOS, Ctrl elsewhere).
#[cfg(target_os = "macos")]
const WORD: &str = "alt";
#[cfg(not(target_os = "macos"))]
const WORD: &str = "ctrl";

/// Bindings that cannot be written in the JSON keymap because the modifier depends on the
/// platform.
pub fn platform_bindings() -> Vec<(String, &'static str, &'static str)> {
    let w = WORD;
    #[allow(unused_mut)] // only the non-macOS bindings push more
    let mut out = vec![
        (format!("{w}-left"), "outliner::WordLeft", "BlockEditor"),
        (format!("{w}-right"), "outliner::WordRight", "BlockEditor"),
        (
            format!("{w}-shift-left"),
            "outliner::SelectWordLeft",
            "BlockEditor",
        ),
        (
            format!("{w}-shift-right"),
            "outliner::SelectWordRight",
            "BlockEditor",
        ),
        (
            format!("{w}-backspace"),
            "outliner::DeleteWordBackward",
            "BlockEditor",
        ),
        (
            format!("{w}-delete"),
            "outliner::DeleteWordForward",
            "BlockEditor",
        ),
    ];
    // Alt+Left/Right zoom out/in; on macOS Alt is the word-motion modifier.
    #[cfg(not(target_os = "macos"))]
    for context in ["Outliner", "BlockEditor", "BlockSelection"] {
        out.push(("alt-right".to_owned(), "outliner::ZoomIn", context));
        out.push(("alt-left".to_owned(), "outliner::ZoomOut", context));
    }
    out
}
