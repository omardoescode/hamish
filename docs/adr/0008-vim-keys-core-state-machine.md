---
status: accepted
---
# Vim keys through a core state machine, not GPUI key sequences

With Vim mode on, the Editor gives each key that no key binding takes to `Vim` in `crates/core/src/vim.rs`. `Vim` holds the mode (Normal or Insert), the count and the keys of a command not yet complete. It reads the key, the Document text and the Caret, and gives back one `VimCommand`: pass the key on, do nothing, select a Caret, or make a `CaretEdit`. The Editor runs the command through `EditSession` like any other caret action.

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
