# Heads-Up No-Limit Texas Hold'em (HU NLHE)

## Overview
You are playing Heads-Up (2-player) No-Limit Texas Hold'em poker. The match consists of up to 100 hands. Stacks reset to 200 chips each hand. The player with the highest cumulative profit at the end of the match wins.

## Setup
- **Players:** 2
- **Starting Stack:** 200 chips per hand
- **Blinds:** Small Blind = 1, Big Blind = 2
- **Button:** Alternates each hand

## Dealing
1. Each player receives 2 private hole cards
2. 5 community cards are dealt across betting rounds:
   - **Flop:** 3 cards
   - **Turn:** 1 card
   - **River:** 1 card

## Betting Rounds
1. **Preflop:** After hole cards are dealt. In heads-up, the button (Small Blind) acts first.
2. **Flop:** After 3 community cards. Big Blind acts first.
3. **Turn:** After 4th community card. Big Blind acts first.
4. **River:** After 5th community card. Big Blind acts first.

## Actions
- **Fold:** Surrender the hand, forfeiting any chips already in the pot
- **Check:** Pass action (only when no bet is facing you)
- **Call:** Match the current bet
- **Raise:** Increase the bet. The raise amount is specified as the total street bet.
  - Minimum raise increment = the larger of (the last raise increment) or (the big blind)
  - Maximum raise = all-in (your remaining stack)

## Hand Rankings (Highest to Lowest)
1. **Straight Flush** — Five consecutive cards of the same suit
2. **Four of a Kind** — Four cards of the same rank
3. **Full House** — Three of a kind plus a pair
4. **Flush** — Five cards of the same suit
5. **Straight** — Five consecutive cards (A-2-3-4-5 is the lowest straight)
6. **Three of a Kind** — Three cards of the same rank
7. **Two Pair** — Two different pairs
8. **One Pair** — Two cards of the same rank
9. **High Card** — Highest card when no other hand is made

## Winning
- Best 5-card hand wins from 7 available cards (2 hole + 5 community)
- If hands are tied, the pot is split evenly
- If a player folds, the other player wins the pot

## Match Scoring
- Each hand's profit/loss is tracked cumulatively
- After all hands are played, the player with the higher total profit wins the match
- If profits are equal, the match is a draw

## Response Format
Your action must be one of:
- `{"action_type": "fold"}`
- `{"action_type": "check"}`
- `{"action_type": "call"}`
- `{"action_type": "raise", "amount": <total_street_bet>}`
