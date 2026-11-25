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

/// Words that start with a consonant letter and a vowel sound (silent "h"),
/// or with "un" + vowel letter and a vowel sound, by prefix.
const VOWEL_SOUND: [&str; 7] = ["hour", "honest", "honor", "honour", "heir", "unin", "unim"];

/// Words that start with a vowel letter and a consonant sound, by prefix.
const CONSONANT_SOUND: [&str; 7] = ["uni", "use", "usu", "uti", "eu", "ewe", "once"];

/// Words that start with a vowel letter and a consonant sound, whole.
const CONSONANT_SOUND_WORDS: [&str; 1] = ["one"];

fn takes_an(word: &str) -> Option<bool> {
    let first = word.chars().next().filter(|c| c.is_alphabetic())?;
    let lower = word.to_lowercase();
    let word_end = lower
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(lower.len());
    let first_word = lower.get(..word_end).unwrap_or_default();
    if VOWEL_SOUND.iter().any(|prefix| lower.starts_with(prefix)) {
        return Some(true);
    }
    let consonant_sound = CONSONANT_SOUND
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || CONSONANT_SOUND_WORDS.contains(&first_word);
    Some(!consonant_sound && "aeiouAEIOU".contains(first))
}
