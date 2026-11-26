//! The Vim mode row keys that the Alternatives panel, the Lab and Settings share: `j` and
//! `k` move between the rows, Enter acts on the row, and Escape closes the panel.

use gpui::{App, Context, Window};
use hamish_core::RowCommand;

/// A panel with rows that the Vim mode keys walk.
pub(crate) trait VimRows: Sized + 'static {
    /// The row that the Vim mode keys act on.
    fn vim_row(&self, cx: &App) -> usize;

    /// Makes `row` the row of the Vim mode keys; a row past the last gives the last.
    fn select_vim_row(&mut self, row: usize, cx: &mut Context<Self>);

    /// Enter: acts on the row.
    fn enter_vim_row(&mut self, window: &mut Window, cx: &mut Context<Self>);

    /// Escape: closes the panel.
    fn close_vim_rows(&mut self, window: &mut Window, cx: &mut Context<Self>);
}

/// Runs `command` on the rows of `view`.
pub(crate) fn run_row_command<V: VimRows>(
    view: &mut V,
    command: RowCommand,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    match command {
        RowCommand::Down => {
            let row = view.vim_row(cx) + 1;
            view.select_vim_row(row, cx);
        }
        RowCommand::Up => {
            let row = view.vim_row(cx).saturating_sub(1);
            view.select_vim_row(row, cx);
        }
        RowCommand::Enter => view.enter_vim_row(window, cx),
        RowCommand::Close => view.close_vim_rows(window, cx),
    }
}
