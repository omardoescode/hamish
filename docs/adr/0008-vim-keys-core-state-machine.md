---
status: accepted
---
# Vim keys through a core state machine, not GPUI key sequences

With Vim mode on, the Editor gives each key that no key binding takes to `Vim` in `crates/core/src/vim.rs`. `Vim` holds the mode (Normal, Insert or Visual), the count, the keys of a command not yet complete and the last change for dot repeat. It reads the key, the Document text (with its Slot and Ghost spans) and the Caret, and gives back one `VimCommand`: pass the key on, do nothing, select a Caret, make a `CaretEdit`, yank or cut to the register, put from the register, undo or redo. The Editor runs the command through `EditSession` like any other caret action.

## Why not GPUI key sequences

GPUI 0.2.2 can bind multi-key sequences (`g g`) in a key context. We do not use them for Vim keys:

- A pending sequence flushes after 1 s (`Window::dispatch_key_event`). Vim waits with no time limit.
- Counts (`3w`, `12j`) and, later, operators with motions (`d2w`) are a grammar, not a fixed list of sequences. One binding per combination does not scale.
- In `core`, the grammar has unit tests with no window (ADR 0005).

## Consequences

- Key bindings still win: GPUI dispatches a bound key as its action before the key-down listener sees it. The key contexts `VimNormal` and `VimInsert` let bindings differ per mode.
- In Normal mode the Editor stops each key that `Vim` uses, and the text input does not take text (`CaretView::takes_text`).
- Escape is a bound action (`CloseMenu`). In Insert mode it goes to Normal mode. In Normal mode it clears the count and goes up the tree, so it still closes the Alternatives panel.
- The Chrome bar shows the mode and the keys of a command not yet complete (`Vim::status`).
- The register is the system clipboard. Core has no clipboard, so a put comes back as `VimCommand::Put`; the Editor reads the clipboard and gives its text to `Put::command`.
- `u` and `Ctrl+R` are the Editor undo and redo. One Vim command is one Undo step: an operator edit joins no other, and a change joins the typing of the Insert mode it starts (`EditKind::Insert`).
- An operator edit over a Slot edge goes through the same `EditSession::edit` path as a selection delete (ADR 0006).
