use hamish_core::{
    AlternativesCommand, AlternativesKeys, Caret, Direction, Document, LabCommand, Level,
    RowCommand, SettingsCommand, TrimLevel, Vim, VimAction, VimCommand, VimKey,
};

fn vim_key(key: char) -> VimKey {
    match key {
        '\u{1b}' => VimKey::Escape,
        '\n' => VimKey::Enter,
        '\u{8}' => VimKey::Backspace,
        key => VimKey::Char(key),
    }
}

/// The commands of `typed` in the Alternatives panel, one per key.
fn alternatives_commands(keys: &mut AlternativesKeys, typed: &str) -> Vec<AlternativesCommand> {
    typed.chars().map(|key| keys.key(vim_key(key))).collect()
}

/// The command of each key of `typed`, from `of`.
fn each<C>(typed: &str, of: fn(VimKey) -> C) -> Vec<C> {
    typed.chars().map(|key| of(vim_key(key))).collect()
}

#[test]
fn in_the_alternatives_panel_j_k_move_rows_and_enter_takes_the_row() {
    let mut keys = AlternativesKeys::default();

    let commands = alternatives_commands(&mut keys, "jk\n");

    assert_eq!(
        commands,
        vec![
            AlternativesCommand::Row(RowCommand::Down),
            AlternativesCommand::Row(RowCommand::Up),
            AlternativesCommand::Row(RowCommand::Enter),
        ]
    );
}

#[test]
fn in_the_alternatives_panel_dd_deletes_the_variant_and_digits_choose_the_level() {
    let mut keys = AlternativesKeys::default();

    let commands = alternatives_commands(&mut keys, "dd123dx");

    assert_eq!(
        commands,
        vec![
            AlternativesCommand::Nothing,
            AlternativesCommand::DeleteRow,
            AlternativesCommand::ChooseLevel(Level::Word),
            AlternativesCommand::ChooseLevel(Level::Sentence),
            AlternativesCommand::ChooseLevel(Level::Paragraph),
            AlternativesCommand::Nothing,
            AlternativesCommand::Nothing,
        ]
    );
}

#[test]
fn in_the_alternatives_panel_i_types_in_the_row_until_escape() {
    let mut keys = AlternativesKeys::default();

    let commands = alternatives_commands(&mut keys, "ijk\u{8}\u{1b}j\u{1b}");

    assert_eq!(
        commands,
        vec![
            AlternativesCommand::Insert,
            AlternativesCommand::PassOn,
            AlternativesCommand::PassOn,
            AlternativesCommand::PassOn,
            AlternativesCommand::EndInsert,
            AlternativesCommand::Row(RowCommand::Down),
            AlternativesCommand::Row(RowCommand::Close),
        ]
    );
    assert!(!keys.inserting());
}

#[test]
fn in_the_alternatives_panel_rows_take_no_text_and_no_backspace() {
    let mut keys = AlternativesKeys::default();

    let commands = alternatives_commands(&mut keys, "x\u{8}");

    assert_eq!(
        commands,
        vec![AlternativesCommand::Nothing, AlternativesCommand::Nothing]
    );
    assert_eq!(keys.key(VimKey::Ctrl('z')), AlternativesCommand::PassOn);
}

#[test]
fn in_the_lab_j_k_move_between_checks_and_enter_runs_one() {
    let commands = each("jk\nhi\u{1b}\u{8}", LabCommand::of);

    assert_eq!(
        commands,
        vec![
            LabCommand::Row(RowCommand::Down),
            LabCommand::Row(RowCommand::Up),
            LabCommand::Row(RowCommand::Enter),
            LabCommand::Nothing,
            LabCommand::Nothing,
            LabCommand::Row(RowCommand::Close),
            LabCommand::PassOn,
        ]
    );
}

#[test]
fn in_settings_j_k_move_fields_and_h_l_change_a_value() {
    let commands = each("jkhl\nx\u{8}", SettingsCommand::of);

    assert_eq!(
        commands,
        vec![
            SettingsCommand::Row(RowCommand::Down),
            SettingsCommand::Row(RowCommand::Up),
            SettingsCommand::Step(Direction::Back),
            SettingsCommand::Step(Direction::Forward),
            SettingsCommand::Row(RowCommand::Enter),
            SettingsCommand::Nothing,
            SettingsCommand::PassOn,
        ]
    );
}

#[test]
fn in_an_open_settings_select_j_k_move_rows_and_other_keys_search() {
    let commands = each("jkhx\n", RowCommand::in_select);

    assert_eq!(
        commands,
        vec![
            Some(RowCommand::Down),
            Some(RowCommand::Up),
            None,
            None,
            None
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
