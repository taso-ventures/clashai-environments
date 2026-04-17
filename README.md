# clashai-environments

A collection of self-contained game environments with a standard agent protocol. Run the unified server, connect an agent in any language with any LLM or framework, play.

## Included environments

| Game | Players | Docs |
|------|---------|------|
| [Coup](docs/coup.md) | 2–6 | Bluffing + deduction, challenge/block mechanics |
| [Vibe Check](docs/vibe-check.md) | 4+ (teams) | Spectrum-guessing / Wavelength-style |
| [Wordle](docs/wordle.md) | 2+ | Multi-player variant — per-player hidden target, public guess feedback |
| [Tic-Tac-Toe](docs/tic-tac-toe.md) | 2 | Classic 3×3 |
| [Connect Four](docs/connect-four.md) | 2 | 7×6 gravity board |
| [Red Button](docs/red-button.md) | 2 | Asymmetric persuasion |
| [Poker](docs/poker.md) | 2 | Heads-up No-Limit Texas Hold'em |

## Quickstart

```bash
# Build + run the unified server
cargo run --release --bin environment-server

# In another shell, create a Coup match
curl -X POST http://localhost:8080/matches \
  -H 'content-type: application/json' \
  -d '{"environment_type":"coup","player_count":2,"seed":42}'

# Get legal actions for player 0
curl 'http://localhost:8080/matches/<match_id>/legal_actions?player_id=0'

# Submit an action
curl -X POST http://localhost:8080/matches/<match_id>/actions \
  -H 'content-type: application/json' \
  -d '{"player_id":0,"action":{"action_type":"income"}}'

# Spectate in a browser
open http://localhost:8080/viewer/coup.html?matchId=<match_id>
```

Docker:
```bash
docker build -t clashai/environment-server services/environment-server
docker run -p 8080:8080 clashai/environment-server
```

## Protocol

The wire protocol is a small JSON-over-HTTP/WebSocket surface — no auth, no framework lock-in. See [`PROTOCOL.md`](PROTOCOL.md) for the full spec: session lifecycle, action submission, spectator event envelope, and error shape.

## Writing an agent

1. Pick an environment from `docs/` and learn its action schema.
2. Loop: `GET /matches/:id/state?player_id=X` → `GET /matches/:id/legal_actions?player_id=X` → choose → `POST /matches/:id/actions`.
3. Stop when `GET /matches/:id/status` returns `is_terminal: true`.

A reference client in `examples/minimal-client.rs` plays a full game from start to finish in ~120 lines. Any language with an HTTP client works — the server is the authoritative rules engine and handles all legality checks.

## Adding an environment

Each environment has three pieces:

1. `crates/<game>-protocol/` — action and state types. Derive `Serialize` / `Deserialize`. Implement `EnvironmentState`, `EnvironmentAction`, and (if sequential-turn) `SequentialState` from `eval-runtime`.
2. `crates/<game>-engine/` (optional) — pure rules engine if it's big enough to warrant its own crate; otherwise embed as a module in the protocol crate.
3. A feature-gated module in `crates/environment-engine/src/<game>.rs` implementing `Environment` so the unified server picks it up via `environment_type`.
4. A viewer HTML file under `services/environment-server/static/viewer/<game>.html` (optional — text-only clients work fine without it).
5. A `docs/<game>.md` describing rules, action schema, and a wire example.

## Layout

```
crates/
├── eval-runtime/                 # shared TurnInfo + environment traits
├── unified-event-protocol/       # spectator event envelope
├── environment-engine/           # Environment trait + per-game adapters (feature-gated)
├── coup-{protocol,engine}/
├── vibe-check-{protocol,engine}/
├── wordle-protocol/              # embedded engine
├── tic-tac-toe-protocol/         # embedded engine
├── connect-four-protocol/        # embedded engine
├── red-button-protocol/          # embedded engine
└── poker-protocol/               # embedded engine
services/
└── environment-server/           # HTTP/WS server + static 3D viewers
examples/
└── minimal-client.rs             # reference agent loop
docs/
└── <game>.md                     # per-game rules + schema
```

## License

MIT. See [`LICENSE`](LICENSE). See [`CONTRIBUTORS.md`](CONTRIBUTORS.md) for authors and credit to original game designers for non-public-domain games.

## Contributing

- New environments: follow the "Adding an environment" checklist above.
- New viewer: drop an HTML file in `services/environment-server/static/viewer/`; the unified server will route `/viewer/<game>.html` to it.
- Bug fixes to engines: open a PR against `main`; include a regression test in the affected `tests/` directory.
- Clean-room rule on commercial tabletop games: no copyrighted rulebook text verbatim, no trademarked names where avoidable, no shipped game art.
