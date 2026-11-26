use std::path::{Path, PathBuf};

use hamish_core::{
    Caret, CommandLine, DirectoryEntry, LineCommand, LineError, LineKey, LineStep, Vim, VimAction,
    VimCommand, VimKey, resolve_path,
};

fn type_line(line: &mut CommandLine, typed: &str) {
    for key in typed.chars() {
        assert_eq!(line.key(LineKey::Char(key)), LineStep::Open);
    }
}

fn submit(typed: &str) -> LineStep {
    let mut line = CommandLine::default();
    type_line(&mut line, typed);
    line.key(LineKey::Enter)
}

fn run(command: LineCommand) -> LineStep {
    LineStep::Run(Ok(command))
}

fn entry(name: &str, directory: bool) -> DirectoryEntry {
    DirectoryEntry {
        name: name.to_owned(),
        directory,
    }
}

#[test]
fn colon_in_normal_mode_opens_the_command_line() {
    let mut vim = Vim::default();

    let command = vim.key(VimKey::Char(':'), "one two", &Caret::at(0));

    assert_eq!(command, VimCommand::Run(VimAction::OpenCommandLine));
}

#[test]
fn colon_after_an_operator_cancels_the_operator() {
    let mut vim = Vim::default();
    vim.key(VimKey::Char('d'), "one two", &Caret::at(0));

    let command = vim.key(VimKey::Char(':'), "one two", &Caret::at(0));

    assert_eq!(command, VimCommand::Nothing);
    assert_eq!(vim.status(), "NORMAL");
}

#[test]
fn write_commands_parse_with_and_without_a_path() {
    assert_eq!(submit("w"), run(LineCommand::Write(None)));
    assert_eq!(submit("write"), run(LineCommand::Write(None)));
    assert_eq!(
        submit("w  notes/draft.md "),
        run(LineCommand::Write(Some("notes/draft.md".to_owned())))
    );
}

#[test]
fn quit_commands_parse_with_and_without_force() {
    assert_eq!(submit("q"), run(LineCommand::Quit { force: false }));
    assert_eq!(submit("quit"), run(LineCommand::Quit { force: false }));
    assert_eq!(submit("q!"), run(LineCommand::Quit { force: true }));
    assert_eq!(
        submit("wq"),
        run(LineCommand::WriteQuit {
            path: None,
            only_unsaved: false
        })
    );
    assert_eq!(
        submit("x"),
        run(LineCommand::WriteQuit {
            path: None,
            only_unsaved: true
        })
    );
}

#[test]
fn edit_commands_parse_a_path_and_force() {
    assert_eq!(
        submit("e draft.md"),
        run(LineCommand::Edit {
            path: Some("draft.md".to_owned()),
            force: false
        })
    );
    assert_eq!(
        submit("edit! ~/a b.md"),
        run(LineCommand::Edit {
            path: Some("~/a b.md".to_owned()),
            force: true
        })
    );
    assert_eq!(
        submit("e!"),
        run(LineCommand::Edit {
            path: None,
            force: true
        })
    );
}

#[test]
fn set_changes_only_the_vim_mode_setting() {
    assert_eq!(submit("set vim"), run(LineCommand::SetVimMode(true)));
    assert_eq!(submit("set novim"), run(LineCommand::SetVimMode(false)));
    assert_eq!(
        submit("set wrap"),
        LineStep::Run(Err(LineError::UnknownSetting("wrap".to_owned())))
    );
    assert_eq!(submit("set"), LineStep::Run(Err(LineError::NoSetting)));
}

#[test]
fn chrome_and_settings_parse() {
    assert_eq!(submit("chrome"), run(LineCommand::ToggleChrome));
    assert_eq!(submit("settings"), run(LineCommand::OpenSettings));
}

#[test]
fn an_unknown_command_or_extra_text_is_an_error_with_a_message() {
    let unknown = submit("source ~/.hamishrc");
    assert_eq!(
        unknown,
        LineStep::Run(Err(LineError::Unknown("source".to_owned())))
    );
    let LineStep::Run(Err(error)) = unknown else {
        unreachable!()
    };
    assert_eq!(error.to_string(), "Not a command: source");
    assert_eq!(
        submit("q now"),
        LineStep::Run(Err(LineError::ExtraText("now".to_owned())))
    );
}

#[test]
fn enter_or_escape_closes_and_clears_the_line() {
    let mut line = CommandLine::default();
    type_line(&mut line, "w");
    assert_eq!(line.text(), "w");

    assert_eq!(line.key(LineKey::Escape), LineStep::Closed);
    assert_eq!(line.text(), "");
    assert_eq!(line.key(LineKey::Enter), LineStep::Closed);
}

#[test]
fn backspace_deletes_a_char_and_closes_an_empty_line() {
    let mut line = CommandLine::default();
    type_line(&mut line, "wα");

    assert_eq!(line.key(LineKey::Backspace), LineStep::Open);
    assert_eq!(line.text(), "w");
    assert_eq!(line.key(LineKey::Backspace), LineStep::Open);
    assert_eq!(line.key(LineKey::Backspace), LineStep::Closed);
}

#[test]
fn up_and_down_walk_the_history_and_down_past_the_end_gives_back_the_typed_text() {
    let mut line = CommandLine::default();
    for typed in ["w", "set vim", "chrome"] {
        type_line(&mut line, typed);
        line.key(LineKey::Enter);
    }
    type_line(&mut line, "e");

    line.key(LineKey::Up);
    assert_eq!(line.text(), "chrome");
    line.key(LineKey::Up);
    line.key(LineKey::Up);
    assert_eq!(line.text(), "w");
    line.key(LineKey::Up);
    assert_eq!(line.text(), "w");
    line.key(LineKey::Down);
    assert_eq!(line.text(), "set vim");
    line.key(LineKey::Down);
    line.key(LineKey::Down);
    assert_eq!(line.text(), "e");
}

#[test]
fn the_same_line_twice_in_a_row_is_one_history_entry() {
    let mut line = CommandLine::default();
    for typed in ["w", "w", "chrome"] {
        type_line(&mut line, typed);
        line.key(LineKey::Enter);
    }

    line.key(LineKey::Up);
    line.key(LineKey::Up);
    assert_eq!(line.text(), "w");
    line.key(LineKey::Up);
    assert_eq!(line.text(), "w");
    line.key(LineKey::Down);
    assert_eq!(line.text(), "chrome");
}

#[test]
fn tab_completes_a_command_name_and_more_tabs_cycle_the_candidates() {
    let mut line = CommandLine::default();
    type_line(&mut line, "se");

    assert_eq!(line.key(LineKey::Tab), LineStep::Open);
    assert_eq!(line.text(), "set");
    line.key(LineKey::Tab);
    assert_eq!(line.text(), "settings");
    line.key(LineKey::Tab);
    assert_eq!(line.text(), "set");
}

#[test]
fn tab_completes_the_setting_of_set() {
    let mut line = CommandLine::default();
    type_line(&mut line, "set no");

    line.key(LineKey::Tab);

    assert_eq!(line.text(), "set novim");
}

#[test]
fn tab_after_a_path_command_asks_for_the_typed_directory() {
    let mut line = CommandLine::default();
    type_line(&mut line, "e notes/dr");

    assert_eq!(
        line.key(LineKey::Tab),
        LineStep::ListDirectory("notes/".to_owned())
    );
    assert_eq!(line.text(), "e notes/dr");
}

#[test]
fn directory_entries_complete_the_path_with_a_slash_after_a_directory() {
    let mut line = CommandLine::default();
    type_line(&mut line, "w notes/dr");
    line.key(LineKey::Tab);

    line.complete_paths(
        "notes/",
        vec![
            entry("drafts", true),
            entry("other.md", false),
            entry("draft.md", false),
        ],
    );

    assert_eq!(line.text(), "w notes/draft.md");
    line.key(LineKey::Tab);
    assert_eq!(line.text(), "w notes/drafts/");
}

#[test]
fn dot_entries_complete_only_after_a_typed_dot() {
    let mut line = CommandLine::default();
    type_line(&mut line, "e ");
    assert_eq!(
        line.key(LineKey::Tab),
        LineStep::ListDirectory(String::new())
    );

    line.complete_paths("", vec![entry(".git", true), entry("a.md", false)]);

    assert_eq!(line.text(), "e a.md");
}

#[test]
fn entries_for_a_line_the_writer_changed_since_tab_do_nothing() {
    let mut line = CommandLine::default();
    type_line(&mut line, "e dr");
    line.key(LineKey::Tab);
    type_line(&mut line, "x");

    line.complete_paths("", vec![entry("draft.md", false)]);

    assert_eq!(line.text(), "e drx");
}

#[test]
fn a_command_refuses_to_drop_unsaved_changes_without_force() {
    let quit = LineCommand::Quit { force: false };
    let edit = LineCommand::Edit {
        path: Some("b.md".to_owned()),
        force: false,
    };

    assert_eq!(quit.refusal(true, true), Some(LineError::Unsaved));
    assert_eq!(quit.refusal(false, true), None);
    assert_eq!(LineCommand::Quit { force: true }.refusal(true, true), None);
    assert_eq!(edit.refusal(true, true), Some(LineError::Unsaved));
    assert_eq!(
        LineError::Unsaved.to_string(),
        "Unsaved changes: add ! to drop them"
    );
}

#[test]
fn a_command_with_no_path_refuses_when_the_document_has_none() {
    let reload = LineCommand::Edit {
        path: None,
        force: true,
    };
    let write_quit = LineCommand::WriteQuit {
        path: None,
        only_unsaved: false,
    };
    let exit = LineCommand::WriteQuit {
        path: None,
        only_unsaved: true,
    };

    assert_eq!(reload.refusal(false, false), Some(LineError::NoFileName));
    assert_eq!(
        write_quit.refusal(false, false),
        Some(LineError::NoFileName)
    );
    assert_eq!(exit.refusal(true, false), Some(LineError::NoFileName));
    assert_eq!(exit.refusal(false, false), None);
    assert_eq!(LineCommand::Write(None).refusal(true, false), None);
}

#[test]
fn a_typed_path_resolves_from_home_or_from_the_document_folder() {
    let home = Path::new("/home/writer");
    let folder = Path::new("/home/writer/notes");

    assert_eq!(
        resolve_path("~/a.md", Some(home), Some(folder)),
        PathBuf::from("/home/writer/a.md")
    );
    assert_eq!(
        resolve_path("/tmp/a.md", Some(home), Some(folder)),
        PathBuf::from("/tmp/a.md")
    );
    assert_eq!(
        resolve_path("b.md", Some(home), Some(folder)),
        PathBuf::from("/home/writer/notes/b.md")
    );
    assert_eq!(
        resolve_path("b.md", Some(home), None),
        PathBuf::from("/home/writer/b.md")
    );
}

/// The line after `keys`, with `|` at the caret.
fn line_after(keys: &[LineKey]) -> String {
    let mut line = CommandLine::default();
    for &key in keys {
        line.key(key);
    }
    let (before, after) = line.text().split_at(line.caret());
    format!("{before}|{after}")
}

fn typed(text: &str) -> Vec<LineKey> {
    text.chars().map(LineKey::Char).collect()
}

fn then(mut keys: Vec<LineKey>, more: &[LineKey]) -> Vec<LineKey> {
    keys.extend_from_slice(more);
    keys
}

#[test]
fn left_and_right_move_the_caret_one_char_and_typing_goes_in_at_the_caret() {
    assert_eq!(line_after(&typed("wq")), "wq|");
    assert_eq!(line_after(&then(typed("wq"), &[LineKey::Left])), "w|q");
    assert_eq!(
        line_after(&then(typed("wq"), &[LineKey::Left, LineKey::Char('x')])),
        "wx|q"
    );
    assert_eq!(
        line_after(&then(
            typed("wq"),
            &[LineKey::Left, LineKey::Left, LineKey::Left, LineKey::Right]
        )),
        "w|q"
    );
    assert_eq!(line_after(&then(typed("wq"), &[LineKey::Right])), "wq|");
}

#[test]
fn left_moves_over_one_whole_char_of_more_bytes() {
    assert_eq!(line_after(&then(typed("e αβ"), &[LineKey::Left])), "e α|β");
}

#[test]
fn home_and_end_go_to_the_start_and_the_end() {
    assert_eq!(
        line_after(&then(typed("wq"), &[LineKey::Home, LineKey::Char('x')])),
        "x|wq"
    );
    assert_eq!(
        line_after(&then(typed("wq"), &[LineKey::Home, LineKey::End])),
        "wq|"
    );
}

#[test]
fn backspace_deletes_the_char_before_the_caret() {
    assert_eq!(
        line_after(&then(typed("wxq"), &[LineKey::Left, LineKey::Backspace])),
        "w|q"
    );
    assert_eq!(
        line_after(&then(typed("wq"), &[LineKey::Home, LineKey::Backspace])),
        "|wq"
    );
}

#[test]
fn delete_word_deletes_the_blanks_and_the_word_before_the_caret() {
    assert_eq!(
        line_after(&then(typed("set novim"), &[LineKey::DeleteWord])),
        "set |"
    );
    assert_eq!(
        line_after(&then(typed("set  "), &[LineKey::DeleteWord])),
        "|"
    );
    assert_eq!(
        line_after(&then(typed("e notes/draft.md"), &[LineKey::DeleteWord])),
        "e notes/draft.|"
    );
    assert_eq!(
        line_after(&then(
            typed("w one two"),
            &[
                LineKey::Left,
                LineKey::Left,
                LineKey::Left,
                LineKey::Left,
                LineKey::DeleteWord
            ]
        )),
        "w | two"
    );
}

#[test]
fn delete_to_start_deletes_the_text_before_the_caret() {
    assert_eq!(
        line_after(&then(
            typed("w one two"),
            &[
                LineKey::Left,
                LineKey::Left,
                LineKey::Left,
                LineKey::Left,
                LineKey::DeleteToStart
            ]
        )),
        "| two"
    );
}

#[test]
fn up_and_tab_put_the_caret_at_the_line_end() {
    let mut keys = then(typed("chrome"), &[LineKey::Enter]);
    keys.extend(typed("ab"));
    keys.extend([LineKey::Home, LineKey::Up]);
    assert_eq!(line_after(&keys), "chrome|");
    assert_eq!(
        line_after(&then(typed("setti"), &[LineKey::Home, LineKey::Tab])),
        "settings|"
    );
}
