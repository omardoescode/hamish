//! The Vim mode rows of the Shortcuts sheet.

/// The keys of Vim mode as the Shortcuts sheet lists them: what each does, then its keys.
pub const VIM_KEYS: [(&str, &str); 28] = [
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
    ("Text objects: Active variant, Ghost range", "i v  i g"),
    ("Put the clipboard after, before the caret", "p P"),
    ("Repeat the last change", "."),
    ("Undo, redo", "u  Ctrl+R"),
    ("Visual mode, Visual line mode", "v V"),
    ("Select the last Visual area again", "g v"),
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
    ("Back to Normal mode", "Esc"),
    ("Text edits in Insert mode", "Ctrl keys as usual"),
    ("The register is the clipboard", "y p"),
];
