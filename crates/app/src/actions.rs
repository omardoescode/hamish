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
    ]
);

gpui::actions!(workspace, [ToggleChrome]);
