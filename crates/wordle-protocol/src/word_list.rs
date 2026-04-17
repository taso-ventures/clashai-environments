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
    select_word_at(seed, 0)
}

/// Pick a deterministic answer word for the given seed and slot index.
///
/// The slot index lets callers derive multiple non-identical targets from a
/// single match seed (for per-player targets). The seed is mixed with the
/// slot via a splitmix-style step so two consecutive slots do not map to
/// consecutive answer-list entries.
pub fn select_word_at(seed: u64, slot: usize) -> String {
    let answers: Vec<&str> = ANSWER_WORDS
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let mut x = seed.wrapping_add(0x9E37_79B9_7F4A_7C15u64.wrapping_mul(slot as u64));
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    answers[(x as usize) % answers.len()].trim().to_lowercase()
}

pub fn valid_word_set() -> &'static HashSet<String> {
    &VALID_WORDS
}
