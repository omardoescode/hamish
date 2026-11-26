//! The Vim mode rows of the Shortcuts sheet.

/// The keys of Vim mode as the Shortcuts sheet lists them: what each does, then its keys.
/// `Space` is the default leader; [`vim_keys`] shows the leader in use.
pub const VIM_KEYS: [(&str, &str); 46] = [
    ("Move left, down, up, right", "h j k l"),
    ("Next word start, word start before, word end", "w b e"),
    ("Line start, line end", "0 $"),
    ("First line, last line (a count: that line)", "g g  G"),
    ("First non-blank of the next line", "Enter"),
    ("A count repeats a motion", "3 w"),
    ("Insert mode before, after the caret", "i a"),
    ("Insert mode on a new line below, above", "o O"),
    (
        "Delete, change, yank over a motion or a text object",
        "d c y",
    ),
    ("Delete, change, yank whole lines", "d d  c c  y y"),
    (
        "Delete the char, to the line end; change to the line end",
        "x D C",
    ),
    ("Text objects: word, sentence, paragraph", "i w  a s  i p"),
    (
        "Text objects: Active variant, Ghost range (a: with the blank around)",
        "i v  a v  i g  a g",
    ),
    ("Put the clipboard after, before the caret", "p P"),
    ("Repeat the last change", "."),
    ("Undo, redo", "u  Ctrl+R"),
    ("Visual mode, Visual line mode", "v V"),
    ("Select the last Visual area again", "g v"),
    (
        "Visual mode: the head goes to a click; the nearer end goes to a shift-click",
        "Click  Shift+Click",
    ),
    (
        "Ghost, Revive, Stash over a motion, a text object or the Visual selection",
        "g h  g H  g s",
    ),
    (
        "Ghost, Revive, Stash whole lines",
        "g h g h  g H g H  g s g s",
    ),
    (
        "Cycle the Slot at the caret (a count: more steps)",
        "] v  [ v",
    ),
    ("Next Slot, previous Slot", "] s  [ s"),
    ("Next Lab mark, previous Lab mark", "] m  [ m"),
    (
        "Alternatives panel, Lab, Trim (a count 1 to 4: the Trim level)",
        "Space a  Space l  Space t",
    ),
    (
        "Accept, dismiss the Lab mark at the caret; keep the Trim cut",
        "Space y  Space x  Space k",
    ),
    ("Put the Overflow block at the caret", "Space p"),
    (
        "Editor, Overflow (Overflow has the Vim keys too)",
        "Ctrl+W h  Ctrl+W l",
    ),
    ("Trim: next cut, cut before (a count: more cuts)", "n  N"),
    (
        "Trim: keep the cut at the caret, make the cuts, close the Trim bar",
        "Space k  Enter  Esc",
    ),
    (
        "Alternatives panel: rows, make the row the Active variant",
        "j k  Enter",
    ),
    (
        "Alternatives panel: delete the Variant; Word, Sentence, Paragraph",
        "d d  1 2 3",
    ),
    (
        "Alternatives panel: type in the row, back to the rows",
        "i  Esc",
    ),
    ("Lab: checks, run the check, close", "j k  Enter  Esc"),
    (
        "Settings: fields, change a value, open a select (in it: rows; letters search)",
        "j k  h l  Enter",
    ),
    (
        "Command line: save, save to a path; close, close and drop changes",
        ":w  :w path  :q  :q!",
    ),
    (
        "Command line: save and close; save if changed and close",
        ":wq  :x",
    ),
    (
        "Command line: open a file, the file again; drop changes and open",
        ":e path  :e  :e! path",
    ),
    (
        "Command line: Vim mode off, on; Chrome bar off or on; Settings",
        ":set novim  :set vim  :chrome  :settings",
    ),
    ("Command line: read the hamishrc again", ":source"),
    (
        "Command line: caret; start, end; delete the word, delete to the start",
        "Left Right  Home End  Ctrl+B Ctrl+E  Ctrl+W  Ctrl+U",
    ),
    (
        "Command line: complete, recent commands, close",
        "Tab  Up Down  Esc",
    ),
    (
        "Command line: run; delete the char before the caret (on an empty line: close)",
        "Enter  Backspace",
    ),
    ("Back to Normal mode", "Esc"),
    ("Text edits in Insert mode", "Ctrl keys as usual"),
    ("The register is the clipboard", "y p"),
];

/// The rows of [`VIM_KEYS`] with `leader` in place of each `Space` key.
#[must_use]
pub fn vim_keys(leader: char) -> Vec<(&'static str, String)> {
    let leader = if leader == ' ' {
        "Space".to_owned()
    } else {
        leader.to_string()
    };
    VIM_KEYS
        .iter()
        .map(|&(label, keys)| {
            let keys: Vec<&str> = keys
                .split(' ')
                .map(|key| if key == "Space" { leader.as_str() } else { key })
                .collect();
            (label, keys.join(" "))
        })
        .collect()
}
