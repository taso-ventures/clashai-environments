# Vibe Check

A team party game inspired by Wavelength (Wolfgang Warsch, Alex Hague, Justin Vickers). Two teams take turns; the active team's Psychic sees a hidden target position on a spectrum (e.g., "cold ←→ hot") and gives a one-word clue. Teammates guess where the target sits. The opposing team guesses whether the active team's pointer landed left or right of the true target.

Canonical rules: <https://boardgamegeek.com/boardgame/262543/wavelength>. Clean-room implementation; no copyrighted card text verbatim.

## Actions

```
# Psychic (active team)
{ "action_type": "give_clue", "clue": "lukewarm" }

# Guessers (active team)
{ "action_type": "submit_guess", "position": 0.62 }

# Opposing team
{ "action_type": "submit_steal_guess", "direction": "left" }

# Orchestrator-only (timeout fallback)
{ "action_type": "forfeit" }
```

Canonical source: `VibeCheckAction` in `crates/vibe-check-protocol/src/lib.rs`.

## State

- `round`: 1-based round counter.
- `phase`: `"clue"`, `"guess"`, `"steal"`, `"scoring"`, or `"game_over"`.
- `teams`: array of `{ team_id, score, members: [player_id], ... }`.
- `active_team`: team whose Psychic is clue-giving.
- `current_card`: `{ left: "cold", right: "hot", category: "temperature" }`.
- `target_position` (private to active Psychic until scoring): `f64` in `[0.0, 1.0]`.
- `current_clue` (public after `give_clue`).
- `current_guess` (public after `guess_position`).
- `scoring_zone`: only in `scoring` phase — `"bullseye" | "near" | "far" | "miss"`.

## Wire example

```
POST /matches
{ "environment_type": "vibe_check", "player_count": 4, "seed": 7 }

POST /matches/:id/actions
{ "player_id": 2, "action": { "action_type": "give_clue", "clue": "lukewarm" } }
```

## Spectrum cards

The engine ships a built-in deck of ~40 spectrum pairs in `crates/vibe-check-engine/resources/spectrum_cards.json`. Contribute more via PR — any pair of antonyms plus an optional category works.
