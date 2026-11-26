//! Article agreement: "a"/"an" before a Word Slot agrees with its Active variant's first sound.

/// The article that agrees with `word`, in the case of `article`.
/// `None` when `article` is not "a"/"an" or `word` does not start with a letter.
pub(crate) fn agreeing(article: &str, word: &str) -> Option<&'static str> {
    let an = takes_an(article, word)?;
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
const CONSONANT_SOUND: [&str; 8] = ["uni", "unesco", "use", "usu", "uti", "eu", "ewe", "once"];

/// Words that start with a vowel letter and a consonant sound, whole.
const CONSONANT_SOUND_WORDS: [&str; 1] = ["one"];

/// Capital letters whose names start with a vowel sound ("eff", "aitch", "ex").
const VOWEL_SOUND_LETTERS: &str = "AEFHILMNORSX";

/// An all-capital word of two or more letters reads as letter names ("an FBI"), unless the
/// article is all capitals too: then the text is in capitals. An acronym that reads as a word
/// ("a NASA") takes the article of a word; one that starts with "U" reads "you" ("a UEFA").
fn takes_an(article: &str, word: &str) -> Option<bool> {
    let first = word.chars().next().filter(|c| c.is_alphabetic())?;
    let letters = word
        .get(
            ..word
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(word.len()),
        )
        .unwrap_or_default();
    let initialism = article != "AN"
        && letters.chars().count() >= 2
        && letters.chars().all(char::is_uppercase)
        && !reads_as_word(letters);
    if initialism {
        return Some(VOWEL_SOUND_LETTERS.contains(first));
    }
    if article != "AN" && first == 'U' && letters.chars().all(char::is_uppercase) {
        return Some(false);
    }
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

/// An all-capital acronym reads as a word when it has four or more letters, a vowel after
/// its first letter, and no run of three or more consonants ("NASA", not "HTML" or "NSFW").
fn reads_as_word(letters: &str) -> bool {
    let vowel = |c: char| "AEIOU".contains(c);
    let longest_consonant_run = letters
        .split(vowel)
        .map(|run| run.chars().count())
        .max()
        .unwrap_or_default();
    letters.chars().count() >= 4 && letters.chars().skip(1).any(vowel) && longest_consonant_run < 3
}
