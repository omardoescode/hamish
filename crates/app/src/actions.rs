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
    ]
);

gpui::actions!(workspace, [ToggleChrome, ToggleOverflow]);

gpui::actions!(overflow, [UseOverflowBlock, LeaveOverflow]);
