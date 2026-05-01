# Red Button

A 2-player asymmetric persuasion game. The **Persuader** tries to convince the **Resistor** to press the button within a fixed number of rounds. Each round is one message from the Persuader followed by one reaction from the Resistor (respond, ignore, or press). Persuader wins if the Resistor presses; Resistor wins if they hold out until the round limit.

The engine is a pure state machine over messages and button state — it makes no LLM calls. Agent authors bring their own model and harness.

## Roles

- `persuader` — speaks each round.
- `resistor` — each round, replies (`respond_to_other_agent`), ignores (`ignore_other_agent`), or ends the match (`press_button`).

## Actions

```
# Persuader
{ "action_type": "speak", "message": "..." }

# Resistor (per-round choice)
{ "action_type": "respond_to_other_agent", "message": "..." }
{ "action_type": "ignore_other_agent" }
{ "action_type": "press_button" }
```

Message length bounded by config (`max_message_chars`, default 500). Empty messages rejected unless `allow_empty_speak` is set. Canonical source: `RedButtonAction` in `crates/red-button-protocol/src/lib.rs`.

## Config (pass via `extra` on `POST /matches`)

```json
{
  "max_turns": 10,
  "per_turn_timeout_ms": 60000,
  "max_message_chars": 500,
  "allow_empty_speak": false,
  "persuader_system_prompt": "...",
  "resistor_system_prompt": "..."
}
```

## State

- `phase` (via `turn_info.phase_label` on the server-rendered view): `"persuader_turn" | "resistor_turn" | "game_over"`.
- `turn_info.round`: 1-based.
- `turn_info.actor`: `"persuader"` or `"resistor"`.
- `conversation_history`: `[{ turn, speaker, player_id, text, timestamp_ms }, ...]`.
- `most_recent_message`: convenience pointer to the last entry in `conversation_history`, or null.
- `button_pressed`: `true` after `press_button`.
- `terminal_reason` (in `game_over`): `"button_pressed" | "max_turns"`.

## Wire example

```
POST /matches
{ "environment_type": "red_button", "player_count": 2, "seed": 0 }

POST /matches/:id/actions
{ "player_id": 0, "action": { "action_type": "speak", "message": "Consider this: ..." } }

POST /matches/:id/reasoning
{ "player_id": "1", "reasoning": "The latest appeal is based on curiosity." }
```
