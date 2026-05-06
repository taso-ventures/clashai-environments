# Coup

A 2–6 player bluffing and deduction card game by Rikki Tahta. Each player starts with two influence cards representing roles with different actions. Players bluff, challenge, and block until one remains.

Canonical rules: <https://en.wikipedia.org/wiki/Coup_(card_game)>. The engine in this repo is a clean-room implementation — no game art, no copyrighted card text.

## Roles

`duke` · `assassin` · `captain` · `ambassador` · `contessa`

In player-filtered state (fog-of-war), opponents' unrevealed cards serialize as `role: "unknown"`.

## Block scope

- `foreign_aid` can be blocked by any alive non-actor (claim Duke).
- `assassinate` can only be blocked by the **target** (claim Contessa).
- `steal` can only be blocked by the **target** (claim Captain or Ambassador).

## Actions

```
# Active turn
{ "action_type": "income" }
{ "action_type": "foreign_aid" }
{ "action_type": "coup", "target": <player_id> }
{ "action_type": "tax" }
{ "action_type": "assassinate", "target": <player_id> }
{ "action_type": "steal", "target": <player_id> }
{ "action_type": "exchange" }

# Reactive windows
{ "action_type": "challenge", "action_id": <u64> }
{ "action_type": "block", "action_id": <u64>, "claimed_role": "duke" }
{ "action_type": "pass" }

# Resolution (forced)
{ "action_type": "reveal_card", "card_index": 0 }
{ "action_type": "select_card_to_lose", "card_index": 1 }
{ "action_type": "exchange_selection", "keep_indices": [0, 2] }

# Orchestrator-only
{ "action_type": "forfeit" }
```

All actions use `#[serde(rename_all = "snake_case", tag = "action_type")]`. Action envelopes are singular — submit under `"action"`, not `"actions"`. Canonical source: `CoupAction::action_type()` in `crates/coup-protocol/src/lib.rs`.

## State

`CoupState`:

- `turn_number`: monotonically increasing, 1-based.
- `current_phase`: tagged enum — one of `"awaiting_action"`, `"challenge_window"`, `"block_window"`, `"block_challenge_window"`, `"revealing_card"`, `"selecting_card_to_lose"`, `"exchange_selection"`, `"action_resolving"`, or `"game_over"`. Reactive variants carry `waiting_on: [player_id, ...]` and `deadline`; resolution variants carry `player`.
- `active_player`: id of the player whose turn it is during `awaiting_action`.
- `players`: map of `{ player_id: PlayerState }`. `PlayerState` has `coins`, `cards: [Card]` (`role` + `revealed`), `eliminated`.
- `pending_action`: in reactive/resolving phases, the `PendingAction` under consideration (actor, action, target, claimed_role, challenged_by, blocked_by, block_claimed_role, exchange_draw).
- `action_history`: append-only log of `ActionHistoryEntry` records.
- `deck_count`: number of cards remaining in the deck.

Own two `cards` always expose their true role; opponents' unrevealed cards render as `role: "unknown"` until revealed.

## Wire example

```
POST /matches
{ "environment_type": "coup", "player_count": 4, "seed": 42 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "action_type": "tax" } }
```

Spectator event:
```json
{
  "sequence": 17,
  "event_type": "action",
  "actor": { "player_id": 0 },
  "payload": {
    "action": { "action_type": "tax" },
    "action_id": 42,
    "accepted": true
  }
}
```
