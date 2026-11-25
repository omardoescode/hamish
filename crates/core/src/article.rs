//! Article agreement: "a"/"an" before a Word Slot agrees with its Active variant's first sound.

/// The article that agrees with `word`, in the case of `article`.
/// `None` when `article` is not "a"/"an" or `word` does not start with a letter.
pub(crate) fn agreeing(article: &str, word: &str) -> Option<&'static str> {
    let an = takes_an(word)?;
    match (article, an) {
        ("a" | "an", false) => Some("a"),
        ("a" | "an", true) => Some("an"),
        ("A" | "An" | "AN", false) => Some("A"),
        ("A" | "An", true) => Some("An"),
        ("AN", true) => Some("AN"),
        _ => None,
    }
}

fn takes_an(word: &str) -> Option<bool> {
    let first = word.chars().next().filter(|c| c.is_alphabetic())?;
    Some("aeiou".contains(first))
}
