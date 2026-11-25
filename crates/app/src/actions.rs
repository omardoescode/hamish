//! GPUI actions; key bindings are in [`crate::init`].

gpui::actions!(
    editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveToLineStart,
        MoveToLineEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectToLineStart,
        SelectToLineEnd,
        SelectAll,
        Backspace,
        Delete,
        Newline,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        OpenAlternatives,
        AiAlternatives,
        ToggleGhost,
        Stash,
        CloseMenu,
        DeleteActiveVariant,
    ]
);

gpui::actions!(
    alternatives_panel,
    [
        SelectPreviousVariant,
        SelectNextVariant,
        ConfirmVariant,
        ClosePanel,
        DeleteBackward,
        DeleteVariant,
    ]
);

gpui::actions!(
    workspace,
    [
        ToggleChrome,
        ToggleOverflow,
        Save,
        SaveAs,
        Open,
        OpenSettings,
        CloseSettings
    ]
);

gpui::actions!(overflow, [UseOverflowBlock, LeaveOverflow]);
