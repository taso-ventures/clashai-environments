# Word list provenance

This directory ships two 5-letter English word lists used by the Wordle environment:

| File | Size | Purpose |
|------|------|---------|
| `answers.txt` | 2,309 words | The "answer set" each player's hidden target is drawn from. |
| `valid_guesses.txt` | 10,639 words | The extended set of guesses the engine accepts as well-formed. |

Both lists are publicly-distributed open word lists used widely across the Wordle community and are loaded at compile time via `include_str!` from `crates/wordle-protocol/src/word_list.rs`. They are included in this repository on a fair-use / public-distribution basis: the lists are compilations of common English five-letter words, and individual five-letter words are not copyrightable.

## Replacing the lists

If you would prefer stricter license clarity for a fork or downstream distribution, swap either or both files for one of the public-domain or permissively-licensed alternatives below. The schema is one lowercase word per line, sorted, with no blank lines or comments.

- **SCOWL** — Spell Checker Oriented Word Lists, public domain / MIT / BSD depending on subset. <http://wordlist.aspell.net/>
- **ENABLE** — Enhanced North American Benchmark Lexicon, public domain. Often shipped as `enable1.txt`.
- **dwyl/english-words** — Unlicense. <https://github.com/dwyl/english-words>

After swapping, run `cargo test -p wordle-protocol` — a few engine tests assert against specific words from the current set and may need their fixtures updated.

## Trademarks

"Wordle" is a trademark of The New York Times. This repository ships a multi-player variant inspired by the guess-feedback mechanic; it does not use the Wordle name as a product identifier and ships no NYT assets, art, or branded text. See [`CONTRIBUTORS.md`](../../../CONTRIBUTORS.md) for the full credit note.
