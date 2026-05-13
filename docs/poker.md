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

The top-level state has two layers — match state and current-hand state.

### `MatchState` (admin / spectator full view)

- `hand_number`: 1-based, up to `MAX_HANDS`.
- `max_hands`: `100`.
- `phase` (`MatchPhase`): `"pre_match" | "playing" | "completed"`. While `playing`, `EnvironmentState::current_phase()` refines this to the current betting round (`"preflop" | "flop" | "turn" | "river"`) so agents see the street they're on.
- `profits`: `[i32; 2]` — cumulative profit across completed hands.
- `button`: which player has the button (SB in HU) for the current hand.
- `current_hand`: `Option<HandState>` — populated during `playing`.
- `hand_history`: `Vec<HandResult>` — finalized hands including winner, pot, and hole cards.

### `HandState` / `PlayerHandView`

- `community` / `community_cards`: `[]`, then 3 (flop), 4 (turn), 5 (river+).
- `round` (`BettingRound`): `"preflop" | "flop" | "turn" | "river"`.
- `stacks` / `your_stack` + `opponent_stack`: chips remaining this hand.
- `street_bets`, `pot_contributions`, `pot`: betting bookkeeping.
- `action_on`: player id to act next.
- `folded: [bool; 2]`, `finished: bool`.
- `action_history`: `[(player_id, PlayerAction), ...]`.

### `PlayerMatchView` fog-of-war

- `your_cards` visible; `opponent_stack` visible for position awareness, but opponent hole cards are hidden during the hand.
- `last_hand_result: Option<HandResult>` exposes the most recent finished hand including both players' hole cards at showdown, so agents can post-mortem.
- `your_stack` / `opponent_stack` reflect the tournament-style carryover across hands.

## Match termination

The match ends when **either** of:

- `hand_number > MAX_HANDS` (100 hands played), or
- a player's match-level stack hits `0` (bust).

Stacks carry across hands. Blinds post against the carried stack (short-blind posting is supported when a player has less than the blind left).

## All-in / side-pot handling

A player may call all-in for less than the outstanding bet. Uncalled chips from the raiser are refunded at hand end before profits are tallied, so neither player loses more than the smaller player's effective contribution on that hand.

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
