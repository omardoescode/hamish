use std::ops::Range;

use hamish_core::{
    AiAdapter, AiError, AiRequest, CancelToken, Caret, Cycle, Document, EditSession, LabCheck,
    LabFinding, LabRequest, Level, Provenance, TrimLevel, TrimRequest, Vim, VimAction, VimCommand,
    VimKey, VimMode,
};

/// The command of the last key of `keys`, typed from `offset` in `document`. A Select
/// moves the caret; no other command changes the Document.
fn last_command(document: &Document, offset: usize, keys: &str) -> (VimCommand, Vim) {
    let mut vim = Vim::default();
    let mut caret = Caret::at(offset);
    let mut command = VimCommand::Nothing;
    for key in keys.chars() {
        command = vim.key(vim_key(key), document, &caret);
        if let VimCommand::Select(next) = &command {
            caret = next.clone();
        }
    }
    (command, vim)
}

fn command(document: &Document, offset: usize, keys: &str) -> VimCommand {
    last_command(document, offset, keys).0
}

fn vim_key(key: char) -> VimKey {
    match key {
        '\u{1b}' => VimKey::Escape,
        key => VimKey::Char(key),
    }
}

/// Ghost, Revive or Stash on a Document; `false` when refused.
type Verb = fn(&mut Document, Range<usize>) -> bool;

/// Runs `keys` on `session` as the Editor does: Ghost, Revive and Stash change the
/// Document as one Undo step, and the caret goes to the start of their range.
fn run_session(session: &mut EditSession, vim: &mut Vim, keys: &str) {
    for key in keys.chars() {
        let command = vim.key(vim_key(key), session.document(), session.caret());
        let (range, verb): (Range<usize>, Verb) = match command {
            VimCommand::Select(caret) => {
                session.select(caret);
                continue;
            }
            VimCommand::Edit(edit) | VimCommand::Cut { edit, .. } => {
                session.edit(&edit);
                continue;
            }
            VimCommand::Ghost(range) => (range, |document, range| document.ghost(range).is_ok()),
            VimCommand::Revive(range) => (range, |document, range| document.revive(range).is_ok()),
            VimCommand::Stash(range) => (range, |document, range| document.stash(range).is_ok()),
            VimCommand::Undo(_) => {
                session.undo();
                continue;
            }
            VimCommand::PassOn
            | VimCommand::Nothing
            | VimCommand::Yank { .. }
            | VimCommand::Put(_)
            | VimCommand::Redo(_)
            | VimCommand::CycleSlot { .. }
            | VimCommand::Run(_) => continue,
        };
        let start = range.start;
        let done = session.change(|document| verb(document, range).then_some(()).ok_or(()));
        assert!(done.is_ok(), "the verb changes the Document");
        session.select(Caret::at(start));
    }
}

fn session(text: &str) -> (EditSession, Vim) {
    (EditSession::new(Document::new(text)), Vim::default())
}

#[test]
fn gh_ghosts_over_a_motion_or_a_text_object() {
    let document = Document::new("one two three");
    assert_eq!(command(&document, 0, "ghw"), VimCommand::Ghost(0..4));
    assert_eq!(command(&document, 5, "ghiw"), VimCommand::Ghost(4..7));
    assert_eq!(command(&document, 0, "2gh2w"), VimCommand::Ghost(0..13));
    assert_eq!(command(&document, 0, "gh"), VimCommand::Nothing);
}

#[test]
fn capital_gh_revives_and_gs_stashes_over_a_motion() {
    let document = Document::new("one two three");
    assert_eq!(command(&document, 4, "gHe"), VimCommand::Revive(4..7));
    assert_eq!(command(&document, 4, "gs$"), VimCommand::Stash(4..13));
}

#[test]
fn a_doubled_verb_acts_on_whole_lines() {
    let document = Document::new("one\ntwo\nthree");
    assert_eq!(command(&document, 5, "ghgh"), VimCommand::Ghost(4..7));
    assert_eq!(command(&document, 0, "2gHgH"), VimCommand::Revive(0..7));
    assert_eq!(command(&document, 5, "gsgs"), VimCommand::Stash(4..8));
}

#[test]
fn in_visual_mode_ghost_revive_and_stash_act_on_the_selection() {
    let document = Document::new("one two three");
    let (ghost, vim) = last_command(&document, 4, "vegh");
    assert_eq!(ghost, VimCommand::Ghost(4..7));
    assert_eq!(vim.mode(), VimMode::Normal);
    assert_eq!(command(&document, 4, "vegH"), VimCommand::Revive(4..7));
    assert_eq!(command(&document, 0, "Vgs"), VimCommand::Stash(0..13));
}

#[test]
fn ghost_is_one_undo_step_and_dot_repeats_it() {
    let (mut session, mut vim) = session("one two three");
    run_session(&mut session, &mut vim, "ghiww.");
    assert_eq!(session.document().ghost_ranges(), vec![0..3, 4..7]);
    assert_eq!(session.caret().offset(), 4);

    run_session(&mut session, &mut vim, "u");
    assert_eq!(session.document().ghost_ranges(), vec![0..3]);
}

#[test]
fn revive_returns_the_ghost_range_and_stash_moves_text_to_overflow() {
    let (mut session, mut vim) = session("one two three");
    run_session(&mut session, &mut vim, "ghegHe");
    assert_eq!(
        session.document().ghost_ranges(),
        Vec::<Range<usize>>::new()
    );

    run_session(&mut session, &mut vim, "wgsiw");
    assert_eq!(session.document().text(), "one  three");
    assert_eq!(session.document().overflow().text(), "two");
}

/// "one two three" with a Word Slot over "one" and one over "three".
fn two_slots() -> Document {
    let mut document = Document::new("one two three");
    for (range, variant) in [(0..3, "1"), (8..13, "3")] {
        assert!(
            document
                .create_slot(range, Level::Word, variant, Provenance::Writer)
                .is_ok()
        );
    }
    document
}

#[test]
fn bracket_v_cycles_the_slot_at_the_caret_with_a_count() {
    let document = two_slots();
    assert_eq!(
        command(&document, 1, "]v"),
        VimCommand::CycleSlot {
            cycle: Cycle::Next,
            times: 1
        }
    );
    assert_eq!(
        command(&document, 1, "3[v"),
        VimCommand::CycleSlot {
            cycle: Cycle::Previous,
            times: 3
        }
    );
}

#[test]
fn a_slot_cycle_is_no_change_that_dot_repeats() {
    let document = two_slots();
    assert_eq!(command(&document, 1, "]v."), VimCommand::Nothing);
}

#[test]
fn bracket_s_jumps_between_slot_starts_with_a_count() {
    let document = two_slots();
    assert_eq!(
        command(&document, 5, "]s"),
        VimCommand::Select(Caret::at(8))
    );
    assert_eq!(
        command(&document, 5, "[s"),
        VimCommand::Select(Caret::at(0))
    );
    assert_eq!(
        command(&document, 0, "2]s"),
        VimCommand::Select(Caret::at(8))
    );
    assert_eq!(
        command(&document, 0, "5]s"),
        VimCommand::Select(Caret::at(8))
    );
    assert_eq!(command(&document, 9, "]s"), VimCommand::Nothing);
}

/// Answers each Lab check with the same findings, as an AI backend would.
struct FakeAdapter(Vec<LabFinding>);

impl AiAdapter for FakeAdapter {
    fn variants(&self, _: &AiRequest, _: &CancelToken) -> Result<Vec<String>, AiError> {
        Ok(Vec::new())
    }

    fn lab_check(&self, _: &LabRequest, _: &CancelToken) -> Result<Vec<LabFinding>, AiError> {
        Ok(self.0.clone())
    }

    fn trim(&self, _: &TrimRequest, _: &CancelToken) -> Result<Vec<Range<usize>>, AiError> {
        Ok(Vec::new())
    }
}

/// "one two three four" with Lab marks over "two" and "four".
fn two_marks() -> Document {
    let mut document = Document::new("one two three four");
    let findings = [4..7, 14..18].map(|range| LabFinding {
        range,
        note: None,
        fix: None,
    });
    let request = document.lab_request(LabCheck::HedgesFiller);
    let found = FakeAdapter(findings.to_vec()).lab_check(&request, &CancelToken::default());
    assert!(found.is_ok_and(|found| document.apply_lab_result(&request, &found).is_ok()));
    document
}

#[test]
fn bracket_m_jumps_between_lab_marks_with_a_count() {
    let document = two_marks();
    assert_eq!(
        command(&document, 0, "]m"),
        VimCommand::Select(Caret::at(4))
    );
    assert_eq!(
        command(&document, 0, "2]m"),
        VimCommand::Select(Caret::at(14))
    );
    assert_eq!(
        command(&document, 17, "[m"),
        VimCommand::Select(Caret::at(14))
    );
    assert_eq!(
        command(&document, 17, "2[m"),
        VimCommand::Select(Caret::at(4))
    );
}

#[test]
fn in_visual_mode_bracket_s_and_m_move_the_selection_end() {
    let document = two_marks();
    assert_eq!(
        command(&document, 0, "v]m"),
        VimCommand::Select(Caret::over(0..5))
    );
}

#[test]
fn space_is_the_leader_of_the_hamish_actions() {
    let document = Document::new("one two");
    for (keys, action) in [
        (" a", VimAction::OpenAlternatives),
        (" l", VimAction::ToggleLab),
        (" y", VimAction::AcceptLabMark),
        (" x", VimAction::DismissLabMark),
        (" k", VimAction::KeepTrimCut),
    ] {
        assert_eq!(
            command(&document, 0, keys),
            VimCommand::Run(action),
            "{keys}"
        );
    }
    assert_eq!(command(&document, 0, " q"), VimCommand::Nothing);
}

#[test]
fn space_t_runs_a_trim_and_a_count_picks_the_trim_level() {
    let document = Document::new("one two");
    let trim = |keys| command(&document, 0, keys);
    assert_eq!(
        trim(" t"),
        VimCommand::Run(VimAction::RunTrim(TrimLevel::SlightTrim))
    );
    assert_eq!(
        trim("2 t"),
        VimCommand::Run(VimAction::RunTrim(TrimLevel::TightenMore))
    );
    assert_eq!(
        trim("3 t"),
        VimCommand::Run(VimAction::RunTrim(TrimLevel::EvenSharper))
    );
    assert_eq!(
        trim("9 t"),
        VimCommand::Run(VimAction::RunTrim(TrimLevel::CutInHalf))
    );
}

#[test]
fn the_status_shows_the_leader_as_space() {
    let document = Document::new("one");
    let (_, vim) = last_command(&document, 0, "2 ");
    assert_eq!(vim.status(), "NORMAL 2\u{2423}");
}

#[test]
fn gv_selects_the_last_visual_area_again() {
    let document = Document::new("one two three");
    let (reselected, vim) = last_command(&document, 4, "vl\u{1b}0gv");
    assert_eq!(reselected, VimCommand::Select(Caret::over(4..6)));
    assert_eq!(vim.mode(), VimMode::Visual);
    assert_eq!(
        last_command(&document, 4, "Vj\u{1b}gv").1.mode(),
        VimMode::VisualLine
    );
    assert_eq!(command(&document, 0, "gv"), VimCommand::Nothing);
}
