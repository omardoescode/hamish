use hamish_core::{
    Caret, Fed, HAMISHRC_EXAMPLE, Hamishrc, HamishrcError, HamishrcProblem, MapMode, Vim,
    VimAction, VimCommand, VimKey,
};

const ACTIONS: [&str; 2] = ["Stash", "AcceptLabMark"];

fn parse(text: &str) -> Hamishrc {
    Hamishrc::parse(text, &ACTIONS)
}

/// The keys of `typed` (one char each) through the maps of `rc` in `mode`, until the feed
/// waits or ends.
fn fed(rc: &Hamishrc, mode: MapMode, typed: &str) -> Vec<Fed> {
    let mut vim = Vim::default();
    let start = match mode {
        MapMode::Normal => None,
        MapMode::Insert => Some('i'),
        MapMode::Visual => Some('v'),
    };
    if let Some(start) = start {
        vim.key(VimKey::Char(start), "one", &Caret::at(0));
    }
    let mut out = Vec::new();
    for typed in typed.chars() {
        vim.press(VimKey::Char(typed));
        while let Some(next) = vim.next_key(rc) {
            out.push(next);
        }
    }
    out
}

fn keys(typed: &str) -> Vec<Fed> {
    typed
        .chars()
        .map(|typed| Fed::Key(VimKey::Char(typed)))
        .collect()
}

#[test]
fn an_nnoremap_gives_its_keys_in_place_of_the_typed_keys() {
    let rc = parse("nnoremap Y y$\n");

    assert_eq!(rc.errors(), []);
    assert_eq!(fed(&rc, MapMode::Normal, "Yw"), keys("y$w"));
}

#[test]
fn an_nmap_maps_its_target_keys_again_and_an_nnoremap_does_not() {
    let recursive = parse("nmap a b\nnmap b x\n");
    let plain = parse("nnoremap a b\nnmap b x\n");

    assert_eq!(fed(&recursive, MapMode::Normal, "a"), keys("x"));
    assert_eq!(fed(&plain, MapMode::Normal, "a"), keys("b"));
}

#[test]
fn a_target_that_starts_with_the_keys_of_its_map_does_not_map_them_again() {
    let rc = parse("nmap x xd\n");

    assert_eq!(fed(&rc, MapMode::Normal, "x"), keys("xd"));
}

#[test]
fn two_maps_that_map_to_each_other_stop_after_1000_maps() {
    let rc = parse("nmap a b\nnmap b a\n");

    assert_eq!(
        fed(&rc, MapMode::Normal, "aw"),
        [Fed::TooDeep, Fed::Key(VimKey::Char('w'))]
    );
}

#[test]
fn keys_that_can_still_grow_into_a_longer_map_wait() {
    let rc = parse("nnoremap a X\nnnoremap ab Y\n");

    assert_eq!(fed(&rc, MapMode::Normal, "a"), []);
    assert_eq!(fed(&rc, MapMode::Normal, "ab"), keys("Y"));
    assert_eq!(fed(&rc, MapMode::Normal, "ac"), keys("Xc"));
}

#[test]
fn an_insert_map_waits_on_its_first_key_and_gives_it_back_when_the_next_key_differs() {
    let rc = parse("inoremap jj <Esc>\n");

    assert_eq!(fed(&rc, MapMode::Insert, "j"), []);
    assert_eq!(fed(&rc, MapMode::Insert, "jj"), [Fed::Key(VimKey::Escape)]);
    assert_eq!(fed(&rc, MapMode::Insert, "jk"), keys("jk"));
}

#[test]
fn a_map_applies_only_in_its_mode() {
    let rc = parse("nnoremap x y\nvnoremap x d\n");

    assert_eq!(fed(&rc, MapMode::Normal, "x"), keys("y"));
    assert_eq!(fed(&rc, MapMode::Visual, "x"), keys("d"));
    assert_eq!(fed(&rc, MapMode::Insert, "x"), keys("x"));
}

#[test]
fn a_normal_mode_map_does_not_apply_while_an_operator_waits_for_its_motion() {
    let rc = parse("nnoremap w b\n");
    let mut vim = Vim::default();
    let mut out = Vec::new();

    for typed in ['d', 'w'] {
        vim.press(VimKey::Char(typed));
        let next = vim.next_key(&rc);
        if let Some(Fed::Key(key)) = next {
            vim.key(key, "one two", &Caret::at(0));
        }
        out.extend(next);
    }

    assert_eq!(out, keys("dw"));
}

#[test]
fn the_leader_is_space_and_let_mapleader_changes_it_for_maps_and_the_hamish_keys() {
    let space = parse("nnoremap <leader>w dw\n");
    let comma = parse("let mapleader = \",\"\nnnoremap <Leader>w dw\n");

    assert_eq!(space.leader(), ' ');
    assert_eq!(fed(&space, MapMode::Normal, " w"), keys("dw"));
    assert_eq!(comma.leader(), ',');
    assert_eq!(fed(&comma, MapMode::Normal, ",w"), keys("dw"));

    let mut vim = Vim::default();
    let mut last = VimCommand::Nothing;
    for typed in [',', 'a'] {
        vim.press(VimKey::Char(typed));
        while let Some(Fed::Key(key)) = vim.next_key(&comma) {
            last = vim.key(key, "one", &Caret::at(0));
        }
    }
    assert_eq!(last, VimCommand::Run(VimAction::OpenAlternatives));
}

#[test]
fn a_leader_of_space_in_key_notation_reads() {
    let rc = parse("let mapleader = \"\\<Space>\"\n");

    assert_eq!(rc.errors(), []);
    assert_eq!(rc.leader(), ' ');
}

#[test]
fn an_action_name_after_a_colon_runs_that_action() {
    let rc = parse("nnoremap <leader>s :Stash\nnnoremap <leader>y :AcceptLabMark<CR>\n");

    assert_eq!(
        fed(&rc, MapMode::Normal, " s"),
        [Fed::Action("Stash".to_owned())]
    );
    assert_eq!(
        fed(&rc, MapMode::Normal, " y"),
        [Fed::Action("AcceptLabMark".to_owned())]
    );
}

#[test]
fn ctrl_keys_and_named_keys_read_in_key_notation() {
    let rc = parse("nnoremap <C-x> d<lt><Bar><CR>\n");
    let mut vim = Vim::default();
    vim.press(VimKey::Ctrl('x'));
    let mut out = Vec::new();
    while let Some(next) = vim.next_key(&rc) {
        out.push(next);
    }

    assert_eq!(
        out,
        [
            Fed::Key(VimKey::Char('d')),
            Fed::Key(VimKey::Char('<')),
            Fed::Key(VimKey::Char('|')),
            Fed::Key(VimKey::Enter),
        ]
    );
}

#[test]
fn an_unmap_removes_the_map_of_a_line_before_it() {
    let rc = parse("nnoremap Y y$\nnunmap Y\n");

    assert_eq!(rc.errors(), []);
    assert_eq!(fed(&rc, MapMode::Normal, "Y"), keys("Y"));
}

#[test]
fn each_bad_line_gives_an_error_with_its_line_number_and_the_good_lines_still_load() {
    let rc = parse(
        "\" keys\n\nxmap a b\nnnoremap Y y$\nnmap\nnmap Q\nnnoremap Z <Foo>\nnnoremap K :Nope\nlet mapleader = \",,\"\nlet foo = 1\nnunmap Q\n",
    );

    let error = |line, problem| HamishrcError { line, problem };
    assert_eq!(
        rc.errors(),
        [
            error(3, HamishrcProblem::UnknownCommand("xmap".to_owned())),
            error(5, HamishrcProblem::NoKeys),
            error(6, HamishrcProblem::NoTarget),
            error(7, HamishrcProblem::UnknownKey("<Foo>".to_owned())),
            error(8, HamishrcProblem::UnknownAction("Nope".to_owned())),
            error(9, HamishrcProblem::BadLeader("\",,\"".to_owned())),
            error(10, HamishrcProblem::UnknownVariable("foo".to_owned())),
            error(11, HamishrcProblem::NoMap("Q".to_owned())),
        ]
    );
    assert_eq!(fed(&rc, MapMode::Normal, "Y"), keys("y$"));
    assert_eq!(
        rc.errors()[3].to_string(),
        "hamishrc line 7: Not a key: <Foo>"
    );
}

#[test]
fn the_example_hamishrc_loads_with_no_error_and_maps_no_key() {
    let rc = parse(HAMISHRC_EXAMPLE);

    assert_eq!(rc.errors(), []);
    assert_eq!(rc.leader(), ' ');
    assert_eq!(fed(&rc, MapMode::Normal, "Y"), keys("Y"));
}
