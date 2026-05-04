# Wordle

Multi-player Wordle variant. Each player has their own hidden 5-letter target. Players cycle turns submitting guesses; feedback is public (per letter: `correct` / `present` / `absent`). First player to solve their own word wins; after the last guess, remaining players enter a Banter phase to speak freely before the match ends.

Canonical Wordle rules: <https://www.nytimes.com/games/wordle>. Uses publicly-distributed open guess/answer word lists; see [`crates/wordle-protocol/resources/PROVENANCE.md`](../crates/wordle-protocol/resources/PROVENANCE.md) for sourcing notes and how to swap them. No trademark use — this is a multi-player variant, not "Wordle" the branded NYT product.

## Actions

```
{ "action_type": "guess", "word": "crane" }
{ "action_type": "send_message", "message": "Good luck" }
```

Phase determines which action is legal:
- `lobby`: accepts `send_message` (free-form chat, capped per player by `max_messages_per_chat_phase`) and `guess`. The first guess by any player transitions to `guessing`, so silent players do not block the match.
- `guessing`: accepts `guess` from active players and `send_message` from a player who just solved (a one-shot "win" announcement).
- `banter`: accepts `send_message` only. The phase auto-advances to `game_over` once the total banter message budget is exhausted (`max_messages_per_chat_phase × player_count`).
- `game_over`: no legal actions.

Canonical source: `WordleAction` in `crates/wordle-protocol/src/lib.rs`.

## State

`WordleFullState` (server / observer view):
- `turn`: 1-based, 0 during `lobby`.
- `phase`: `"lobby" | "guessing" | "banter" | "game_over"`.
- `players`: `Vec<PlayerProgress>` — each entry has `player_id`, `display_name`, `target_word`, `guesses`, `solved`, `eliminated`, `solved_turn`.
- `chat_messages`: `Vec<ChatMessage>` with `player_id`, `player_name`, `text`, `turn`, `timestamp_ms`, `phase` (`"lobby"|"win"|"banter"`).
- `is_terminal`, `terminal_reason` (`"all_solved_or_eliminated" | "max_guesses_exhausted"`), `solve_order`.

`WordlePlayerView` (player-filtered, via `state_for_player`):
- `my_progress`: full `PlayerProgress` including your own `target_word`.
- `opponents`: `Vec<OpponentSummary>` — no target word leaks; only `guess_count`, `solved`, `eliminated`.
- `chat_messages`: same log, but win/banter messages have each player's target redacted.
- `revealed_target_word`: mirrors your own target (always present for the owner).
- `needs_guess_this_turn`, `is_terminal`, `max_guesses`.

## Daily seed

`derive_wordle_daily_seed(date_utc, slot_index)` produces a stable per-day, per-slot seed (SHA-256). `build_wordle_daily_seed_key(date_utc, slot_index)` returns the canonical seed key string for logging. Each player's target is drawn deterministically from the match seed plus their slot index via `word_list::select_word_at`.

## Wire example

```
POST /matches
{ "environment_type": "wordle", "player_count": 3, "seed": 1734567890 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "action_type": "guess", "word": "crane" } }
```
