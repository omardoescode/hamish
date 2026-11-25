use hamish_core::key_label;

#[test]
fn a_key_label_names_each_modifier_and_the_key_with_a_capital() {
    assert_eq!(key_label("ctrl-shift-o"), "Ctrl+Shift+O");
    assert_eq!(key_label("ctrl-,"), "Ctrl+,");
    assert_eq!(key_label("ctrl-/"), "Ctrl+/");
    assert_eq!(key_label("ctrl-shift-backspace"), "Ctrl+Shift+Backspace");
    assert_eq!(key_label("alt-cmd-a"), "Alt+Cmd+A");
}

#[test]
fn a_key_label_shortens_escape_and_keeps_a_minus_key() {
    assert_eq!(key_label("escape"), "Esc");
    assert_eq!(key_label("ctrl--"), "Ctrl+-");
    assert_eq!(key_label("-"), "-");
}

#[test]
fn a_key_label_of_a_sequence_puts_a_space_between_its_keys() {
    assert_eq!(key_label("ctrl-k ctrl-s"), "Ctrl+K Ctrl+S");
}
