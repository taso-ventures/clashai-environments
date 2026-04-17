# Poker (Heads-Up No-Limit Texas Hold'em)

2-player No-Limit Texas Hold'em. Each hand: blinds post (SB=1, BB=2), two hole cards each, four betting rounds (preflop, flop, turn, river) with community cards revealed between. Best 5-card hand from 7 cards wins, or the last player who didn't fold takes the pot. Match runs `MAX_HANDS = 100` hands or until one player is eliminated.

Public domain rules.

## Constants

- `INITIAL_STACK = 200`
- `SMALL_BLIND = 1`
- `BIG_BLIND = 2`
- `MAX_HANDS = 100`
- `NUM_PLAYERS = 2`

## Actions

```
{ "action_type": "fold" }
{ "action_type": "check" }
{ "action_type": "call" }
{ "action_type": "raise", "amount": 12 }
```

`amount` for `raise` is the total street bet (not the increment). Sub-minimum raises and raises exceeding stack are rejected with `400`. Canonical source: `PokerAction` in `crates/poker-protocol/src/lib.rs`.

## State

- `hand_number`: 1-based, up to `MAX_HANDS`.
- `phase`: `"preflop" | "flop" | "turn" | "river" | "showdown" | "hand_over" | "match_over"`.
- `community_cards`: `[]`, 3, 4, or 5 cards revealed by phase.
- `pot`: current pot size.
- `stacks`: `{ "0": 200, "1": 200 }`.
- `current_player`: id of the player to act.
- `legal_raises`: `{ min, max }` when `raise` is legal.
- `hole_cards` (private): your two cards. Opponent cards hidden until showdown.
- `action_history`: per-hand log.

Card shapes:
```json
{ "rank": "ace", "suit": "spades" }
```

## Wire example

```
POST /matches
{ "environment_type": "poker", "player_count": 2, "seed": 42 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "action_type": "raise", "amount": 6 } }
```
