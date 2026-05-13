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

`VibeCheckState`:

- `round`: 1-based round counter.
- `phase` (`TurnPhase`): tagged enum with variants
  - `"clue_phase"` with `active_team`, `cluegiver`
  - `"guess_phase"` with `active_team`, `cluegiver`, `clue`, `pending_guesses`
  - `"steal_phase"` with `active_team`, `stealing_team`, `clue`, `active_guess`, `pending_steals`
  - `"resolving"` / `"game_over"`
- `teams`: `Vec<TeamState>` — `team_id`, `score`, `player_ids`.
- `players`: `Vec<PlayerInfo>` — team assignments.
- `spectrum`: current `SpectrumCard` (`left_label`, `right_label`, `category`) — visible to all.
- `target`: `Option<Target>` (`position` in `[0.0, 1.0]`). Visible only to the active-team Psychic during `clue_phase`/`guess_phase`/`steal_phase`; hidden from all other players until `resolving` — then public.
- `zone_config`: cumulative outer radii (`bullseye_half_width` < `near_half_width` < `far_half_width`). See struct docs in `vibe-check-protocol` for the exact semantics.
- `target_score`, `cluegiver_rotation`, `round_history`, `is_game_over`.

Fog-of-war: `state_for_player` exposes `target` only to the active Psychic. `pending_guesses` / `pending_steals` are redacted for everyone so individual submissions don't leak before the phase resolves.

## Wire example

```
POST /matches
{ "environment_type": "vibe_check", "player_count": 4, "seed": 7 }

POST /matches/:id/actions
{ "player_id": 2, "action": { "action_type": "give_clue", "clue": "lukewarm" } }
```

## Spectrum cards

The engine ships a built-in deck of ~40 spectrum pairs in `crates/vibe-check-engine/resources/spectrum_cards.json`. Contribute more via PR — any pair of antonyms plus an optional category works.
