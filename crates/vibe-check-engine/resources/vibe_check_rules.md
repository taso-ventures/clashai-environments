# Vibe Check — Game Rules

## Overview
Vibe Check is a team-based word-association guessing game inspired by Wavelength. Two teams compete to score points by estimating where a hidden target lies on a spectrum between two opposing concepts.

## Setup
- Players are split into two teams (Team A and Team B).
- Team A goes first. Team B starts with 1 point to compensate for the second-mover disadvantage.
- A target score is set (default: 10 points). First team to reach it wins.

## Round Flow
1. **Spectrum card drawn** — Two opposing endpoints are revealed (e.g., "Hot" vs "Cold"), along with an optional category that constrains what clues can reference.
2. **Target set** — A random position on the spectrum [0.0, 1.0] is hidden from everyone except the Cluegiver.
3. **Clue phase** — The active team's Cluegiver gives exactly one clue (a word or short phrase, max 5 words).
4. **Guess phase** — The active team submits a guess position (0.0–1.0).
5. **Steal phase** — The opposing team guesses whether the target is to the LEFT or RIGHT of the active team's guess.
6. **Reveal & score** — The target is revealed, scoring zones are computed, and points are awarded.
7. **Turn passes** to the other team (with an exception — see Extra Turn below).

## Scoring Zones
Zones are centered on the target position:
- **Bullseye** (4 points): Within the innermost zone.
- **Near** (3 points): Adjacent to bullseye.
- **Far** (2 points): Outer edges of the scoring window.
- **Miss** (0 points): Outside all scoring zones.

## Steal Rules
- The steal team guesses whether the hidden target is to the **left** (lower value) or **right** (higher or equal value) of the **active team's guess position**.
- If the direction guess is correct, the steal team earns **1 point**.
- If wrong, the steal team earns **0 points**.
- **Bullseye negates steal**: If the active team scores a Bullseye, the steal team gets 0 points regardless of their direction guess.

## Extra Turn Rule
- If the active team scores a **Bullseye** (4 points) but is still **behind** the opposing team in total score, the active team gets an extra turn instead of alternating.
- In all other cases, the stealing team becomes the active team for the next round.

## Win Condition
- First team to reach the target score wins.
- If both teams cross the threshold in the same round, the active team's score resolves first.
- If both teams are tied after resolution, the game is a draw.

## Clue Constraints
- The Cluegiver must provide exactly one clue (word or short phrase, max 5 words, max 200 characters).
- The clue **must not** contain any digits or numbers.
- The clue **must not** contain any of the spectrum endpoint words (case-insensitive).
- The clue should not reference the spectrum position directly (e.g., no "halfway", "far left").
- If a category is specified, the clue should be an example from that category.
- Violation of length, word count, number, or endpoint constraints results in 0 points for the round.

## Position Values
- 0.0 = fully the left endpoint
- 1.0 = fully the right endpoint
- 0.5 = exactly between both endpoints
