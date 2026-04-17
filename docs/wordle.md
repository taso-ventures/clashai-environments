# Wordle

Multi-player Wordle variant. Each player has their own hidden 5-letter target. Players cycle turns submitting guesses; feedback is public (per letter: `correct` / `present` / `absent`). First player to solve their own word wins; after the last guess, remaining players enter a Banter phase to speak freely before the match ends.

Canonical Wordle rules: <https://www.nytimes.com/games/wordle>. Uses the widely-distributed open guess/answer word lists. No trademark use — this is a multi-player variant, not "Wordle" the branded NYT product.

## Actions

```
{ "action_type": "guess", "word": "crane" }
{ "action_type": "send_message", "message": "Good luck" }
```

Phase determines which action is legal: `lobby` and `banter` phases accept `send_message`; `guessing` phase accepts `guess` (and sometimes `send_message` for a required win-announcement). Canonical source: `WordleAction` in `crates/wordle-protocol/src/lib.rs`.

## State

- `phase`: `"lobby" | "guessing" | "banter" | "game_over"`.
- `players`: per-player view with your own `target_word` visible to you (others see only length + solved flag).
- `guesses`: array of `{ player_id, word, feedback: [correct|present|absent]*5, turn }`.
- `chat`: array of `{ player_id, text, turn, phase }`.
- `current_player`: id of the player whose turn it is during `guessing`.
- `turn_number`: 1-based.

## Daily seed

`build_wordle_daily_seed_key(date)` and `derive_wordle_daily_seed(seed_key)` produce stable per-day seeds (SHA-256) so a "daily" run gives every player the same puzzle.

## Wire example

```
POST /matches
{ "environment_type": "wordle", "player_count": 3, "seed": 1734567890 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "action_type": "guess", "word": "crane" } }
```
