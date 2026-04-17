# Coup

A 2–6 player bluffing and deduction card game by Rikki Tahta. Each player starts with two influence cards representing roles with different actions. Players bluff, challenge, and block until one remains.

Canonical rules: <https://en.wikipedia.org/wiki/The_Resistance:_Coup>. The engine in this repo is a clean-room implementation — no game art, no copyrighted card text.

## Roles

`duke` · `assassin` · `captain` · `ambassador` · `contessa`

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

The state object includes:

- `phase`: `"play"`, `"reactive"` (waiting on challenge/block/pass), `"resolving"`, or `"game_over"`.
- `players`: array of `PlayerPublicInfo` with visible cards (revealed), coins, and alive flag.
- `current_player`: id of the player whose turn it is during `play` phase.
- `pending_action`: in `reactive`/`resolving`, the action under consideration, its actor, target, and claimed role.
- `action_history`: append-only log of completed actions for context.
- Player view includes their own two `cards` with role; opponents' cards are hidden until revealed.

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
