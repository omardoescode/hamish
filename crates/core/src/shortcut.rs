//! Shortcut labels: a key binding as the Shortcuts sheet and the tooltips show it.

/// The label of `keys`, a key binding in GPUI keystroke syntax (`"ctrl-shift-o"`): each
/// modifier and the key with a capital, joined by "+", and "escape" as "Esc". The keystrokes
/// of a sequence keep their space.
///
/// ```
/// assert_eq!(hamish_core::key_label("ctrl-shift-o escape"), "Ctrl+Shift+O Esc");
/// ```
#[must_use]
pub fn key_label(keys: &str) -> String {
    keys.split(' ')
        .map(keystroke_label)
        .collect::<Vec<_>>()
        .join(" ")
}

fn keystroke_label(keystroke: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = keystroke;
    while let Some((modifier, after)) = rest.split_once('-') {
        if modifier.is_empty() || after.is_empty() {
            break;
        }
        parts.push(capitalized(modifier));
        rest = after;
    }
    parts.push(if rest == "escape" {
        "Esc".to_owned()
    } else {
        capitalized(rest)
    });
    parts.join("+")
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
