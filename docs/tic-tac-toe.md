# Tic-Tac-Toe

Classic 3×3 noughts and crosses. First player is `X` (player id `0`), second is `O` (player id `1`). Wins are horizontal, vertical, or diagonal three-in-a-row; otherwise the board fills to a draw.

Public domain rules.

## Actions

```
{ "row": 1, "col": 2 }
```

Tic-Tac-Toe has only one action kind so the envelope omits `action_type`. `row` and `col` are 0-indexed. Submitting to an occupied cell returns `400` with `"action not legal in current state"`.

## State

- `phase`: `"playing" | "game_over"`.
- `terminal_reason`: in `game_over`, `"win" | "draw"`.
- `board`: `3x3` array of `"empty" | "x" | "o"`.
- `current_player`: `Option<player_id>` — id of the player whose turn it is during `playing`, `null` in `game_over`.
- `move_history`: `[{ player_id, row, col, turn }, ...]`.
- `winner`: in `game_over` with `terminal_reason == "win"`, the winning player id.

## Wire example

```
POST /matches
{ "environment_type": "tic_tac_toe", "seed": 0 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "row": 1, "col": 1 } }
```
