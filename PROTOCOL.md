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
  "player_ids": ["0", "1"],
  "player_names": {"0": "Alice", "1": "Bob"},
  "extra": {}
}
```

Only `environment_type` is required. Defaults: `player_count=2`, `seed=random`, `match_id=ULID`, `player_ids=["0"..player_count)`, `player_names={}`, `extra={}`. `environment_type` is one of: `coup`, `vibe_check`, `red_button`, `wordle`, `poker`, `tic_tac_toe`, `connect_four`. Player IDs are strings at the server boundary; numeric JSON IDs are accepted for compatibility and normalized to strings.

Response `200`:
```json
{
  "match_id": "01HZ...",
  "spectator_url": "http://localhost:8080/viewer/coup.html?matchId=01HZ...",
  "environment_type": "coup"
}
```

`spectator_url` is `null` for environments that ship without an HTML viewer (currently `poker`, `tic_tac_toe`, `connect_four`, `wordle`). Clients must handle the null case. Adding a viewer is a static-file drop into `services/environment-server/static/viewer/<game>.html` plus a route mapping in the create-match handler.

### `GET /matches/:id/state?player_id=X`

Player-filtered state. Omit `player_id` to receive full state (spectator view — leaks hidden info for games with fog-of-war, so only use server-side or for observer clients).

Response: `{ "state": <environment-specific JSON> }`. See `docs/<game>.md` for per-game state schemas.

### `GET /matches/:id/legal_actions?player_id=X`

Legal actions available to the given player at the current state. The `player_id` query parameter is **required** — omitting it returns `400`. When it's not the given player's turn, the server returns an empty array.

Response: bare JSON array of environment-specific action objects — `[<action>, <action>, ...]`.

### `POST /matches/:id/actions`

Submit an action.

Request:
```json
{
  "player_id": "0",
  "action": { "action_type": "income" }
}
```

The `action` shape is environment-specific. Per-game docs have the full action grammar. The request key is singular (`"action"`, not `"actions"`). Games with multiple action kinds tag them with `"action_type": "<snake_case>"` as the discriminant; single-action games (Tic-Tac-Toe, Connect Four) omit the tag.

Response `200`: `{ "accepted": true, "events": [<event>, ...], "is_terminal": false }` on success. `events` is always a JSON array (possibly empty); single-action games and games whose spectator stream is the source of truth typically return `[]`. Response `400`: `{ "accepted": false, "error": "<reason>" }` for illegal or wrong-actor moves.

### `POST /matches/:id/reasoning`

Optional — forward a sanitized agent rationale or telemetry summary to spectators as a side-channel event. Do not send private model traces or provider-private data. This endpoint does not affect game state.

Request:
```json
{ "player_id": "0", "reasoning": "I am choosing a low-risk opening action." }
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

The `catchup_start` and `catchup_end` markers are bare control frames, **not** `UnifiedEvent` envelopes — they have no `event_id`, `sequence`, `timestamp_ms`, etc. Clients should check for the marker keys (`catchup_start` / `catchup_end`) before attempting to deserialize a frame as a `UnifiedEvent`. The markers let clients know to render replay events without animation and switch to live-mode rendering after `catchup_end`.

The catchup log is capped at the most recent **5 000 events** per match (`EVENT_LOG_CAP` in `services/environment-server/src/lib.rs`). Once a match exceeds that, the oldest events are evicted FIFO and only the tail is replayed to late-joining spectators. The live broadcast stream is unaffected — every event is delivered to subscribers in real time regardless of cap.

After catchup, live events are pushed as they occur. Each event is a `UnifiedEvent` envelope:

```json
{
  "event_id": "01HZ...",
  "sequence": 42,
  "timestamp_ms": 1776342896789,
  "environment_type": "coup",
  "match_id": "01HZ...",
  "event_type": "action",
  "actor": { "player_id": "0", "agent_name": "Alice" },
  "action": { /* environment-specific */ },
  "reasoning": {
    "text": "I am choosing a low-risk opening action.",
    "tokens_in": 123,
    "tokens_out": 45,
    "latency_ms": 900
  },
  "is_terminal": false
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
