use hamish_core::{
    Caret, Document, Level, PanelCommand, PanelKeys, TrimLevel, Vim, VimAction, VimCommand, VimKey,
    VimPanel,
};

fn vim_key(key: char) -> VimKey {
    match key {
        '\u{1b}' => VimKey::Escape,
        '\n' => VimKey::Enter,
        '\u{8}' => VimKey::Backspace,
        key => VimKey::Char(key),
    }
}

/// The commands of `keys` in `panel`, one per key.
fn panel_commands(keys: &mut PanelKeys, panel: VimPanel, typed: &str) -> Vec<PanelCommand> {
    typed
        .chars()
        .map(|key| keys.key(panel, vim_key(key)))
        .collect()
}

#[test]
fn in_the_alternatives_panel_j_k_move_rows_and_enter_takes_the_row() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Alternatives, "jk\n");

    assert_eq!(
        commands,
        vec![PanelCommand::Down, PanelCommand::Up, PanelCommand::Enter]
    );
}

#[test]
fn in_the_alternatives_panel_dd_deletes_the_variant_and_digits_choose_the_level() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Alternatives, "dd123dx");

    assert_eq!(
        commands,
        vec![
            PanelCommand::Nothing,
            PanelCommand::DeleteRow,
            PanelCommand::ChooseLevel(Level::Word),
            PanelCommand::ChooseLevel(Level::Sentence),
            PanelCommand::ChooseLevel(Level::Paragraph),
            PanelCommand::Nothing,
            PanelCommand::Nothing,
        ]
    );
}

#[test]
fn in_the_alternatives_panel_i_types_in_the_row_until_escape() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Alternatives, "ijk\u{8}\u{1b}j\u{1b}");

    assert_eq!(
        commands,
        vec![
            PanelCommand::Insert,
            PanelCommand::PassOn,
            PanelCommand::PassOn,
            PanelCommand::PassOn,
            PanelCommand::EndInsert,
            PanelCommand::Down,
            PanelCommand::Close,
        ]
    );
    assert!(!keys.inserting());
}

#[test]
fn in_the_alternatives_panel_rows_take_no_text_and_no_backspace() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Alternatives, "x\u{8}");

    assert_eq!(commands, vec![PanelCommand::Nothing, PanelCommand::Nothing]);
    assert_eq!(
        keys.key(VimPanel::Alternatives, VimKey::Ctrl('z')),
        PanelCommand::PassOn
    );
}

#[test]
fn in_the_lab_j_k_move_between_checks_and_enter_runs_one() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Lab, "jk\nhi\u{1b}");

    assert_eq!(
        commands,
        vec![
            PanelCommand::Down,
            PanelCommand::Up,
            PanelCommand::Enter,
            PanelCommand::Nothing,
            PanelCommand::Nothing,
            PanelCommand::Close,
        ]
    );
}

#[test]
fn in_settings_j_k_move_fields_and_h_l_change_a_value() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::Settings, "jkhl\nx");

    assert_eq!(
        commands,
        vec![
            PanelCommand::Down,
            PanelCommand::Up,
            PanelCommand::Left,
            PanelCommand::Right,
            PanelCommand::Enter,
            PanelCommand::Nothing,
        ]
    );
}

#[test]
fn in_an_open_settings_select_j_k_move_rows_and_other_keys_search() {
    let mut keys = PanelKeys::default();

    let commands = panel_commands(&mut keys, VimPanel::SettingsSelect, "jkhx\n");

    assert_eq!(
        commands,
        vec![
            PanelCommand::Down,
            PanelCommand::Up,
            PanelCommand::PassOn,
            PanelCommand::PassOn,
            PanelCommand::Enter,
        ]
    );
}

/// The commands of `keys` typed from `offset` in `document`. A Select moves the caret.
fn commands(document: &Document, offset: usize, keys: &[VimKey]) -> Vec<VimCommand> {
    let mut vim = Vim::default();
    let mut caret = Caret::at(offset);
    keys.iter()
        .map(|key| {
            let command = vim.key(*key, document, &caret);
            if let VimCommand::Select(next) = &command {
                caret = next.clone();
            }
            command
        })
        .collect()
}

#[test]
fn ctrl_w_h_and_ctrl_w_l_move_between_the_editor_and_overflow() {
    let document = Document::new("one two");
    let keys = [
        VimKey::Ctrl('w'),
        VimKey::Char('l'),
        VimKey::Ctrl('w'),
        VimKey::Char('h'),
        VimKey::Ctrl('w'),
        VimKey::Char('x'),
        VimKey::Char('l'),
    ];

    assert_eq!(
        commands(&document, 0, &keys),
        vec![
            VimCommand::Nothing,
            VimCommand::Run(VimAction::FocusOverflow),
            VimCommand::Nothing,
            VimCommand::Run(VimAction::FocusEditor),
            VimCommand::Nothing,
            VimCommand::Nothing,
            VimCommand::Select(Caret::at(1)),
        ]
    );
}

#[test]
fn space_p_puts_the_overflow_block_and_p_stays_the_register_put() {
    let document = Document::new("one two");

    let leader = commands(&document, 0, &[VimKey::Char(' '), VimKey::Char('p')]);
    let put = commands(&document, 0, &[VimKey::Char('p')]);

    assert_eq!(
        leader.last(),
        Some(&VimCommand::Run(VimAction::UseOverflowBlock))
    );
    assert!(matches!(put.last(), Some(VimCommand::Put(_))));
}

/// "one two three four five" with Trim cuts over "two" (4..7) and "four" (14..18).
fn trimmed() -> Document {
    let mut document = Document::new("one two three four five");
    let request = document.trim_request(TrimLevel::SlightTrim);
    let applied = request.map(|request| document.apply_trim_result(&request, &[4..7, 14..18]));
    assert!(matches!(applied, Some(Ok(()))), "the Trim proposal opens");
    document
}

#[test]
fn with_a_trim_proposal_n_and_capital_n_walk_the_cuts() {
    let document = trimmed();
    let keys = [
        VimKey::Char('n'),
        VimKey::Char('n'),
        VimKey::Char('n'),
        VimKey::Char('N'),
        VimKey::Char('2'),
        VimKey::Char('N'),
    ];

    assert_eq!(
        commands(&document, 0, &keys),
        vec![
            VimCommand::Select(Caret::at(4)),
            VimCommand::Select(Caret::at(14)),
            VimCommand::Nothing,
            VimCommand::Select(Caret::at(4)),
            VimCommand::Nothing,
            VimCommand::Nothing,
        ]
    );
}

#[test]
fn with_a_trim_proposal_space_k_keeps_the_cut_enter_makes_the_cuts_and_escape_closes() {
    let document = trimmed();

    let keys = [
        VimKey::Char(' '),
        VimKey::Char('k'),
        VimKey::Enter,
        VimKey::Escape,
    ];

    assert_eq!(
        commands(&document, 4, &keys),
        vec![
            VimCommand::Nothing,
            VimCommand::Run(VimAction::KeepTrimCut),
            VimCommand::Run(VimAction::MakeTrimCuts),
            VimCommand::Run(VimAction::CloseTrim),
        ]
    );
}

#[test]
fn with_a_trim_proposal_k_stays_the_up_motion() {
    let mut document = Document::new("one\ntwo three");
    let request = document.trim_request(TrimLevel::SlightTrim);
    let applied = request.map(|request| document.apply_trim_result(&request, &[0..3, 8..13]));
    assert!(matches!(applied, Some(Ok(()))), "the Trim proposal opens");

    assert_eq!(
        commands(&document, 4, &[VimKey::Char('k')]),
        vec![VimCommand::Select(Caret::at(0))]
    );
}

#[test]
fn with_no_trim_proposal_k_and_enter_move_and_escape_does_nothing() {
    let document = Document::new("one\ntwo");

    let keys = [VimKey::Enter, VimKey::Char('k'), VimKey::Escape];

    assert_eq!(
        commands(&document, 0, &keys),
        vec![
            VimCommand::Select(Caret::at(4)),
            VimCommand::Select(Caret::at(0)),
            VimCommand::Nothing,
        ]
    );
}

#[test]
fn escape_with_a_count_typed_only_clears_it_with_a_trim_proposal() {
    let document = trimmed();

    let keys = [VimKey::Char('3'), VimKey::Escape, VimKey::Escape];

    assert_eq!(
        commands(&document, 0, &keys),
        vec![
            VimCommand::Nothing,
            VimCommand::Nothing,
            VimCommand::Run(VimAction::CloseTrim),
        ]
    );
}
