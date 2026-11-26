use hamish_core::{
    AlternativesCommand, AlternativesKeys, Caret, CommandLine, Document, LabCheck, LabCommand,
    LabFinding, Level, LineKey, LineStep, Provenance, SettingsCommand, TrimLevel, VIM_KEYS, Vim,
    VimCommand, VimKey,
};

/// Where the keys of a Shortcuts sheet row go.
#[derive(Clone, Copy)]
enum Place {
    /// Normal mode in [`marked`], from the caret at 8.
    Normal,
    /// Normal mode in [`trimmed`], from the caret at 8.
    Trim,
    /// Insert mode.
    Insert,
    /// Clicks in Visual mode.
    Click,
    Alternatives,
    Lab,
    Settings,
    /// The keys after the Command line opens.
    CommandLine,
    /// No keys: the row tells how other keys act.
    Prose,
}

/// One row of [`VIM_KEYS`]: where its keys go, and each command, as `(setup, keys)`. The
/// setup keys move the caret or start a mode first; the row does not list them.
type Row = (Place, &'static [(&'static str, &'static str)]);

const ROWS: [Row; 46] = [
    (Place::Normal, &[("", "h"), ("", "j"), ("", "k"), ("", "l")]),
    (Place::Normal, &[("", "w"), ("", "b"), ("", "e")]),
    (Place::Normal, &[("", "0"), ("", "$")]),
    (Place::Normal, &[("", "g g"), ("", "G")]),
    (Place::Normal, &[("", "Enter")]),
    (Place::Normal, &[("", "3 w")]),
    (Place::Normal, &[("", "i"), ("", "a")]),
    (Place::Normal, &[("", "o"), ("", "O")]),
    (Place::Normal, &[("", "d"), ("", "c"), ("", "y")]),
    (Place::Normal, &[("", "d d"), ("", "c c"), ("", "y y")]),
    (Place::Normal, &[("", "x"), ("", "D"), ("", "C")]),
    (Place::Normal, &[("d", "i w"), ("d", "a s"), ("d", "i p")]),
    (
        Place::Normal,
        &[("b d", "i v"), ("b d", "a v"), ("d", "i g"), ("d", "a g")],
    ),
    (Place::Normal, &[("", "p"), ("", "P")]),
    (Place::Normal, &[("x", ".")]),
    (Place::Normal, &[("", "u"), ("", "Ctrl+R")]),
    (Place::Normal, &[("", "v"), ("", "V")]),
    (Place::Normal, &[("v Esc", "g v")]),
    (Place::Click, &[("", "Click"), ("", "Shift+Click")]),
    (Place::Normal, &[("", "g h"), ("", "g H"), ("", "g s")]),
    (
        Place::Normal,
        &[("", "g h g h"), ("", "g H g H"), ("", "g s g s")],
    ),
    (Place::Normal, &[("", "] v"), ("", "[ v")]),
    (Place::Normal, &[("", "] s"), ("", "[ s")]),
    (Place::Normal, &[("", "] m"), ("", "[ m")]),
    (
        Place::Normal,
        &[("", "Space a"), ("", "Space l"), ("", "Space t")],
    ),
    (
        Place::Normal,
        &[("", "Space y"), ("", "Space x"), ("", "Space k")],
    ),
    (Place::Normal, &[("", "Space p")]),
    (Place::Normal, &[("", "Ctrl+W h"), ("", "Ctrl+W l")]),
    (Place::Trim, &[("", "n"), ("", "N")]),
    (Place::Trim, &[("", "Space k"), ("", "Enter"), ("", "Esc")]),
    (Place::Alternatives, &[("", "j"), ("", "k"), ("", "Enter")]),
    (
        Place::Alternatives,
        &[("", "d d"), ("", "1"), ("", "2"), ("", "3")],
    ),
    (Place::Alternatives, &[("", "i Esc")]),
    (
        Place::Lab,
        &[("", "j"), ("", "k"), ("", "Enter"), ("", "Esc")],
    ),
    (
        Place::Settings,
        &[("", "j"), ("", "k"), ("", "h"), ("", "l"), ("", "Enter")],
    ),
    (
        Place::CommandLine,
        &[("", ":w"), ("", ":w path"), ("", ":q"), ("", ":q!")],
    ),
    (Place::CommandLine, &[("", ":wq"), ("", ":x")]),
    (
        Place::CommandLine,
        &[("", ":e path"), ("", ":e"), ("", ":e! path")],
    ),
    (
        Place::CommandLine,
        &[
            ("", ":set novim"),
            ("", ":set vim"),
            ("", ":chrome"),
            ("", ":settings"),
        ],
    ),
    (Place::CommandLine, &[("", ":source")]),
    (
        Place::CommandLine,
        &[
            ("", "Left Right"),
            ("", "Home End"),
            ("", "Ctrl+B Ctrl+E"),
            ("", "Ctrl+W"),
            ("", "Ctrl+U"),
        ],
    ),
    (
        Place::CommandLine,
        &[("", "Tab"), ("", "Up Down"), ("", "Esc")],
    ),
    (Place::CommandLine, &[("", "Enter"), ("", "Backspace")]),
    (Place::Insert, &[("", "Esc")]),
    (Place::Prose, &[]),
    (Place::Normal, &[("", "y"), ("", "p")]),
];

fn vim_key(name: &str) -> VimKey {
    if let Some(key) = name.strip_prefix("Ctrl+") {
        return VimKey::Ctrl(one(&key.to_lowercase()));
    }
    match name {
        "Enter" => VimKey::Enter,
        "Esc" => VimKey::Escape,
        "Space" => VimKey::Char(' '),
        _ => VimKey::Char(one(name)),
    }
}

fn line_key(name: &str) -> LineKey {
    let key = match name {
        "Left" => Some(LineKey::Left),
        "Right" => Some(LineKey::Right),
        "Home" => Some(LineKey::Home),
        "End" => Some(LineKey::End),
        "Tab" => Some(LineKey::Tab),
        "Up" => Some(LineKey::Up),
        "Down" => Some(LineKey::Down),
        "Enter" => Some(LineKey::Enter),
        "Esc" => Some(LineKey::Escape),
        "Backspace" => Some(LineKey::Backspace),
        _ => name
            .strip_prefix("Ctrl+")
            .and_then(|key| LineKey::ctrl(&key.to_lowercase())),
    };
    assert!(key.is_some(), "the Command line has no key {name}");
    key.unwrap_or(LineKey::Escape)
}

fn one(name: &str) -> char {
    let key = hamish_core::one_char(name);
    assert!(key.is_some(), "no key is named {name}");
    key.unwrap_or_default()
}

/// "one two three four five six\nseven": Slots over "two" and "four", a Ghost range over
/// "three", Lab marks over "two" and "five".
fn marked() -> Document {
    let mut document = Document::new("one two three four five six\nseven");
    for (range, variant) in [(4..7, "2"), (14..18, "4")] {
        let created = document.create_slot(range, Level::Word, variant, Provenance::Writer);
        assert!(created.is_ok());
    }
    assert!(document.ghost(8..13).is_ok());
    let request = document.lab_request(LabCheck::HedgesFiller);
    let findings = [4..7, 19..23].map(|range| LabFinding {
        range,
        note: None,
        fix: None,
    });
    assert!(document.apply_lab_result(&request, &findings).is_ok());
    document
}

/// "one two three four five" with Trim cuts over "two" and "four".
fn trimmed() -> Document {
    let mut document = Document::new("one two three four five");
    let request = document.trim_request(TrimLevel::SlightTrim);
    let applied = request.map(|request| document.apply_trim_result(&request, &[4..7, 14..18]));
    assert!(matches!(applied, Some(Ok(()))));
    document
}

/// `setup`, then `keys`, in Normal mode from the caret at 8. Each key of `keys` is used, and
/// the last one completes a command, leaves one pending, or changes the mode.
fn assert_vim(document: &Document, setup: &str, keys: &str) {
    let mut vim = Vim::default();
    let mut caret = Caret::at(8);
    let mut press = |vim: &mut Vim, name: &str| {
        let command = vim.key(vim_key(name), document, &caret);
        if let VimCommand::Select(next) = &command {
            caret = next.clone();
        }
        command
    };
    for name in setup.split_whitespace() {
        press(&mut vim, name);
    }
    let mode = vim.mode();
    let mut last = VimCommand::Nothing;
    for name in keys.split_whitespace() {
        last = press(&mut vim, name);
        assert_ne!(
            last,
            VimCommand::PassOn,
            "{keys}: Vim mode passes {name} on"
        );
    }
    let pending = vim.status() != vim.mode().label();
    assert!(
        last != VimCommand::Nothing || pending || vim.mode() != mode,
        "{keys}: no command"
    );
}

fn assert_insert(keys: &str) {
    let mut vim = Vim::default();
    vim.key(VimKey::Char('i'), "one", &Caret::at(0));
    for name in keys.split_whitespace() {
        let command = vim.key(vim_key(name), "one", &Caret::at(0));
        assert!(
            !matches!(command, VimCommand::PassOn | VimCommand::Nothing),
            "{keys}: Insert mode does not use {name}"
        );
    }
}

fn assert_click(keys: &str) {
    let mut vim = Vim::default();
    vim.key(VimKey::Char('v'), "one two", &Caret::at(0));
    let extend = keys == "Shift+Click";
    assert!(
        vim.click("one two", 4, extend).is_some(),
        "{keys}: Visual mode takes no click"
    );
}

fn assert_alternatives(keys: &str) {
    let mut panel = AlternativesKeys::default();
    let mut last = AlternativesCommand::Nothing;
    for name in keys.split_whitespace() {
        last = panel.key(vim_key(name));
        assert_ne!(
            last,
            AlternativesCommand::PassOn,
            "{keys}: {name} passes on"
        );
    }
    assert_ne!(last, AlternativesCommand::Nothing, "{keys}: no command");
}

fn assert_line(keys: &str) {
    let mut line = CommandLine::default();
    let Some(command) = keys.strip_prefix(':') else {
        for name in keys.split_whitespace() {
            line.key(line_key(name));
        }
        return;
    };
    for typed in command.chars() {
        line.key(LineKey::Char(typed));
    }
    let step = line.key(LineKey::Enter);
    assert!(matches!(step, LineStep::Run(Ok(_))), "{keys}: {step:?}");
}

#[test]
fn each_vim_mode_row_of_the_shortcuts_sheet_lists_keys_that_vim_mode_uses() {
    let opened = Vim::default().key(VimKey::Char(':'), "one", &Caret::at(0));
    assert!(
        matches!(opened, VimCommand::Run(_)),
        ": opens no Command line"
    );
    let (marked, trimmed) = (marked(), trimmed());
    assert_eq!(VIM_KEYS.len(), ROWS.len());
    for ((label, listed), (place, commands)) in VIM_KEYS.iter().zip(ROWS) {
        let typed: Vec<&str> = commands
            .iter()
            .flat_map(|(_, keys)| keys.split_whitespace())
            .collect();
        if !matches!(place, Place::Prose) {
            let listed: Vec<&str> = listed.split_whitespace().collect();
            assert_eq!(listed, typed, "{label}");
        }
        for (setup, keys) in commands {
            match place {
                Place::Normal => assert_vim(&marked, setup, keys),
                Place::Trim => assert_vim(&trimmed, setup, keys),
                Place::Insert => assert_insert(keys),
                Place::Click => assert_click(keys),
                Place::Alternatives => assert_alternatives(keys),
                Place::Lab => assert!(
                    !matches!(
                        LabCommand::of(vim_key(keys)),
                        LabCommand::PassOn | LabCommand::Nothing
                    ),
                    "{keys}"
                ),
                Place::Settings => assert!(
                    !matches!(
                        SettingsCommand::of(vim_key(keys)),
                        SettingsCommand::PassOn | SettingsCommand::Nothing
                    ),
                    "{keys}"
                ),
                Place::CommandLine => assert_line(keys),
                Place::Prose => {}
            }
        }
    }
}
