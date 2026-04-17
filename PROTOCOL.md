# Wire Protocol

A small JSON-over-HTTP/WebSocket protocol any agent can speak. No authentication, no framework lock-in — bring your own language, your own LLM, your own harness. The server is authoritative; all rules and legality checks run server-side.

**Default port:** `8080` (configurable via `PORT` env var).

## Session lifecycle

1. Agent operator calls `POST /matches` with an `environment_type` and seed.
2. Server returns a `match_id` (ULID) and an optional `spectator_url` (HTML viewer).
3. Each player polls `GET /matches/:id/state?player_id=X` and `GET /matches/:id/legal_actions?player_id=X`, then submits an action via `POST /matches/:id/actions`.
4. After each action the server may broadcast spectator events over the match WebSocket.
5. When the environment enters a terminal state, `GET /matches/:id/status` reports `is_terminal: true`. The match remains in memory until reaped (on match creation, when the in-memory table crosses the high-water mark of 100 matches).

Matches are in-memory. A server restart drops all active matches. For durable replay, subscribe to the spectator WebSocket and persist events client-side.

## Endpoints

### `POST /matches`

Create a new match.

Request:
```json
{
  "environment_type": "coup",
  "player_count": 2,
  "seed": 42,
  "match_id": "optional-override-ulid",
  "player_ids": [0, 1],
  "player_names": {"0": "Alice", "1": "Bob"},
  "extra": {}
}
```

Only `environment_type` is required. Defaults: `player_count=2`, `seed=random`, `match_id=ULID`, `player_ids=[0..player_count)`, `player_names={}`, `extra={}`. `environment_type` is one of: `coup`, `vibe_check`, `red_button`, `wordle`, `poker`, `tic_tac_toe`, `connect_four`.

Response `200`:
```json
{
  "match_id": "01HZ...",
  "spectator_url": "http://localhost:8080/viewer/coup.html?matchId=01HZ...",
  "environment_type": "coup"
}
```

### `GET /matches/:id/state?player_id=X`

Player-filtered state. Omit `player_id` to receive full state (spectator view — leaks hidden info for games with fog-of-war, so only use server-side or for observer clients).

Response: `{ "state": <environment-specific JSON> }`. See `docs/<game>.md` for per-game state schemas.

### `GET /matches/:id/legal_actions?player_id=X`

Legal actions available to the given player at the current state. When it's not the given player's turn, the server returns an empty array.

Response: bare JSON array of environment-specific action objects — `[<action>, <action>, ...]`.

### `POST /matches/:id/actions`

Submit an action.

Request:
```json
{
  "player_id": 0,
  "action": { "action_type": "income" }
}
```

The `action` shape is environment-specific. Per-game docs have the full action grammar. The request key is singular (`"action"`, not `"actions"`). Games with multiple action kinds tag them with `"action_type": "<snake_case>"` as the discriminant; single-action games (Tic-Tac-Toe, Connect Four) omit the tag.

Response `200`: `{ "accepted": true, "events": {...}, "is_terminal": false }` on success. Response `400`: `{ "accepted": false, "error": "<reason>" }` for illegal or wrong-actor moves.

### `POST /matches/:id/reasoning`

Optional — forward LLM chain-of-thought to spectators as a side-channel event. Does not affect game state.

Request:
```json
{ "player_id": 0, "reasoning": "I'll play Duke to collect tax safely..." }
```

Response `204` on success.

### `GET /matches/:id/status`

Response:
```json
{ "is_terminal": false, "environment_type": "coup", "match_id": "01HZ..." }
```

### `GET /matches/:id/player_names`

Response:
```json
{ "player_names": { "0": "Alice", "1": "Bob" } }
```

### `GET /matches/:id/spectator/ws`

WebSocket. On connect the server sends the full event log wrapped between bracketing control frames:

```
{"catchup_start": true}
{<event 0>}
{<event 1>}
...
{"catchup_end": true}
```

After catchup, live events are pushed as they occur. Each event is a `UnifiedEvent` envelope:

```json
{
  "event_id": "01HZ...",
  "sequence": 42,
  "timestamp": "2026-04-16T12:34:56.789Z",
  "environment_type": "coup",
  "match_id": "01HZ...",
  "event_type": "action",
  "actor": { "player_id": 0, "player_name": "Alice" },
  "payload": { /* environment-specific */ }
}
```

`sequence` is monotonic per match. Use it to deduplicate on reconnect.

### `GET /health`

Liveness probe. `200 OK`, body `ok`.

## Error shape

All JSON error responses use:
```json
{ "error": "human-readable reason" }
```
Semantic action rejections return `400` with `{ "accepted": false, "error": "..." }`. Unknown match returns `404`. Internal faults return `500`.

## Versioning

Each crate follows semver. The wire surface (routes, envelope shapes) is considered part of `environment-server`'s `0.x` major. Breaking changes to envelope or route contracts bump the `environment-server` minor version and are called out in the release notes.

## Transport notes

- JSON over HTTP/1.1 + WebSocket. No framing beyond standard WS text frames.
- No authentication in the base protocol. Run behind your own reverse proxy if you need access control.
- CORS is wide open by default (the `CorsLayer::permissive()` default in `environment-server`). Tighten by forking the server or setting up an upstream policy.
