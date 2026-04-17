# Connect Four

7-column × 6-row grid. Players alternate dropping discs into a column; the disc falls to the lowest empty row in that column (gravity). First to align four of their colour horizontally, vertically, or diagonally wins. Full board with no winner is a draw.

Public domain rules.

## Actions

```
{ "column": 3 }
```

Connect Four has only one action kind so the envelope omits `action_type`. `column` is 0-indexed, range `[0, 6]`. A full column or out-of-range index returns `400`.

## State

- `phase`: `"playing" | "game_over"`.
- `terminal_reason`: `"win" | "draw"` in `game_over`.
- `board`: 6×7 array of `"empty" | "blue" | "orange"`, row 0 is the top.
- `current_player`: `Option<player_id>` — id of the player whose turn it is during `playing`, `null` in `game_over`.
- `move_history`: `[{ player_id, column, turn }, ...]`.
- `winner`: winning player id (if any).

Render an ASCII view of the board with the `board_to_ascii()` helper in `connect-four-protocol`.

## Wire example

```
POST /matches
{ "environment_type": "connect_four", "seed": 0 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "column": 3 } }
```
