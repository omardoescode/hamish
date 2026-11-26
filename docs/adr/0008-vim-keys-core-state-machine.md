---
status: accepted
---
# Vim keys through a core state machine, not GPUI key sequences

With Vim mode on, the Editor gives each key that no key binding takes to `Vim` in `crates/core/src/vim.rs` (its modules are in `crates/core/src/vim/`). `Vim` holds the mode (Normal, Insert or Visual), the count, the keys of a command not yet complete and the last change for dot repeat. It reads the key, the Document text (with its Slot and Ghost spans) and the Caret, and gives back one `VimCommand`: pass the key on, do nothing, select a Caret, make a `CaretEdit`, yank or cut to the register, put from the register, undo or redo. The Editor runs the command through `EditSession` like any other caret action.

## Why not GPUI key sequences

The pinned GPUI (ADR 0009) can bind multi-key sequences (`g g`) in a key context: `KeyBinding::new` splits its keystrokes at whitespace. We do not use them for Vim keys:

- A pending sequence flushes after 1 s (`Window::dispatch_key_event`). Vim waits with no time limit.
- Counts (`3w`, `12j`) and, later, operators with motions (`d2w`) are a grammar, not a fixed list of sequences. One binding per combination does not scale.
- In `core`, the grammar has unit tests with no window (ADR 0005).

## Consequences

- Key bindings still win: GPUI dispatches a bound key as its action before the key-down listener sees it. The key contexts `VimNormal` and `VimInsert` let bindings differ per mode.
- In Normal mode the Editor stops each key that `Vim` uses, and the text input does not take text (`CaretView::takes_text`).
- Escape is a bound action (`CloseMenu`). In Insert mode it goes to Normal mode. In Normal mode it clears the count and goes up the tree, so it still closes the Alternatives panel.
- The Chrome bar shows the mode and the keys of a command not yet complete (`Vim::status`).
- The register is the system clipboard. Core has no clipboard, so a put comes back as `VimCommand::Put`; the Editor reads the clipboard and gives its text to `Put::command`.
- `u` and `Ctrl+R` are the Editor undo and redo. One Vim command is one Undo step: an operator edit joins no other. One Insert mode session, from the key that starts it to Escape or `jk`, is one Undo step with all its typing and deleting (`EditSession::start_insert`); a caret move in it starts a new one.
- In Visual mode a click moves the head of the Visual selection, and a shift-click moves the end nearer to the click (`Vim::click`).
- An operator edit over a Slot edge goes through the same `EditSession::edit` path as a selection delete (ADR 0006).
- Panels read Vim keys in core, not through GPUI bindings. Each panel has its own command type, and all share `RowCommand` for `j`, `k`, Enter and Escape: the Alternatives panel (`AlternativesKeys`, Insert-only after `i`), the Lab popover (`LabCommand`) and Settings (`SettingsCommand`). Overflow text has its own `Vim`; `Ctrl+W h` and `Ctrl+W l` move the keyboard between the Editor and Overflow. While a Trim proposal is open, `n` and `N` in Normal mode walk the cuts, Enter makes them and Escape closes the Trim bar; `k` stays the up motion, and the leader `k` keeps the cut at the caret.
- `:` in Normal mode comes back as `VimCommand::Run` and opens the Command line at the window bottom. `CommandLine` in core reads its keys and gives back a `LineCommand` or a `LineError`. It also keeps the caret, the recent commands of the session and the Tab candidates. The caret blinks with the caret blink Effect. The Workspace draws the line, runs the command through the same save and open code as the key bindings, and shows an error as a Notice. Tab on a path lists the directory on the background executor; core then picks the candidates. `:set` changes only the Vim mode setting.
- Key maps of the hamishrc act before the grammar, also in core: each key goes into a key feed in `Vim` (`Vim::press`), and `Vim::next_key` gives it out mapped (`Fed`). The Editor gives each mapped key to `Vim::key` and runs its command before it asks for the next key, so the mode of each key picks its maps, as in Vim. Keys that can still grow into a longer map wait with no time limit. A key that a map takes or holds does not reach the text input; in Insert mode the Editor types the mapped keys itself. A map target `:Name` names a `VimAction` from one name table in core; an unknown name is a line-numbered error. The app runs the GPUI action of each `VimAction`, for a map and for a leader key, from one table (`gpui_action`); no reflection. `:Quit` follows the `:q` rule; `:ForceQuit` acts as `:q!`. `let mapleader` changes the leader of the Hamish keys too. Keys that a GPUI key binding takes (Escape, and Enter and Backspace in Insert mode) never reach the maps. Escape, and a key that no map can hold, first flush the held keys (`Vim::flush_key`), as in Vim.
- Hamish keys are in the same grammar: Ghost, Revive and Stash are operators (`gh`, `gH`, `gs`), `]` and `[` jump or cycle a Slot, and Space is the leader. A leader key comes back as `VimCommand::Run`; the Editor dispatches the same GPUI action as the key binding or menu item.
