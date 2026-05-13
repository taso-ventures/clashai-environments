# Coup - Game Rules

## Overview

Coup is a bluffing and deduction card game for 2-6 players. Each player starts with 2 influence cards (face-down) and 2 coins. The goal is to be the last player with influence remaining.

## Components

- **5 Roles** (3 copies each = 15 cards total): Duke, Assassin, Captain, Ambassador, Contessa
- **Coins**: Treasury of coins shared by all players

## Setup

Each player receives 2 face-down influence cards and 2 coins. The remaining cards form the Court deck.

## Turn Structure

On your turn, you MUST take exactly one action. You cannot pass your turn.

### Actions

| Action | Cost | Effect | Role Claim |
|--------|------|--------|------------|
| **Income** | Free | Take 1 coin from treasury | None |
| **Foreign Aid** | Free | Take 2 coins from treasury | None (blockable) |
| **Coup** | 7 coins | Target player loses 1 influence | None (unblockable) |
| **Tax** | Free | Take 3 coins from treasury | Duke |
| **Assassinate** | 3 coins | Target player loses 1 influence | Assassin |
| **Steal** | Free | Take 2 coins from target player | Captain |
| **Exchange** | Free | Draw 2 cards from deck, choose which to keep | Ambassador |

**Mandatory Coup**: If you have 10+ coins at the start of your turn, you MUST coup.

## Challenges

When a player claims a role for an action (Tax, Assassinate, Steal, Exchange), any other player may challenge.

- **Challenge succeeds** (claimer does NOT have the role): The claimer loses 1 influence. The action is cancelled.
- **Challenge fails** (claimer DOES have the role): The challenger loses 1 influence. The claimer reveals the role card, shuffles it into the deck, and draws a replacement. The action proceeds.

## Blocks

Some actions can be blocked by claiming to have a specific role:

| Action | Can Be Blocked By |
|--------|-------------------|
| **Foreign Aid** | Duke |
| **Assassinate** | Contessa |
| **Steal** | Captain or Ambassador |

Blocks can themselves be challenged. If the block-challenge succeeds (blocker lied), the original action proceeds. If the block-challenge fails (blocker has the role), the action is cancelled and the challenger loses influence.

## Losing Influence

When you lose influence, you choose one of your face-down cards to reveal. Once both cards are revealed, you are eliminated.

## Bluffing

You do NOT need to have a role to claim it. Any player can claim any action or block. The risk is being challenged.

## Game End

The game ends when only one player has influence remaining. That player wins.

## Turn Phases

Each action may pass through several phases:

1. **AwaitingAction** - Active player chooses an action
2. **ChallengeWindow** - Other players may challenge a role claim
3. **BlockWindow** - Eligible players may block the action
4. **BlockChallengeWindow** - Players may challenge a block
5. **RevealingCard** - A player must reveal a card to prove a role
6. **SelectingCardToLose** - A player must choose a card to lose
7. **ExchangeSelection** - Ambassador player selects which cards to keep
8. **GameOver** - One player remains

## Strategy Tips

- **Bluff selectively**: Claim roles that benefit you, but don't overcommit.
- **Track eliminations**: As cards are revealed, the probability of players holding specific roles changes.
- **Challenge wisely**: A failed challenge costs you influence.
- **Coins matter**: Building toward coup gives you a guaranteed elimination, but costs are high.
- **Income is safe**: It cannot be challenged or blocked.
