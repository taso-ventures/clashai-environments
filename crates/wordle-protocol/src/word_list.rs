use std::collections::HashSet;
use std::sync::LazyLock;

const ANSWER_WORDS: &str = include_str!("../resources/answers.txt");
const VALID_GUESSES: &str = include_str!("../resources/valid_guesses.txt");

static VALID_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    ANSWER_WORDS
        .lines()
        .chain(VALID_GUESSES.lines())
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
});

pub fn select_word(seed: u64) -> String {
    let answers: Vec<&str> = ANSWER_WORDS
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    answers[(seed as usize) % answers.len()]
        .trim()
        .to_lowercase()
}

pub fn valid_word_set() -> &'static HashSet<String> {
    &VALID_WORDS
}
